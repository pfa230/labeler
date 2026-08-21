import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import {
  useFavorites,
  useMoveTemplateGroup,
  useRecentTemplates,
  useSetFavorite,
  useTemplates,
} from "../api/queries";
import { useToast } from "../app/toast-context";
import { EmptyTemplates } from "../components/EmptyTemplates";
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
  selected,
  onToggleSelect,
  onToggleFavorite,
  onMove,
}: {
  template: TemplateSummary;
  favorite: boolean;
  selected: boolean;
  onToggleSelect: () => void;
  onToggleFavorite: () => void;
  onMove: () => void;
}) {
  const [failed, setFailed] = useState(false);
  return (
    <div
      className="flex h-full flex-col gap-3 rounded-lg border p-4 transition-shadow hover:shadow-md"
      style={{
        background: selected ? "var(--accent-soft)" : "var(--surface)",
        borderColor: selected ? "var(--accent)" : "var(--border)",
      }}
    >
      <div className="flex items-center justify-between gap-2">
        <label className="flex items-center gap-2 cursor-pointer">
          <input
            type="checkbox"
            checked={selected}
            onChange={onToggleSelect}
            aria-label={`Select ${template.name}`}
            className="h-4 w-4 rounded border-gray-300"
          />
        </label>
        {template.group && (
          <span
            className="truncate rounded-full px-2 py-0.5 text-xs font-medium border"
            style={{ background: "var(--bg)", color: "var(--muted)", borderColor: "var(--border)" }}
          >
            {template.group}
          </span>
        )}
      </div>
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
          <button
            type="button"
            onClick={onMove}
            aria-label={`Move ${template.name}`}
            className="flex h-11 px-2.5 items-center justify-center rounded-md border text-xs font-medium focus-visible:outline-none focus-visible:ring-2"
            style={{
              background: "var(--surface)",
              borderColor: "var(--border)",
              color: "var(--muted)",
            }}
          >
            Move to…
          </button>
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

function MoveDialog({
  templateIds,
  templatesById,
  groupsInUse,
  onClose,
  onSuccess,
}: {
  templateIds: string[];
  templatesById: Map<string, TemplateSummary>;
  groupsInUse: string[];
  onClose: () => void;
  onSuccess: () => void;
}) {
  const [groupInput, setGroupInput] = useState("");
  const moveGroup = useMoveTemplateGroup();
  const { push } = useToast();
  const [submitting, setSubmitting] = useState(false);

  const isBulk = templateIds.length > 1;
  const title = isBulk
    ? `Move ${templateIds.length} templates`
    : `Move ${templatesById.get(templateIds[0])?.name ?? templateIds[0]}`;

  const handleMove = async (targetGroup: string | null) => {
    setSubmitting(true);
    try {
      if (isBulk) {
        const results = await Promise.allSettled(
          templateIds.map((id) => moveGroup.mutateAsync({ id, group: targetGroup })),
        );
        const successes = results.filter((r) => r.status === "fulfilled").length;
        const failures = results
          .map((r, i) => ({ id: templateIds[i], result: r }))
          .filter((x) => x.result.status === "rejected");

        if (failures.length === 0) {
          push({
            kind: "ok",
            message: `Moved ${successes} templates${targetGroup ? ` to ${targetGroup}` : " to ungrouped"}`,
          });
        } else if (successes === 0) {
          push({ kind: "error", message: `Failed to move ${failures.length} templates` });
        } else {
          push({
            kind: "error",
            message: `Moved ${successes} templates. Failed ${failures.length}: ${failures.map((f) => f.id).join(", ")}`,
          });
        }
      } else {
        const id = templateIds[0];
        const tplName = templatesById.get(id)?.name ?? id;
        await moveGroup.mutateAsync({ id, group: targetGroup });
        push({
          kind: "ok",
          message: `Moved ${tplName}${targetGroup ? ` to ${targetGroup}` : " to ungrouped"}`,
        });
      }
      onSuccess();
      onClose();
    } catch (err) {
      push({
        kind: "error",
        message: err instanceof Error ? err.message : "Failed to move template",
      });
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="dialog"
      aria-label={title}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
    >
      <div
        className="flex w-full max-w-md flex-col gap-4 rounded-lg border p-6 shadow-xl"
        style={{ background: "var(--surface)", borderColor: "var(--border)" }}
      >
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold" style={{ color: "var(--ink)" }}>
            {title}
          </h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="rounded p-1 text-sm focus-visible:outline-none focus-visible:ring-2"
            style={{ color: "var(--muted)" }}
          >
            ✕
          </button>
        </div>

        <form
          onSubmit={(e) => {
            e.preventDefault();
            const trimmed = groupInput.trim();
            if (trimmed) {
              void handleMove(trimmed);
            }
          }}
          className="flex flex-col gap-4"
        >
          <div className="flex flex-col gap-1.5">
            <label
              htmlFor="move-group-input"
              className="text-xs font-medium"
              style={{ color: "var(--muted)" }}
            >
              Group name
            </label>
            <input
              id="move-group-input"
              list="groups-datalist"
              type="text"
              value={groupInput}
              onChange={(e) => setGroupInput(e.target.value)}
              placeholder="Choose or enter group…"
              className="rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2"
              style={{ background: "var(--bg)", borderColor: "var(--border)", color: "var(--ink)" }}
              autoFocus
            />
            <datalist id="groups-datalist">
              {groupsInUse.map((g) => (
                <option key={g} value={g} />
              ))}
            </datalist>
          </div>

          <div className="flex flex-wrap items-center justify-between gap-2 pt-2">
            <button
              type="button"
              disabled={submitting}
              onClick={() => void handleMove(null)}
              className="rounded-md border px-3 py-1.5 text-xs font-medium focus-visible:outline-none focus-visible:ring-2 disabled:opacity-50"
              style={{ borderColor: "var(--border)", color: "var(--muted)" }}
            >
              Make ungrouped
            </button>

            <div className="flex items-center gap-2">
              <button
                type="button"
                disabled={submitting}
                onClick={onClose}
                className="rounded-md border px-3 py-1.5 text-xs font-medium focus-visible:outline-none focus-visible:ring-2"
                style={{ borderColor: "var(--border)", color: "var(--ink)" }}
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={submitting || !groupInput.trim()}
                className="rounded-md px-3 py-1.5 text-xs font-medium focus-visible:outline-none focus-visible:ring-2 disabled:opacity-50"
                style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}
              >
                {submitting ? "Moving…" : "Move"}
              </button>
            </div>
          </div>
        </form>
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
  const [selectedGroup, setSelectedGroup] = useState<string>("all");
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [movingTemplateIds, setMovingTemplateIds] = useState<string[] | null>(null);

  useEffect(() => {
    if (isError) {
      push({
        kind: "error",
        message: error instanceof Error ? error.message : "Failed to load templates",
      });
    }
  }, [isError, error, push]);

  const { groupsInUse, hasUngrouped } = useMemo(() => {
    const set = new Set<string>();
    let ungrouped = false;
    for (const t of data?.templates ?? []) {
      if (t.group) {
        set.add(t.group);
      } else {
        ungrouped = true;
      }
    }
    const sorted = Array.from(set).sort(compareCodePoints);
    return { groupsInUse: sorted, hasUngrouped: ungrouped };
  }, [data]);

  const filtered = useMemo(() => {
    let list = data?.templates ?? [];
    if (selectedGroup === "ungrouped") {
      list = list.filter((t) => !t.group);
    } else if (selectedGroup !== "all") {
      list = list.filter((t) => t.group === selectedGroup);
    }
    const needle = query.trim().toLowerCase();
    if (!needle) return list;
    return list.filter(
      (t) => t.id.toLowerCase().includes(needle) || t.name.toLowerCase().includes(needle),
    );
  }, [data, selectedGroup, query]);

  const favoriteIds = favs.data ?? [];
  const isFavorite = (id: string) => favoriteIds.includes(id);
  const toggleFavorite = (id: string) => setFav.mutate({ id, favorite: !isFavorite(id) });

  const toggleSelect = (id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const byId = useMemo(() => {
    const map = new Map<string, TemplateSummary>();
    for (const t of data?.templates ?? []) map.set(t.id, t);
    return map;
  }, [data]);

  const searching = query.trim() !== "";
  const isFiltered = searching || selectedGroup !== "all";
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
      selected={selectedIds.has(t.id)}
      onToggleSelect={() => toggleSelect(t.id)}
      onToggleFavorite={() => toggleFavorite(t.id)}
      onMove={() => setMovingTemplateIds([t.id])}
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
            style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}
          >
            New template
          </Link>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-1.5" role="toolbar" aria-label="Group filter">
        <button
          type="button"
          onClick={() => setSelectedGroup("all")}
          aria-pressed={selectedGroup === "all"}
          className="rounded-full px-3 py-1 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2"
          style={{
            background: selectedGroup === "all" ? "var(--accent)" : "var(--surface)",
            color: selectedGroup === "all" ? "var(--accent-ink, #fff)" : "var(--ink)",
            border: "1px solid",
            borderColor: selectedGroup === "all" ? "var(--accent)" : "var(--border)",
          }}
        >
          All
        </button>
        {groupsInUse.map((g) => (
          <button
            key={g}
            type="button"
            onClick={() => setSelectedGroup(g)}
            aria-pressed={selectedGroup === g}
            className="rounded-full px-3 py-1 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2"
            style={{
              background: selectedGroup === g ? "var(--accent)" : "var(--surface)",
              color: selectedGroup === g ? "var(--accent-ink, #fff)" : "var(--ink)",
              border: "1px solid",
              borderColor: selectedGroup === g ? "var(--accent)" : "var(--border)",
            }}
          >
            {g}
          </button>
        ))}
        {hasUngrouped && (
          <button
            type="button"
            onClick={() => setSelectedGroup("ungrouped")}
            aria-pressed={selectedGroup === "ungrouped"}
            className="rounded-full px-3 py-1 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2"
            style={{
              background: selectedGroup === "ungrouped" ? "var(--accent)" : "var(--surface)",
              color: selectedGroup === "ungrouped" ? "var(--accent-ink, #fff)" : "var(--ink)",
              border: "1px solid",
              borderColor: selectedGroup === "ungrouped" ? "var(--accent)" : "var(--border)",
            }}
          >
            Ungrouped
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
      {data && filtered.length === 0 && (query || selectedGroup !== "all") && (
        <p style={{ color: "var(--muted)" }}>No templates match your search.</p>
      )}
      {data && (data.templates ?? []).length === 0 && !query && selectedGroup === "all" && <EmptyTemplates />}
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

      {selectedIds.size > 0 && (
        <div
          role="region"
          aria-label="Selection actions"
          className="sticky bottom-4 z-40 flex items-center justify-between gap-4 rounded-lg border p-3 shadow-lg"
          style={{ background: "var(--surface)", borderColor: "var(--border)" }}
        >
          <div className="flex items-center gap-3">
            <span className="text-sm font-medium" style={{ color: "var(--ink)" }}>
              {selectedIds.size} selected
            </span>
            <button
              type="button"
              onClick={() => setSelectedIds(new Set())}
              className="text-xs underline focus-visible:outline-none focus-visible:ring-2"
              style={{ color: "var(--muted)" }}
            >
              Clear selection
            </button>
          </div>
          <button
            type="button"
            onClick={() => setMovingTemplateIds(Array.from(selectedIds))}
            className="rounded-md px-3 py-1.5 text-xs font-medium focus-visible:outline-none focus-visible:ring-2"
            style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}
          >
            Move to…
          </button>
        </div>
      )}

      {movingTemplateIds && (
        <MoveDialog
          templateIds={movingTemplateIds}
          templatesById={byId}
          groupsInUse={groupsInUse}
          onClose={() => setMovingTemplateIds(null)}
          onSuccess={() => {
            setSelectedIds(new Set());
          }}
        />
      )}
    </div>
  );
}
