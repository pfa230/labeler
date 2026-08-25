import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { fetchCatalog, fetchCatalogYaml, type CatalogEntry } from "../api/catalog";
import { ApiError } from "../api/client";
import { useCreateTemplate, useReplaceTemplate, useTemplates } from "../api/queries";
import { useToast } from "../app/toast-context";

const buttonBase =
  "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";

/** Pending replace: the entry the user tried to install and the YAML already on disk, for the diff. */
interface Conflict {
  entry: CatalogEntry;
  incoming: string;
  existing: string;
}

function CatalogCard({
  entry,
  installed,
  onInstall,
  busy,
}: {
  entry: CatalogEntry;
  installed: boolean;
  onInstall: () => void;
  busy: boolean;
}) {
  return (
    <div
      className="flex flex-col gap-2 rounded-lg border p-4"
      style={{ background: "var(--surface)", borderColor: "var(--border)" }}
    >
      <div className="flex items-start justify-between gap-2">
        <h3 className="font-semibold" style={{ color: "var(--ink)" }}>
          {entry.name}
        </h3>
        {installed && (
          <span
            className="shrink-0 rounded-full px-2 py-0.5 text-xs font-medium"
            style={{ background: "var(--accent-soft)", color: "var(--accent)" }}
          >
            installed
          </span>
        )}
      </div>
      {entry.description && (
        <p className="text-sm" style={{ color: "var(--muted)" }}>
          {entry.description}
        </p>
      )}
      <dl className="text-xs" style={{ color: "var(--muted)" }}>
        <div className="flex gap-2">
          <dt>format</dt>
          <dd style={{ color: "var(--ink)" }}>{entry.format}</dd>
          {entry.media_width_mm != null && (
            <>
              <dt>media</dt>
              <dd style={{ color: "var(--ink)" }}>{entry.media_width_mm}mm</dd>
            </>
          )}
        </div>
        <div className="flex gap-2">
          <dt>fields</dt>
          <dd style={{ color: "var(--ink)" }}>{entry.fields.join(", ") || "none"}</dd>
        </div>
      </dl>
      <div className="mt-1 flex gap-2">
        <button
          type="button"
          onClick={onInstall}
          disabled={busy}
          className={buttonBase}
          style={{ background: "var(--accent)", color: "var(--accent-ink)" }}
        >
          {installed ? "Reinstall" : "Install"}
        </button>
        {installed && (
          <Link
            to={`/templates/${encodeURIComponent(entry.id)}`}
            className={`${buttonBase} border`}
            style={{ borderColor: "var(--border)", color: "var(--ink)" }}
          >
            View
          </Link>
        )}
      </div>
    </div>
  );
}

