import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { useTemplates } from "../api/queries";
import { useToast } from "../app/toast-context";
import type { TemplateSummary } from "../api/types";

function FormatBadge({ type }: { type: string }) {
  return (
    <span
      className="rounded-full px-2 py-0.5 text-xs font-medium"
      style={{ background: "var(--accent-soft)", color: "var(--accent)" }}
    >
      {type}
    </span>
  );
}

function TemplateCard({ template }: { template: TemplateSummary }) {
  const [failed, setFailed] = useState(false);
  return (
    <div className="relative">
      <Link
        to={`/print/${encodeURIComponent(template.id)}`}
        aria-label={`Print ${template.name}`}
        className="flex h-full flex-col gap-3 rounded-lg border p-4 transition-shadow hover:shadow-md focus-visible:outline-none focus-visible:ring-2"
        style={{ background: "var(--surface)", borderColor: "var(--border)" }}
      >
        {failed ? (
          <div
            className="flex aspect-[3/1] items-center justify-center rounded-md border text-xs"
            style={{ background: "var(--bg)", borderColor: "var(--border)", color: "var(--muted)" }}
            aria-hidden="true"
          >
            preview
          </div>
        ) : (
          <img
            src={`/api/templates/${template.id}/thumbnail`}
            alt={`${template.name} preview`}
            loading="lazy"
            onError={() => setFailed(true)}
            className="aspect-[3/1] w-full rounded-md border object-contain"
            style={{ background: "var(--bg)", borderColor: "var(--border)" }}
          />
        )}
        <div className="flex items-center justify-between gap-2">
          <h2 className="font-semibold" style={{ color: "var(--ink)" }}>
            {template.name}
          </h2>
          <FormatBadge type={template.format.type} />
        </div>
        <code
          className="self-start rounded px-1.5 py-0.5 text-xs"
          style={{ background: "var(--bg)", color: "var(--muted)" }}
        >
          {template.id}
        </code>
      </Link>
      <Link
        to={`/templates/${encodeURIComponent(template.id)}`}
        aria-label={`${template.name} template details`}
        className="absolute right-2 top-2 z-10 flex h-11 w-11 items-center justify-center rounded-md border text-sm focus-visible:outline-none focus-visible:ring-2"
        style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--muted)" }}
      >
        ⓘ
      </Link>
    </div>
  );
}

export function Templates() {
  const { data, isLoading, isError, error } = useTemplates();
  const { push } = useToast();
  const [query, setQuery] = useState("");

  useEffect(() => {
    if (isError) {
      push({
        kind: "error",
        message: error instanceof Error ? error.message : "Failed to load templates",
      });
    }
  }, [isError, error, push]);

  const filtered = useMemo(() => {
    const all = data?.templates ?? [];
    const needle = query.trim().toLowerCase();
    if (!needle) return all;
    return all.filter(
      (t) => t.id.toLowerCase().includes(needle) || t.name.toLowerCase().includes(needle),
    );
  }, [data, query]);

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <h1 className="text-2xl font-semibold">Labels</h1>
        <Link
          to="/templates/new"
          className="rounded-md px-3 py-2 text-sm font-medium focus-visible:outline-none focus-visible:ring-2"
          style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}
        >
          New template
        </Link>
      </div>

      <input
        type="search"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Search templates…"
        aria-label="Search templates"
        className="w-full max-w-sm rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2"
        style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" }}
      />

      {isLoading && <p style={{ color: "var(--muted)" }}>loading…</p>}
      {isError && (
        <p style={{ color: "var(--bad)" }}>
          {error instanceof Error ? error.message : "Failed to load templates"}
        </p>
      )}
      {data && filtered.length === 0 && (
        <p style={{ color: "var(--muted)" }}>
          {query ? "No templates match your search." : "No templates available."}
        </p>
      )}
      {filtered.length > 0 && (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {filtered.map((t) => (
            <TemplateCard key={t.id} template={t} />
          ))}
        </div>
      )}
    </div>
  );
}
