import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { useFavorites, useRecentTemplates, useSetFavorite, useTemplates } from "../api/queries";
import { useToast } from "../app/toast-context";
import { EmptyTemplates } from "../components/EmptyTemplates";
import { FormatBadge } from "../components/FormatBadge";
import type { TemplateSummary } from "../api/types";

function compareCodePoints(a: string, b: string): number {
  const ca = Array.from(a);
  const cb = Array.from(b);
  const minLen = Math.min(ca.length, cb.length);
  for (let i = 0; i < minLen; i++) {
    const codeA = ca[i].codePointAt(0)!;
    const codeB = cb[i].codePointAt(0)!;
    if (codeA !== codeB) return codeA - codeB;
  }
  return ca.length - cb.length;
}

// A union rather than a string, so a category literally named "All" or "Uncategorized" stays distinct
// from those two filters.
type CategoryFilter = { kind: "all" } | { kind: "uncategorized" } | { kind: "category"; name: string };

const ALL_FILTER: CategoryFilter = { kind: "all" };

function sameFilter(a: CategoryFilter, b: CategoryFilter): boolean {
  if (a.kind !== b.kind) return false;
  return a.kind !== "category" || b.kind !== "category" || a.name === b.name;
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
    <div
      className="flex h-full flex-col gap-3 rounded-lg border p-4 transition-shadow hover:shadow-md"
      style={{ background: "var(--surface)", borderColor: "var(--border)" }}
    >
      <div className="flex items-center">
        <FormatBadge format={template.format} />
      </div>
      <Link
        to={`/print/${encodeURIComponent(template.id)}`}
        aria-label={`Print ${template.name}`}
        className="flex flex-col gap-3 rounded-md focus-visible:outline-none focus-visible:ring-2"
      >
        {failed ? (
          <div
            className="flex aspect-[3/1] items-center justify-center rounded-md border text-xs"
            style={{ background: "var(--paper)", borderColor: "var(--border)", color: "var(--muted)" }}
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
            style={{ background: "var(--paper)", borderColor: "var(--border)" }}
          />
        )}
        <h2 className="font-semibold" style={{ color: "var(--ink)" }}>
          {template.name}
        </h2>
      </Link>
      <div className="mt-auto flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <code
            className="truncate rounded px-1.5 py-0.5 text-xs"
            style={{ background: "var(--paper)", color: "var(--muted)" }}
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
  const [selectedFilter, setSelectedFilter] = useState<CategoryFilter>(ALL_FILTER);

  useEffect(() => {
    if (isError) {
      push({
        kind: "error",
        message: error instanceof Error ? error.message : "Failed to load templates",
      });
    }
  }, [isError, error, push]);

  const { categories, hasUncategorized } = useMemo(() => {
    const set = new Set<string>();
    let uncategorized = false;
    for (const t of data?.templates ?? []) {
      if (t.categories.length === 0) uncategorized = true;
      for (const c of t.categories) set.add(c);
    }
    return { categories: Array.from(set).sort(compareCodePoints), hasUncategorized: uncategorized };
  }, [data]);

  const filtered = useMemo(() => {
    let list = data?.templates ?? [];
    if (selectedFilter.kind === "uncategorized") {
      list = list.filter((t) => t.categories.length === 0);
    } else if (selectedFilter.kind === "category") {
      list = list.filter((t) => t.categories.includes(selectedFilter.name));
    }
    const needle = query.trim().toLowerCase();
    if (!needle) return list;
    return list.filter(
      (t) => t.id.toLowerCase().includes(needle) || t.name.toLowerCase().includes(needle),
    );
  }, [data, selectedFilter, query]);

  const favoriteIds = favs.data ?? [];
  const isFavorite = (id: string) => favoriteIds.includes(id);
  const toggleFavorite = (id: string) => setFav.mutate({ id, favorite: !isFavorite(id) });

  const byId = useMemo(() => {
    const map = new Map<string, TemplateSummary>();
    for (const t of data?.templates ?? []) map.set(t.id, t);
    return map;
  }, [data]);

  const searching = query.trim() !== "";
  const isFiltered = searching || selectedFilter.kind !== "all";
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
        <div className="flex flex-wrap items-center gap-2">
          <Link
            to="/templates/catalog"
            className="rounded-md border px-3 py-2 text-sm font-medium focus-visible:outline-none focus-visible:ring-2"
            style={{ borderColor: "var(--border)", color: "var(--ink)" }}
          >
            Browse catalog
          </Link>
          <Link
            to="/templates/new"
            className="rounded-md px-3 py-2 text-sm font-medium focus-visible:outline-none focus-visible:ring-2"
            style={{ background: "var(--accent)", color: "var(--accent-ink)" }}
          >
            New template
          </Link>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-1.5" role="toolbar" aria-label="Category filter">
        <button
          type="button"
          onClick={() => setSelectedFilter(ALL_FILTER)}
          aria-pressed={selectedFilter.kind === "all"}
          className="rounded-full px-3 py-1 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2"
          style={{
            background: selectedFilter.kind === "all" ? "var(--accent)" : "var(--surface)",
            color: selectedFilter.kind === "all" ? "var(--accent-ink)" : "var(--ink)",
            border: "1px solid",
            borderColor: selectedFilter.kind === "all" ? "var(--accent)" : "var(--border)",
          }}
        >
          All
        </button>
        {categories.map((c) => (
          <button
            key={c}
            type="button"
            onClick={() => setSelectedFilter({ kind: "category", name: c })}
            aria-pressed={sameFilter(selectedFilter, { kind: "category", name: c })}
            className="rounded-full px-3 py-1 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2"
            style={{
              background: sameFilter(selectedFilter, { kind: "category", name: c })
                ? "var(--accent)"
                : "var(--surface)",
              color: sameFilter(selectedFilter, { kind: "category", name: c })
                ? "var(--accent-ink)"
                : "var(--ink)",
              border: "1px solid",
              borderColor: sameFilter(selectedFilter, { kind: "category", name: c })
                ? "var(--accent)"
                : "var(--border)",
            }}
          >
            {c}
          </button>
        ))}
        {hasUncategorized && (
          <button
            type="button"
            onClick={() => setSelectedFilter({ kind: "uncategorized" })}
            aria-pressed={selectedFilter.kind === "uncategorized"}
            className="rounded-full px-3 py-1 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2"
            style={{
              background: selectedFilter.kind === "uncategorized" ? "var(--accent)" : "var(--surface)",
              color: selectedFilter.kind === "uncategorized" ? "var(--accent-ink)" : "var(--ink)",
              border: "1px solid",
              borderColor: selectedFilter.kind === "uncategorized" ? "var(--accent)" : "var(--border)",
            }}
          >
            Uncategorized
          </button>
        )}
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
      {data && filtered.length === 0 && (query || selectedFilter.kind !== "all") && (
        <p style={{ color: "var(--muted)" }}>No templates match.</p>
      )}
      {data && (data.templates ?? []).length === 0 && !query && selectedFilter.kind === "all" && (
        <EmptyTemplates />
      )}
      {!isFiltered && favTemplates.length > 0 && (
        <section aria-label="Favorites" className="flex flex-col gap-2">
          <h2 className="text-sm font-medium" style={{ color: "var(--muted)" }}>
            Favorites
          </h2>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {favTemplates.map(cardFor)}
          </div>
        </section>
      )}

      {!isFiltered && recentTemplates.length > 0 && (
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
