import { useEffect, useMemo, useState } from "react";
import {
  browseConnection,
  type ConnectorSchema,
  type DisplayRow,
  type RelationshipSpec,
  type ResourceSpec,
  type RowRef,
} from "../../api/connectors";

const buttonBase = "rounded-md px-3 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";
const inputClass = "rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;

export interface ConnectorBrowserProps {
  connectionId: string;
  schema: ConnectorSchema;
  selected: RowRef[];
  onSelectedChange: (refs: RowRef[]) => void;
}

const refKey = (r: RowRef) => `${r.resource}:${r.key}`;

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

  // Fresh page whenever resource, applied filters, or parent changes. The `active` flag drops a stale
  // request's result when the user switches resources/drills before it resolves. `resource` is memoized
  // on resourceId, so this effect does not loop. Every `setState` runs inside the async body so
  // `react-hooks/set-state-in-effect` does not fire (see src/lib/livePreview.ts for the same pattern).
  useEffect(() => {
    if (!resource) return;
    let active = true;
    (async () => {
      setBusy(true);
      setError(null);
      try {
        const page = await browseConnection(connectionId, {
          resource: resource.id,
          ...(Object.keys(applied).length ? { filters: applied } : {}),
          ...(parent ? { parent: { relationship: parent.relationship, key: parent.key } } : {}),
        });
        if (!active) return;
        setRows(page.rows);
        setCursor(page.next_cursor);
        setHasMore(page.has_more);
      } catch (err) {
        if (active) setError(err instanceof Error ? err.message : "Browse failed");
      } finally {
        if (active) setBusy(false);
      }
    })();
    return () => {
      active = false;
    };
  }, [connectionId, resource, applied, parent]);

  const loadMore = async () => {
    if (!resource || !cursor) return;
    setBusy(true);
    setError(null);
    try {
      const page = await browseConnection(connectionId, {
        resource: resource.id,
        ...(Object.keys(applied).length ? { filters: applied } : {}),
        ...(parent ? { parent: { relationship: parent.relationship, key: parent.key } } : {}),
        cursor,
      });
      setRows((prev) => [...prev, ...page.rows]);
      setCursor(page.next_cursor);
      setHasMore(page.has_more);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Browse failed");
    } finally {
      setBusy(false);
    }
  };

  const toggle = (ref: RowRef) => {
    if (selectedKeys.has(refKey(ref))) onSelectedChange(selected.filter((r) => refKey(r) !== refKey(ref)));
    else onSelectedChange([...selected, ref]);
  };

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
                    onChange={() => toggle(row.id)}
                  />
                </td>
                {resource.columns.map((c) => (
                  <td key={c.key} className={td}>{row.cells[c.key] ?? ""}</td>
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
        <span className="text-sm" style={{ color: "var(--muted)" }}>{selected.length} selected</span>
      </div>
    </div>
  );
}
