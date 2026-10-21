import { useEffect, useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import {
  fetchTemplateSource,
  useFavorites,
  useRecentTemplates,
  useReloadTemplates,
  useReplaceTemplate,
  useSetFavorite,
  useTemplates,
} from "../api/queries";
import { editCategories, readCategories } from "../lib/categorySource";
import { useToast } from "../app/toast-context";
import { EmptyTemplates } from "../components/EmptyTemplates";
import { FormatBadge } from "../components/FormatBadge";
import { TemplateThumbnail } from "../components/TemplateThumbnail";
import type { TemplateSummary } from "../api/types";
import { useMainScroll } from "../lib/tabViews";

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
  onEditCategories,
}: {
  template: TemplateSummary;
  favorite: boolean;
  onToggleFavorite: () => void;
  onEditCategories: () => void;
}) {
  return (
    <div
      className="flex h-full flex-col gap-3 rounded-lg border p-4 transition-shadow hover:shadow-md"
      style={{ background: "var(--surface)", borderColor: "var(--border)" }}
    >
      <div className="flex items-center">
        <FormatBadge
          format={
            template.format.type === "sheet"
              ? { type: "sheet", count: template.format.positions.length }
              : { type: "single" }
          }
        />
      </div>
      <Link
        to={`/templates/${encodeURIComponent(template.id)}`}
        aria-label={`Print ${template.name}`}
        className="flex flex-col gap-3 rounded-md focus-visible:outline-hidden focus-visible:ring-2"
      >
        <TemplateThumbnail id={template.id} name={template.name} />
        <h2 className="font-semibold" style={{ color: "var(--ink)" }}>
          {template.name}
        </h2>
      </Link>
      <div className="mt-auto flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <code
            className="truncate rounded-sm px-1.5 py-0.5 text-xs"
            style={{ background: "var(--paper)", color: "var(--muted)" }}
          >
            {template.id}
          </code>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <button
            type="button"
            onClick={onEditCategories}
            aria-label={`Edit categories for ${template.name}`}
            className="rounded-md border px-2 py-1 text-xs focus-visible:outline-none focus-visible:ring-2"
            style={{ borderColor: "var(--border)", color: "var(--ink)" }}
          >
            Edit categories
          </button>
          <button
            type="button"
            onClick={onToggleFavorite}
            aria-label={favorite ? `unfavorite ${template.name}` : `favorite ${template.name}`}
            aria-pressed={favorite}
            className="flex h-11 w-11 items-center justify-center rounded-md border text-lg focus-visible:outline-hidden focus-visible:ring-2"
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

function CategoryEditor({
  id,
  source,
  onClose,
  onSaved,
}: {
  id: string;
  source: string;
  onClose: () => void;
  onSaved: () => void;
}) {
  const parsed = useMemo(() => readCategories(source), [source]);
  const [entries, setEntries] = useState<string[]>(() => (parsed.ok ? parsed.categories : []));
  const [saveError, setSaveError] = useState<string | null>(null);
  const save = useReplaceTemplate();
  const { push } = useToast();

  if (!parsed.ok) {
    return (
      <section
        aria-label={`Edit categories for ${id}`}
        className="flex flex-col gap-3 rounded-lg border p-4"
        style={{ background: "var(--surface)", borderColor: "var(--border)" }}
      >
        <h2 className="text-lg font-semibold">Edit categories for {id}</h2>
        <p style={{ color: "var(--bad)" }}>{parsed.reason}</p>
        <p style={{ color: "var(--ink)" }}>
          Edit this template in the{" "}
          <Link className="underline" to={`/templates/${encodeURIComponent(id)}`}>
            template YAML editor
          </Link>
          .
        </p>
        <div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md px-3 py-2 text-sm"
            style={{ color: "var(--muted)" }}
          >
            Cancel
          </button>
        </div>
      </section>
    );
  }

  // Awaiting the mutation keeps Save's completion handling alive after this editor unmounts: a
  // per-call `mutate` callback is dropped once the observer has no listeners, so closing the editor
  // mid-save would swallow the filter refresh.
  const submit = async () => {
    const result = editCategories(source, entries);
    if (!result.ok) {
      setSaveError(result.reason);
      return;
    }
    try {
      await save.mutateAsync({ id, yaml: result.source });
      push({ kind: "ok", message: `Saved ${id}` });
      onSaved();
    } catch (err) {
      const message = err instanceof Error ? err.message : "Save failed";
      setSaveError(message);
      push({ kind: "error", message });
    }
  };

  return (
    <section
      aria-label={`Edit categories for ${id}`}
      className="flex flex-col gap-3 rounded-lg border p-4"
      style={{ background: "var(--surface)", borderColor: "var(--border)" }}
    >
      <h2 className="text-lg font-semibold">Edit categories for {id}</h2>
      <div className="flex flex-col gap-2">
        {entries.map((entry, index) => (
          <div key={index} className="flex items-center gap-2">
            <input
              type="text"
              value={entry}
              aria-label={`Category ${index + 1}`}
              onChange={(e) => {
                const next = [...entries];
                next[index] = e.target.value;
                setEntries(next);
              }}
              className="w-full max-w-sm rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2"
              style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" }}
            />
            <button
              type="button"
              aria-label={`Remove category ${index + 1}`}
              onClick={() => setEntries(entries.filter((_, i) => i !== index))}
              className="rounded-md px-3 py-2 text-sm"
              style={{ color: "var(--bad)" }}
            >
              Remove
            </button>
          </div>
        ))}
        <div>
          <button
            type="button"
            onClick={() => setEntries([...entries, ""])}
            className="rounded-md px-3 py-2 text-sm"
            style={{ color: "var(--accent)" }}
          >
            Add
          </button>
        </div>
      </div>
      {saveError && <p style={{ color: "var(--bad)" }}>{saveError}</p>}
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          disabled={save.isPending}
          onClick={submit}
          className="rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50"
          style={{ background: "var(--accent)", color: "var(--accent-ink)" }}
        >
          Save
        </button>
        <button
          type="button"
          onClick={onClose}
          className="rounded-md px-3 py-2 text-sm"
          style={{ color: "var(--muted)" }}
        >
          Cancel
        </button>
      </div>
    </section>
  );
}

export function Templates() {
  const { data, isLoading, isError, error } = useTemplates();
  const favs = useFavorites();
  const recents = useRecentTemplates();
  const root = useMainScroll(Boolean(data && favs.data && recents.data));
  const setFav = useSetFavorite();
  const reload = useReloadTemplates();
  const { push } = useToast();
  const qc = useQueryClient();
  const [searchParams, setSearchParams] = useSearchParams();
  const query = searchParams.get("q") ?? "";
  const selectedFilter = useMemo<CategoryFilter>(() => {
    const category = searchParams.get("category");
    if (category) return { kind: "category", name: category };
    return searchParams.get("uncategorized") === "1" ? { kind: "uncategorized" } : ALL_FILTER;
  }, [searchParams]);
  const setQuery = (value: string) => {
    const next = new URLSearchParams(searchParams);
    if (value) next.set("q", value);
    else next.delete("q");
    setSearchParams(next, { replace: true });
  };
  const setSelectedFilter = (filter: CategoryFilter) => {
    const next = new URLSearchParams(searchParams);
    next.delete("category");
    next.delete("uncategorized");
    if (filter.kind === "category") next.set("category", filter.name);
    if (filter.kind === "uncategorized") next.set("uncategorized", "1");
    setSearchParams(next, { replace: true });
  };
  const [editor, setEditor] = useState<{ id: string; source: string } | null>(null);

  const broken = data?.broken ?? [];

  const onReload = async () => {
    try {
      await reload.mutateAsync();
    } catch (err) {
      push({ kind: "error", message: err instanceof Error ? err.message : "Reload failed" });
    }
  };

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

  // A save refetches the summaries asynchronously. Derive the active filter from the refreshed
  // summaries rather than the snapshot captured when Save was clicked, so a choice whose last listing
  // template disappeared falls back to All even when concurrent saves each saw the other still there.
  const activeFilter = useMemo<CategoryFilter>(() => {
    if (!data) return selectedFilter;
    if (selectedFilter.kind === "category" && !categories.includes(selectedFilter.name)) return ALL_FILTER;
    if (selectedFilter.kind === "uncategorized" && !hasUncategorized) return ALL_FILTER;
    return selectedFilter;
  }, [data, selectedFilter, categories, hasUncategorized]);

  // The fallback above only hides a vanished choice for this render. Reset the stored selection too, so
  // a later save that reintroduces that category does not silently reselect it.
  useEffect(() => {
    if (activeFilter === selectedFilter) return;
    const next = new URLSearchParams(searchParams);
    next.delete("category");
    next.delete("uncategorized");
    setSearchParams(next, { replace: true });
  }, [activeFilter, selectedFilter, searchParams, setSearchParams]);

  const filtered = useMemo(() => {
    let list = data?.templates ?? [];
    if (activeFilter.kind === "uncategorized") {
      list = list.filter((t) => t.categories.length === 0);
    } else if (activeFilter.kind === "category") {
      list = list.filter((t) => t.categories.includes(activeFilter.name));
    }
    const needle = query.trim().toLowerCase();
    if (!needle) return list;
    return list.filter(
      (t) => t.id.toLowerCase().includes(needle) || t.name.toLowerCase().includes(needle),
    );
  }, [data, activeFilter, query]);

  const favoriteIds = favs.data ?? [];
  const isFavorite = (id: string) => favoriteIds.includes(id);
  const toggleFavorite = (id: string) => setFav.mutate({ id, favorite: !isFavorite(id) });

  const openEditor = async (id: string) => {
    try {
      const source = await qc.fetchQuery({
        queryKey: ["template-source", id],
        queryFn: () => fetchTemplateSource(id),
      });
      setEditor({ id, source });
    } catch (err) {
      push({
        kind: "error",
        message: err instanceof Error ? err.message : "Failed to load template source",
      });
    }
  };

  const handleSaved = (edited: { id: string; source: string }) => {
    // A save that settles after Cancel must close only the editor instance it belongs to; a later
    // editor for the same template is a different object and must stay open with its own draft.
    setEditor((current) => (current === edited ? null : current));
  };

  const byId = useMemo(() => {
    const map = new Map<string, TemplateSummary>();
    for (const t of data?.templates ?? []) map.set(t.id, t);
    return map;
  }, [data]);

  const searching = query.trim() !== "";
  const isFiltered = searching || activeFilter.kind !== "all";
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
      onEditCategories={() => openEditor(t.id)}
    />
  );

  return (
    <div ref={root} className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <h1 className="text-2xl font-semibold">Labels</h1>
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={onReload}
            disabled={reload.isPending || isLoading}
            className="rounded-md border px-3 py-2 text-sm font-medium focus-visible:outline-none focus-visible:ring-2 disabled:opacity-50"
            style={{ borderColor: "var(--border)", color: "var(--ink)" }}
          >
            Reload templates
          </button>
          <Link
            to="/templates/catalog"
            className="rounded-md border px-3 py-2 text-sm font-medium focus-visible:outline-hidden focus-visible:ring-2"
            style={{ borderColor: "var(--border)", color: "var(--ink)" }}
          >
            Browse catalog
          </Link>
          <Link
            to="/templates/new"
            className="rounded-md px-3 py-2 text-sm font-medium focus-visible:outline-hidden focus-visible:ring-2"
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
          aria-pressed={activeFilter.kind === "all"}
          className="rounded-full px-3 py-1 text-xs font-medium transition-colors focus-visible:outline-hidden focus-visible:ring-2"
          style={{
            background: activeFilter.kind === "all" ? "var(--accent)" : "var(--surface)",
            color: activeFilter.kind === "all" ? "var(--accent-ink)" : "var(--ink)",
            border: "1px solid",
            borderColor: activeFilter.kind === "all" ? "var(--accent)" : "var(--border)",
          }}
        >
          All
        </button>
        {categories.map((c) => (
          <button
            key={c}
            type="button"
            onClick={() => setSelectedFilter({ kind: "category", name: c })}
            aria-pressed={sameFilter(activeFilter, { kind: "category", name: c })}
            className="rounded-full px-3 py-1 text-xs font-medium transition-colors focus-visible:outline-hidden focus-visible:ring-2"
            style={{
              background: sameFilter(activeFilter, { kind: "category", name: c })
                ? "var(--accent)"
                : "var(--surface)",
              color: sameFilter(activeFilter, { kind: "category", name: c })
                ? "var(--accent-ink)"
                : "var(--ink)",
              border: "1px solid",
              borderColor: sameFilter(activeFilter, { kind: "category", name: c })
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
            aria-pressed={activeFilter.kind === "uncategorized"}
            className="rounded-full px-3 py-1 text-xs font-medium transition-colors focus-visible:outline-hidden focus-visible:ring-2"
            style={{
              background: activeFilter.kind === "uncategorized" ? "var(--accent)" : "var(--surface)",
              color: activeFilter.kind === "uncategorized" ? "var(--accent-ink)" : "var(--ink)",
              border: "1px solid",
              borderColor: activeFilter.kind === "uncategorized" ? "var(--accent)" : "var(--border)",
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
        className="w-full max-w-sm rounded-md border px-3 py-2 text-sm focus-visible:outline-hidden focus-visible:ring-2"
        style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" }}
      />

      {editor && (
        <CategoryEditor
          key={editor.id}
          id={editor.id}
          source={editor.source}
          onClose={() => setEditor(null)}
          onSaved={() => handleSaved(editor)}
        />
      )}

      {isLoading && <p style={{ color: "var(--muted)" }}>loading…</p>}
      {isError && (
        <p style={{ color: "var(--bad)" }}>
          {error instanceof Error ? error.message : "Failed to load templates"}
        </p>
      )}
      {data && filtered.length === 0 && (query || activeFilter.kind !== "all") && (
        <p style={{ color: "var(--muted)" }}>No templates match.</p>
      )}
      {data && (data.templates ?? []).length === 0 && !query && activeFilter.kind === "all" && (
        broken.length === 0 ? <EmptyTemplates /> : (
          <p style={{ color: "var(--muted)" }}>No printable templates.</p>
        )
      )}

      {data && broken.length > 0 && (
        <details
          className="rounded-lg border p-4"
          style={{ background: "var(--surface)", borderColor: "var(--border)" }}
        >
          <summary className="cursor-pointer text-sm font-medium" style={{ color: "var(--ink)" }}>
            Refused entries ({broken.length})
          </summary>
          <ul className="mt-3 flex flex-col gap-3">
            {broken.map((b) => (
              <li key={b.path}>
                <code className="text-xs" style={{ color: "var(--ink)" }}>
                  {b.path}
                </code>
                <p className="text-sm" style={{ color: "var(--bad)" }}>
                  {b.error}
                </p>
              </li>
            ))}
          </ul>
        </details>
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
