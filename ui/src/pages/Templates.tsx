import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { useFavorites, useRecentTemplates, useSetFavorite, useTemplates } from "../api/queries";
import { useToast } from "../app/toast-context";
import { EmptyTemplates } from "../components/EmptyTemplates";
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

function TemplateCard({
  template,
  favorite,
  onToggleFavorite,
}: {
  template: TemplateSummary;
  favorite: boolean;
  onToggleFavorite: () => void;
}) {
  const [failed, setFailed] = useState(false);
  return (
    // #128: the card is no longer one giant anchor. Interactive controls cannot nest inside an <a>,
    // so when the whole card was the link the ⓘ and ☆ had to be absolutely positioned over it — which
    // put them on top of the thumbnail, the one thing the card exists to show. Linking only the image
    // and title lets the controls sit in normal flow, and drops the absolute/z-index stacking with it.
    <div
      className="flex h-full flex-col gap-3 rounded-lg border p-4 transition-shadow hover:shadow-md"
      style={{ background: "var(--surface)", borderColor: "var(--border)" }}
    >
      <Link
        to={`/print/${encodeURIComponent(template.id)}`}
        aria-label={`Print ${template.name}`}
        className="flex flex-col gap-3 rounded-md focus-visible:outline-none focus-visible:ring-2"
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
        <h2 className="font-semibold" style={{ color: "var(--ink)" }}>
          {template.name}
        </h2>
      </Link>
      <div className="mt-auto flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <FormatBadge type={template.format.type} />
          <code
            className="truncate rounded px-1.5 py-0.5 text-xs"
            style={{ background: "var(--bg)", color: "var(--muted)" }}
          >
            {template.id}
          </code>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <Link
            to={`/templates/${encodeURIComponent(template.id)}`}
            aria-label={`${template.name} template details`}
            className="flex h-11 w-11 items-center justify-center rounded-md border text-sm focus-visible:outline-none focus-visible:ring-2"
            style={{
              background: "var(--surface)",
              borderColor: "var(--border)",
              color: "var(--muted)",
            }}
          >
            ⓘ
          </Link>
          <button
            type="button"
            onClick={onToggleFavorite}
            aria-label={favorite ? `unfavorite ${template.name}` : `favorite ${template.name}`}
            aria-pressed={favorite}
            className="flex h-11 w-11 items-center justify-center rounded-md border text-lg focus-visible:outline-none focus-visible:ring-2"
            style={{
              background: "var(--surface)",
              borderColor: "var(--border)",
              color: favorite ? "var(--accent)" : "var(--muted)",
            }}
          >
            {favorite ? "★" : "☆"}
          </button>
        </div>
      </div>
    </div>
  );
}

export function Templates() {
  const { data, isLoading, isError, error } = useTemplates();
  const favs = useFavorites();
  const recents = useRecentTemplates();
  const setFav = useSetFavorite();
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

  const favoriteIds = favs.data ?? [];
  const isFavorite = (id: string) => favoriteIds.includes(id);
  const toggleFavorite = (id: string) => setFav.mutate({ id, favorite: !isFavorite(id) });

  // Favorites/Recent are keyed by id; resolve against the loaded list and drop unknowns. Recent excludes
  // favorited ids so a card never shows in both rows. Both rows are hidden while the search box is active.
  const byId = useMemo(() => {
    const map = new Map<string, TemplateSummary>();
    for (const t of data?.templates ?? []) map.set(t.id, t);
    return map;
  }, [data]);
  const searching = query.trim() !== "";
  const favTemplates = favoriteIds.map((id) => byId.get(id)).filter((t): t is TemplateSummary => !!t);
  const recentTemplates = (recents.data ?? [])
    .filter((id) => !favoriteIds.includes(id))
    .map((id) => byId.get(id))
    .filter((t): t is TemplateSummary => !!t);

  const cardFor = (t: TemplateSummary) => (
    <TemplateCard
      key={t.id}
      template={t}
      favorite={isFavorite(t.id)}
      onToggleFavorite={() => toggleFavorite(t.id)}
    />
  );

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
      {data && filtered.length === 0 && query && (
        <p style={{ color: "var(--muted)" }}>No templates match your search.</p>
      )}
      {data && (data.templates ?? []).length === 0 && !query && <EmptyTemplates />}
      {!searching && favTemplates.length > 0 && (
        <section aria-label="Favorites" className="flex flex-col gap-2">
          <h2 className="text-sm font-medium" style={{ color: "var(--muted)" }}>
            Favorites
          </h2>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {favTemplates.map(cardFor)}
          </div>
        </section>
      )}

      {!searching && recentTemplates.length > 0 && (
        <section aria-label="Recent" className="flex flex-col gap-2">
          <h2 className="text-sm font-medium" style={{ color: "var(--muted)" }}>
            Recent
          </h2>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {recentTemplates.map(cardFor)}
          </div>
        </section>
      )}

      {filtered.length > 0 && (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {filtered.map(cardFor)}
        </div>
      )}
    </div>
  );
}