export function Catalog() {
  const catalog = useQuery({ queryKey: ["catalog"], queryFn: fetchCatalog, retry: false });
  const installedQuery = useTemplates();
  const create = useCreateTemplate();
  const replace = useReplaceTemplate();
  const { push } = useToast();
  const [conflict, setConflict] = useState<Conflict | null>(null);
  const [busy, setBusy] = useState<ReadonlySet<string>>(new Set());

  const installed = useMemo(
    () => new Set((installedQuery.data?.templates ?? []).map((t) => t.id)),
    [installedQuery.data],
  );

  const grouped = useMemo(() => {
    const groups = new Map<string, CatalogEntry[]>();
    for (const entry of catalog.data ?? []) {
      const key = entry.vendor ? `${entry.category} · ${entry.vendor}` : entry.category;
      groups.set(key, [...(groups.get(key) ?? []), entry]);
    }
    return [...groups.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  }, [catalog.data]);

  const setBusyFor = (id: string, on: boolean) =>
    setBusy((prev) => {
      const next = new Set(prev);
      if (on) {
        next.add(id);
      } else {
        next.delete(id);
      }
      return next;
    });

  const install = async (entry: CatalogEntry) => {
    setBusyFor(entry.id, true);
    let incoming: string | null = null;
    try {
      incoming = await fetchCatalogYaml(entry);
      await create.mutateAsync(incoming);
      push({ kind: "ok", message: `Installed ${entry.id}` });
    } catch (err) {
      // 409 means it is already on disk: offer Replace with a diff rather than silently
      // overwriting. Reuse the YAML already downloaded above — re-fetching here would put a second
      // network call inside the catch, where a failure becomes an unhandled rejection with no toast.
      if (err instanceof ApiError && err.status === 409 && incoming !== null) {
        try {
          const res = await fetch(`/api/templates/${encodeURIComponent(entry.id)}/source`);
          if (!res.ok) {
            throw new Error(`could not read the installed template (${res.status})`, {
              cause: err,
            });
          }
          setConflict({ entry, incoming, existing: await res.text() });
        } catch (readErr) {
          // Showing a diff with a blank "Installed" side would hide the real failure.
          push({
            kind: "error",
            message: readErr instanceof Error ? readErr.message : "Could not compare templates",
          });
        }
      } else if (err instanceof ApiError && err.status === 422) {
        push({
          kind: "error",
          message: `${entry.id} needs a newer version of labeler: ${err.message}`,
        });
      } else {
        push({ kind: "error", message: err instanceof Error ? err.message : "Install failed" });
      }
    } finally {
      setBusyFor(entry.id, false);
    }
  };

  const confirmReplace = async () => {
    if (!conflict) return;
    try {
      await replace.mutateAsync({ id: conflict.entry.id, yaml: conflict.incoming });
      push({ kind: "ok", message: `Replaced ${conflict.entry.id}` });
      setConflict(null);
    } catch (err) {
      push({ kind: "error", message: err instanceof Error ? err.message : "Replace failed" });
    }
  };

  if (catalog.isError) {
    return (
      <section className="flex flex-col gap-3">
        <h1 className="text-lg font-semibold">Template catalog</h1>
        <p className="text-sm" style={{ color: "var(--muted)" }}>
          Couldn&apos;t reach the template catalog. It is fetched from GitHub by your browser, so this
          usually means no internet connection from here.
        </p>
        <Link to="/templates/new" className="text-sm underline" style={{ color: "var(--accent)" }}>
          Paste a template as YAML instead
        </Link>
      </section>
    );
  }

  return (
    <section className="flex flex-col gap-4">
      <div>
        <h1 className="text-lg font-semibold">Template catalog</h1>
        <p className="text-sm" style={{ color: "var(--muted)" }}>
          Install only the templates you need. Installed templates are yours to edit.
        </p>
      </div>

      {catalog.isPending && (
        <p className="text-sm" style={{ color: "var(--muted)" }}>
          Loading catalog…
        </p>
      )}

      {grouped.map(([group, entries]) => (
        <section key={group} aria-label={group} className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold uppercase tracking-wide" style={{ color: "var(--muted)" }}>
            {group}
          </h2>
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {entries.map((entry) => (
              <CatalogCard
                key={entry.id}
                entry={entry}
                installed={installed.has(entry.id)}
                busy={busy.has(entry.id)}
                onInstall={() => void install(entry)}
              />
            ))}
          </div>
        </section>
      ))}

      {conflict && (
        <div
          role="dialog"
          aria-label={`Replace ${conflict.entry.id}?`}
          className="flex flex-col gap-3 rounded-lg border p-4"
          style={{ background: "var(--surface)", borderColor: "var(--border)" }}
        >
          <h2 className="font-semibold">Replace {conflict.entry.id}?</h2>
          <p className="text-sm" style={{ color: "var(--muted)" }}>
            A template with this id is already installed. Replacing overwrites it, including any edits
            you made.
          </p>
          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <h3 className="text-xs font-semibold" style={{ color: "var(--muted)" }}>
                Installed
              </h3>
              <pre className="max-h-64 overflow-auto rounded-md border p-2 text-xs" style={{ borderColor: "var(--border)" }}>
                {conflict.existing}
              </pre>
            </div>
            <div>
              <h3 className="text-xs font-semibold" style={{ color: "var(--muted)" }}>
                From catalog
              </h3>
              <pre className="max-h-64 overflow-auto rounded-md border p-2 text-xs" style={{ borderColor: "var(--border)" }}>
                {conflict.incoming}
              </pre>
            </div>
          </div>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => void confirmReplace()}
              className={buttonBase}
              style={{ background: "var(--accent)", color: "var(--accent-ink)" }}
            >
              Replace
            </button>
            <button
              type="button"
              onClick={() => setConflict(null)}
              className={`${buttonBase} border`}
              style={{ borderColor: "var(--border)", color: "var(--ink)" }}
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
