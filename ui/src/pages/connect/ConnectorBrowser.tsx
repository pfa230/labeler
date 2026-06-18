import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  browseConnection,
  type ConnectorSchema,
  type DisplayRow,
  type RelationshipSpec,
  type ResourceSpec,
  type SelectedRow,
} from "../../api/connectors";

const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";
const inputClass = "rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;

export interface ConnectorBrowserProps {
  connectionId: string;
  schema: ConnectorSchema;
  selected: SelectedRow[];
  onSelectedChange: (rows: SelectedRow[]) => void;
}

const refKey = (r: { resource: string; key: string }) => `${r.resource}:${r.key}`;
const MATERIALIZE_CAP = 200;

export function ConnectorBrowser({ connectionId, schema, selected, onSelectedChange }: ConnectorBrowserProps) {
  const [resourceId, setResourceId] = useState(schema.resources[0]?.id ?? "");
  const resource = useMemo<ResourceSpec | undefined>(() => schema.resources.find((r) => r.id === resourceId), [schema, resourceId]);
  const [filterDraft, setFilterDraft] = useState<Record<string, string>>({});
  const [applied, setApplied] = useState<Record<string, string>>({});
  const [parent, setParent] = useState<{ relationship: string; key: string; label: string } | undefined>(undefined);
  const [rows, setRows] = useState<DisplayRow[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const selectedKeys = useMemo(() => new Set(selected.map(refKey)), [selected]);
  const loadedKeys = useMemo(() => new Set(rows.map((r) => refKey(r.id))), [rows]);
  const visibleSelected = selected.filter((s) => loadedKeys.has(refKey(s))).length;
  const hiddenSelected = selected.length - visibleSelected;
  const labelFor = (rid: string) => schema.resources.find((r) => r.id === rid)?.label ?? rid;
  const byResourceCount = (rid: string) => selected.filter((s) => s.resource === rid).length;

  // A monotonic request token shared by the fresh-load effect AND loadMore. Any new request bumps it,
  // so a slower in-flight request (fresh OR append) is dropped once a newer one starts. This is what
  // prevents a stale "Load more" from appending the previous resource's rows after a resource switch /
  // drill / filter change (which would also corrupt the cursor). Every `setState` runs inside the async
  // body so `react-hooks/set-state-in-effect` does not fire (see src/lib/livePreview.ts).
  const reqToken = useRef(0);

  useEffect(() => {
    if (!resource) return;
    const token = ++reqToken.current;
    (async () => {
      setBusy(true);
      setError(null);
      try {
        const page = await browseConnection(connectionId, {
          resource: resource.id,
          ...(Object.keys(applied).length ? { filters: applied } : {}),
          ...(parent ? { parent: { relationship: parent.relationship, key: parent.key } } : {}),
        });
        if (reqToken.current !== token) return;
        setRows(page.rows);
        setCursor(page.next_cursor);
        setHasMore(page.has_more);
      } catch (err) {
        if (reqToken.current === token) setError(err instanceof Error ? err.message : "Browse failed");
      } finally {
        if (reqToken.current === token) setBusy(false);
      }
    })();
  }, [connectionId, resource, applied, parent]);

  const loadMore = async () => {
    if (!resource || !cursor) return;
    const token = ++reqToken.current;
    setBusy(true);
    setError(null);
    try {
      const page = await browseConnection(connectionId, {
        resource: resource.id,
        ...(Object.keys(applied).length ? { filters: applied } : {}),
        ...(parent ? { parent: { relationship: parent.relationship, key: parent.key } } : {}),
        cursor,
      });
      // Drop the append if a newer request (resource switch / fresh reload) has since started.
      if (reqToken.current !== token) return;
      setRows((prev) => [...prev, ...page.rows]);
      setCursor(page.next_cursor);
      setHasMore(page.has_more);
    } catch (err) {
      if (reqToken.current === token) setError(err instanceof Error ? err.message : "Browse failed");
    } finally {
      if (reqToken.current === token) setBusy(false);
    }
  };

  const toggle = useCallback((row: DisplayRow) => {
    const id = refKey(row.id);
    if (selectedKeys.has(id)) {
      onSelectedChange(selected.filter((r) => refKey(r) !== id));
    } else {
      if (selected.length >= MATERIALIZE_CAP) return;
      onSelectedChange([
        ...selected,
        {
          resource: row.id.resource,
          key: row.id.key,
          label: String(row.cells.name ?? row.id.key),
          breadcrumb: row.cells.location != null ? String(row.cells.location) : undefined,
          lastSeen: Date.now(),
        },
      ]);
    }
  }, [selected, selectedKeys, onSelectedChange]);

  const relationshipFrom = (rid: string): RelationshipSpec | undefined => schema.relationships.find((rel) => rel.from === rid);
  const drill = (row: DisplayRow, rel: RelationshipSpec) => {
    setParent({ relationship: rel.id, key: row.id.key, label: String(row.cells.name ?? row.id.key) });
    setResourceId(rel.to);
    setApplied({});
    setFilterDraft({});
  };

  const th = "px-3 py-2 text-left text-xs font-medium";
  const td = "px-3 py-2 text-sm";
  const rel = resource ? relationshipFrom(resource.id) : undefined;

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        {schema.resources.map((r) => (
          <button
            key={r.id}
            type="button"
            onClick={() => { setResourceId(r.id); setParent(undefined); setApplied({}); setFilterDraft({}); }}
            className={`${buttonBase} border`}
            style={{ borderColor: "var(--border)", color: r.id === resourceId ? "var(--accent)" : "var(--ink)", background: r.id === resourceId ? "var(--accent-soft)" : "transparent" }}
          >
            {r.label}
          </button>
        ))}
        {parent && (
          <span className="text-sm" style={{ color: "var(--muted)" }}>
            in {parent.label}{" "}
            <button type="button" className="underline" onClick={() => setParent(undefined)} style={{ color: "var(--ink)" }}>clear</button>
          </span>
        )}
      </div>

      {resource && resource.filters.length > 0 && (
        <div className="flex flex-wrap items-end gap-2">
          {resource.filters.map((f) => (
            <label key={f.key} className="flex flex-col gap-1">
              <span className="text-xs" style={{ color: "var(--muted)" }}>{f.label}</span>
              <input
                aria-label={f.label}
                value={filterDraft[f.key] ?? ""}
                onChange={(e) => setFilterDraft({ ...filterDraft, [f.key]: e.target.value })}
                className={inputClass}
                style={inputStyle}
              />
            </label>
          ))}
          <button
            type="button"
            onClick={() => setApplied(Object.fromEntries(Object.entries(filterDraft).filter(([, v]) => v.trim() !== "")))}
            className={`${buttonBase} border`}
            style={{ borderColor: "var(--border)", color: "var(--ink)" }}
          >
            Apply
          </button>
        </div>
      )}

      {error && <p className="text-sm" style={{ color: "var(--bad)" }}>{error}</p>}
      {busy && rows.length === 0 && <p className="text-sm" style={{ color: "var(--muted)" }}>Loading...</p>}

      {resource && (
        <table className="w-full border-collapse">
          <thead>
            <tr>
              <th className={th} style={{ color: "var(--muted)" }}></th>
              {resource.columns.map((c) => (
                <th key={c.key} className={th} style={{ color: "var(--muted)" }}>{c.label}</th>
              ))}
              {rel && <th className={th} style={{ color: "var(--muted)" }}></th>}
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={refKey(row.id)} style={{ borderTop: "1px solid var(--border)" }}>
                <td className={td}>
                  <input
                    type="checkbox"
                    aria-label={`select ${refKey(row.id)}`}
                    checked={selectedKeys.has(refKey(row.id))}
                    disabled={!selectedKeys.has(refKey(row.id)) && selected.length >= MATERIALIZE_CAP}
                    onChange={() => toggle(row)}
                  />
                </td>
                {resource.columns.map((c) => (
                  <td key={c.key} className={td}>
                    {c.key === "name" && row.url ? (
                      <a href={row.url} target="_blank" rel="noopener" className="underline" style={{ color: "var(--ink)" }}>
                        {row.cells[c.key] ?? ""}
                      </a>
                    ) : (
                      row.cells[c.key] ?? ""
                    )}
                  </td>
                ))}
                {rel && (
                  <td className={td}>
                    <button type="button" className="underline" onClick={() => drill(row, rel)} style={{ color: "var(--ink)" }}>Drill in</button>
                  </td>
                )}
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <div className="flex items-center gap-3">
        {hasMore && (
          <button type="button" disabled={busy} onClick={() => void loadMore()} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
            Load more
          </button>
        )}
      </div>

      {selected.length > 0 && (
        <div className="flex flex-col gap-2 rounded-md border p-3" style={{ borderColor: "var(--border)" }}>
          <div className="flex items-center gap-3 text-sm">
            <span className="font-medium">
              {selected.length}/{MATERIALIZE_CAP} selected ({visibleSelected} in this view, {hiddenSelected} elsewhere)
            </span>
            <button type="button" className="underline" onClick={() => onSelectedChange([])} style={{ color: "var(--ink)" }}>Clear all</button>
            {hiddenSelected > 0 && (
              <button type="button" className="underline" onClick={() => onSelectedChange(selected.filter((s) => loadedKeys.has(refKey(s))))} style={{ color: "var(--ink)" }}>Clear hidden</button>
            )}
          </div>
          {schema.resources.map((r) => byResourceCount(r.id) > 0 ? (
            <div key={r.id} className="flex flex-col gap-1">
              <span className="text-xs" style={{ color: "var(--muted)" }}>{labelFor(r.id)} ({byResourceCount(r.id)})</span>
              <div className="flex flex-wrap gap-2">
                {selected.filter((s) => s.resource === r.id).map((s) => (
                  <span key={refKey(s)} className="inline-flex items-center gap-1 rounded border px-2 py-1 text-xs" style={{ borderColor: "var(--border)" }}>
                    {s.label}{s.breadcrumb ? ` · ${s.breadcrumb}` : ""}
                    <button type="button" aria-label={`remove ${s.label}`} onClick={() => onSelectedChange(selected.filter((x) => refKey(x) !== refKey(s)))} style={{ color: "var(--muted)" }}>×</button>
                  </span>
                ))}
              </div>
            </div>
          ) : null)}
        </div>
      )}
    </div>
  );
}
