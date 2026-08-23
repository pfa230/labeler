import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Grid, type ICellProps, type IColumnConfig, type IHeaderCellConfig, type IHeaderCellProps } from "@svar-ui/react-grid";
import "@svar-ui/react-grid/style.css";
import {
  browseConnection,
  type ConnectorSchema,
  type DisplayRow,
  type RelationshipSpec,
  type ResourceSpec,
  type SelectedRow,
} from "../../api/connectors";
import {
  defaultColumnKeys,
  loadSavedColumnKeys,
  saveColumnKeys,
} from "./connectorColumns";
import { compareRowsBy, type SortDirection } from "../../lib/connectorSort";
import { matchesFilters, type ColumnFilters } from "../../lib/connectorFilter";

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

// The row object the grid holds. `id` is the grid's own row identity (svar keys rows by `id`);
// `_row` carries the original DisplayRow so cells never have to reconstruct it from flattened
// cell values. Cell values are also spread in flat (by column key) so the grid's own machinery
// (auto sizing, etc.) sees ordinary column-keyed data, matching the shape svar expects.
interface GridRow {
  id: string;
  _row: DisplayRow;
  [key: string]: unknown;
}

// Shared by every data column: renders the cell's displayed value, and links the name cell to
// the source system when the row carries a url. Has no dependency on component state, so it
// keeps one stable identity across renders rather than being recreated per render.
function NameCell({ row, column }: ICellProps) {
  const displayRow = row._row as DisplayRow;
  const key = String(column.id);
  const value = displayRow.cells[key] ?? "";
  if (key === "name" && displayRow.url) {
    return (
      <a href={displayRow.url} target="_blank" rel="noopener" className="underline" style={{ color: "var(--ink)" }}>
        {value}
      </a>
    );
  }
  return <>{value}</>;
}

export function ConnectorBrowser({ connectionId, schema, selected, onSelectedChange }: ConnectorBrowserProps) {
  const [resourceId, setResourceId] = useState(schema.resources[0]?.id ?? "");
  const resource = useMemo<ResourceSpec | undefined>(() => schema.resources.find((r) => r.id === resourceId), [schema, resourceId]);
  const [filterDraft, setFilterDraft] = useState<Record<string, string>>({});
  const [tags, setTags] = useState<string[]>([]);
  const [pendingTag, setPendingTag] = useState("");
  const [applied, setApplied] = useState<Record<string, import("../../api/connectors").FilterValue>>({});
  const [parent, setParent] = useState<{ relationship: string; key: string; label: string } | undefined>(undefined);
  const [rows, setRows] = useState<DisplayRow[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Ours, not the grid's: see design.md "The grid renders the sort state; it does not own the
  // ordering". `sortState` cycles unsorted -> asc -> desc -> unsorted per column, one column at a
  // time; sorting a different column always replaces it.
  const [sortState, setSortState] = useState<{ key: string; direction: SortDirection } | null>(null);
  const [columnFilters, setColumnFilters] = useState<ColumnFilters>({});

  const [columnOverrides, setColumnOverrides] = useState<Record<string, Set<string>>>({});
  const currentResourceKey = resource ? `${connectionId}:${resource.id}` : "";
  const visibleKeys = useMemo(() => {
    if (!resource) return new Set<string>();
    if (columnOverrides[currentResourceKey]) return columnOverrides[currentResourceKey];
    return loadSavedColumnKeys(connectionId, resource.id, resource.columns);
  }, [connectionId, resource, currentResourceKey, columnOverrides]);

  const [columnsOpen, setColumnsOpen] = useState(false);
  const pickerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!columnsOpen) return;
    const onPointerDown = (e: PointerEvent) => {
      if (pickerRef.current && !pickerRef.current.contains(e.target as Node)) {
        setColumnsOpen(false);
      }
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setColumnsOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [columnsOpen]);

  const setVisibleKeysForCurrent = (next: Set<string>) => {
    if (!resource) return;
    setColumnOverrides((prev) => ({ ...prev, [currentResourceKey]: next }));
    saveColumnKeys(connectionId, resource.id, next);
    // Hiding a column CLEARS its filter rather than parking it: re-showing the column must not
    // silently re-apply a needle the user last typed some columns ago. `activeFilters` already
    // stops a hidden column narrowing anything, so this is about what comes back on re-show.
    // Safe in an event handler; the same prune in an effect would trip react-hooks/set-state-in-effect.
    setColumnFilters((prev) => {
      const kept: ColumnFilters = {};
      for (const [key, value] of Object.entries(prev)) {
        if (next.has(key)) kept[key] = value;
      }
      return Object.keys(kept).length === Object.keys(prev).length ? prev : kept;
    });
  };

  const toggleColumn = (key: string) => {
    if (!resource) return;
    const next = new Set(visibleKeys);
    if (next.has(key)) {
      if (next.size <= 1) return;
      next.delete(key);
    } else {
      next.add(key);
    }
    setVisibleKeysForCurrent(next);
  };

  const showAllColumns = () => {
    if (!resource) return;
    const all = new Set(resource.columns.map((c) => c.key));
    setVisibleKeysForCurrent(all);
  };

  const resetDefaultColumns = () => {
    if (!resource) return;
    const defaults = defaultColumnKeys(resource.columns);
    setVisibleKeysForCurrent(defaults);
  };

  const visibleColumns = useMemo(() => {
    if (!resource) return [];
    const active = resource.columns.filter((c) => visibleKeys.has(c.key));
    if (active.length > 0) return active;
    const defaults = defaultColumnKeys(resource.columns);
    return resource.columns.filter((c) => defaults.has(c.key));
  }, [resource, visibleKeys]);

  const addTag = (raw: string) => {
    const trimmed = raw.trim();
    if (!trimmed) return;
    setTags((prev) => (prev.includes(trimmed) ? prev : [...prev, trimmed]));
    setPendingTag("");
  };

  const removeTag = (tagToRemove: string) => {
    setTags((prev) => prev.filter((t) => t !== tagToRemove));
  };

  const handleApply = () => {
    let currentTags = tags;
    const pendingTrimmed = pendingTag.trim();
    if (pendingTrimmed) {
      currentTags = tags.includes(pendingTrimmed) ? tags : [...tags, pendingTrimmed];
      setTags(currentTags);
      setPendingTag("");
    }
    const next: Record<string, import("../../api/connectors").FilterValue> = {};
    for (const [k, v] of Object.entries(filterDraft)) {
      if (v.trim() !== "") next[k] = v.trim();
    }
    if (currentTags.length > 0) {
      next.tag = currentTags;
    }
    setApplied(next);
  };

  const handleClearFilters = () => {
    setFilterDraft({});
    setTags([]);
    setPendingTag("");
    setApplied({});
  };

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
      setRows([]);
      setCursor(null);
      setHasMore(false);
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
  const drill = useCallback((row: DisplayRow, rel: RelationshipSpec) => {
    setParent({ relationship: rel.id, key: row.id.key, label: String(row.cells.name ?? row.id.key) });
    setResourceId(rel.to);
    setApplied({});
    setFilterDraft({});
    setTags([]);
    setPendingTag("");
    setSortState(null);
    setColumnFilters({});
  }, []);

  const rel = resource ? relationshipFrom(resource.id) : undefined;

  // Filters for a column no longer visible are dropped from what's applied (not from `columnFilters`
  // itself, which is left alone): a hidden column's filter must never keep narrowing the table
  // invisibly, and deriving this on every render, rather than pruning stored state in an effect,
  // needs no extra state and can't fall out of sync with the columns picker.
  const activeFilters = useMemo(() => {
    const visible = new Set(visibleColumns.map((c) => c.key));
    const next: ColumnFilters = {};
    for (const [key, value] of Object.entries(columnFilters)) {
      if (visible.has(key)) next[key] = value;
    }
    return next;
  }, [columnFilters, visibleColumns]);

  // rows -> filter -> compare -> displayed. `rows` itself is never touched: it stays in connector
  // order so "unsorted" always has an order to return to (see design.md "The grid renders the sort
  // state; it does not own the ordering").
  const filteredRows = useMemo(
    () => rows.filter((row) => matchesFilters(row, activeFilters)),
    [rows, activeFilters],
  );
  const comparedRows = useMemo(() => {
    if (!sortState) return filteredRows;
    const field = resource?.columns.find((c) => c.key === sortState.key);
    if (!field) return filteredRows;
    return [...filteredRows].sort(compareRowsBy(field, sortState.direction));
  }, [filteredRows, sortState, resource]);
  const displayedRows = comparedRows;


  // Scope disclosure (design.md "Disclosure copy is specified, not left to taste"): sorting and
  // filtering only ever act on `rows`, the loaded set, never on the connector's full result, so the
  // table says so whenever that scope could otherwise be mistaken for the whole of it. "A filter is
  // active" means a non-empty needle on a visible column, not merely that a filter row exists.
  const filterActive = Object.values(activeFilters).some((v) => v.trim() !== "");
  const loadedCount = rows.length;
  const shownCount = displayedRows.length;
  // When a filter hides every loaded row and more remain to load, that fact replaces the "Showing
  // X of Y" / "more rows loaded so far" pair rather than combining with it: an empty grid alone
  // reads as "no results", not as "narrow further or load more", which is the actual state.
  const noMatchWithMore = filterActive && shownCount === 0 && hasMore;

  // Rows and sort marks are derived together, and that pairing is load-bearing rather than tidiness.
  // `DataStore.init` resets `sortMarks: {}` whenever the data identity changes (grid-store
  // DataStore.ts, the `!isSame(state.data)` branch) and then will not re-apply a `sortMarks` prop it
  // considers unchanged by reference. Deriving the marks separately, keyed only on `sortState`, hands
  // back the same object after a filter edits the row set, so the reset stands: the rows stay
  // correctly sorted while the header claims `aria-sort="none"`, which misleads rather than merely
  // omits. Building both in one memo makes a fresh marks object accompany every fresh row array.
  const { gridRows, sortMarks } = useMemo(() => {
    const nextRows = displayedRows.map((row): GridRow => ({ ...row.cells, id: refKey(row.id), _row: row }));
    return {
      gridRows: nextRows,
      sortMarks: sortState ? { [sortState.key]: { order: sortState.direction } } : {},
    };
  }, [displayedRows, sortState]);

  const SelectCell = useMemo(() => {
    return function SelectCell({ row }: ICellProps) {
      const displayRow = row._row as DisplayRow;
      const id = refKey(displayRow.id);
      const isSelected = selectedKeys.has(id);
      return (
        <input
          type="checkbox"
          aria-label={`select ${id}`}
          checked={isSelected}
          disabled={!isSelected && selected.length >= MATERIALIZE_CAP}
          onChange={() => toggle(displayRow)}
        />
      );
    };
  }, [selectedKeys, selected.length, toggle]);

  const DrillCell = useMemo(() => {
    if (!rel) return undefined;
    return function DrillCell({ row }: ICellProps) {
      const displayRow = row._row as DisplayRow;
      return (
        <button type="button" className="underline" onClick={() => drill(displayRow, rel)} style={{ color: "var(--ink)" }}>
          Drill in
        </button>
      );
    };
  }, [rel, drill]);

  const setColumnFilter = useCallback((key: string, value: string) => {
    setColumnFilters((prev) => ({ ...prev, [key]: value }));
  }, []);

  // FilterCell keeps one stable identity for the component's whole lifetime (it never depends on
  // `columnFilters`): recreating it whenever the filter text changes would make React remount the
  // <input> on every keystroke, dropping focus mid-type. The current value and label instead reach
  // it as ordinary props, threaded through the header row's own config object below (`filterValue`/
  // `filterLabel`, cast back out here) - that config object is free to change every render, since
  // only the `cell` component reference has to stay put for reconciliation to preserve the DOM node.
  const FilterCell = useMemo(() => {
    return function FilterCell({ column, cell }: IHeaderCellProps) {
      const key = String(column.id);
      const { filterValue, filterLabel } = cell as unknown as { filterValue: string; filterLabel: string };
      return (
        <input
          aria-label={`Filter by ${filterLabel}`}
          value={filterValue}
          onChange={(e) => setColumnFilter(key, e.target.value)}
          className={inputClass}
          style={inputStyle}
        />
      );
    };
  }, [setColumnFilter]);

  const columns = useMemo<IColumnConfig[]>(() => {
    // The utility columns get one header row while data columns get two (text + filter). Verified
    // directly against @svar-ui/grid-store's DataStore that this mismatch is fine: normalizeColumns
    // pads a shorter column's header array up to the group's max row count itself, by giving its
    // last row a rowspan and splicing in a hidden filler row, rather than indexing out of bounds.
    const utilityHeader = [{ text: "" }];
    const cols: IColumnConfig[] = [
      { id: "__select", header: utilityHeader, width: 40, cell: SelectCell },
      ...visibleColumns.map((c): IColumnConfig => {
        // Both keys are load-bearing (design.md "Filtering is ours too"): `cell` decides what
        // renders (our input, never the built-in <Filter>), `filter` decides how the header cell
        // behaves - not sortable, no Enter-to-sort, out of tab order, aria-sort="none" - so a click
        // or keypress in the filter box can never sort the column.
        const filterRow: IHeaderCellConfig & { filterValue: string; filterLabel: string } = {
          cell: FilterCell,
          filter: "text",
          filterValue: columnFilters[c.key] ?? "",
          filterLabel: c.label,
        };
        return {
          id: c.key,
          header: [{ text: c.label }, filterRow],
          flexgrow: 1,
          sort: true,
          cell: NameCell,
        };
      }),
    ];
    if (DrillCell) {
      cols.push({ id: "__drill", header: utilityHeader, width: 90, cell: DrillCell });
    }
    return cols;
  }, [visibleColumns, SelectCell, DrillCell, FilterCell, columnFilters]);

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        {schema.resources.map((r) => (
          <button
            key={r.id}
            type="button"
            onClick={() => {
              setResourceId(r.id);
              setParent(undefined);
              setApplied({});
              setFilterDraft({});
              setTags([]);
              setPendingTag("");
              setSortState(null);
              setColumnFilters({});
            }}
            className={`${buttonBase} border`}
            style={{ borderColor: "var(--border)", color: r.id === resourceId ? "var(--accent)" : "var(--ink)", background: r.id === resourceId ? "var(--accent-soft)" : "transparent" }}
          >
            {r.label}
          </button>
        ))}
        {parent && (
          <span className="text-sm" style={{ color: "var(--muted)" }}>
            in {parent.label}{" "}
            <button
              type="button"
              className="underline"
              onClick={() => {
                setParent(undefined);
                setSortState(null);
                setColumnFilters({});
              }}
              style={{ color: "var(--ink)" }}
            >
              clear
            </button>
          </span>
        )}
      </div>

      {resource && (
        <div className="flex flex-wrap items-end justify-between gap-2">
          {resource.filters.length > 0 ? (
            <div className="flex flex-col gap-2">
              <div className="flex flex-wrap items-end gap-2">
                {resource.filters.map((f) => {
                  if (f.key === "tag") {
                    return (
                      <div key={f.key} className="flex flex-col gap-1">
                        <span className="text-xs" style={{ color: "var(--muted)" }}>{f.label}</span>
                        <div className="flex items-center gap-1">
                          <input
                            aria-label={f.label}
                            placeholder="Add tag..."
                            value={pendingTag}
                            onChange={(e) => setPendingTag(e.target.value)}
                            onKeyDown={(e) => {
                              if (e.key === "Enter") {
                                e.preventDefault();
                                addTag(pendingTag);
                              }
                            }}
                            className={inputClass}
                            style={inputStyle}
                          />
                          <button
                            type="button"
                            onClick={() => addTag(pendingTag)}
                            className={`${buttonBase} border`}
                            style={{ borderColor: "var(--border)", color: "var(--ink)" }}
                          >
                            Add
                          </button>
                        </div>
                      </div>
                    );
                  }
                  return (
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
                  );
                })}
                <button
                  type="button"
                  onClick={handleApply}
                  className={`${buttonBase} border`}
                  style={{ borderColor: "var(--border)", color: "var(--ink)" }}
                >
                  Apply
                </button>
                {(Object.keys(applied).length > 0 || Object.keys(filterDraft).length > 0 || tags.length > 0 || pendingTag !== "") && (
                  <button
                    type="button"
                    onClick={handleClearFilters}
                    className={`${buttonBase} border`}
                    style={{ borderColor: "var(--border)", color: "var(--ink)" }}
                  >
                    Clear filters
                  </button>
                )}
              </div>
              {tags.length > 0 && (
                <div className="flex flex-wrap gap-1">
                  {tags.map((tag) => (
                    <span
                      key={tag}
                      className="inline-flex items-center gap-1 rounded border px-2 py-0.5 text-xs"
                      style={{ borderColor: "var(--border)" }}
                    >
                      {tag}
                      <button
                        type="button"
                        aria-label={`Remove tag ${tag}`}
                        onClick={() => removeTag(tag)}
                        style={{ color: "var(--muted)" }}
                      >
                        ×
                      </button>
                    </span>
                  ))}
                </div>
              )}
            </div>
          ) : <div />}

          <div ref={pickerRef} className="relative">
            <button
              type="button"
              onClick={() => setColumnsOpen((prev) => !prev)}
              aria-haspopup="true"
              aria-expanded={columnsOpen}
              aria-label="Customize visible columns"
              className={`${buttonBase} border flex items-center gap-1.5`}
              style={{ borderColor: "var(--border)", color: "var(--ink)", background: "var(--surface)" }}
            >
              <span>Columns ({visibleColumns.length}/{resource.columns.length})</span>
            </button>

            {columnsOpen && (
              <div
                className="absolute right-0 top-full mt-1 z-30 min-w-[220px] max-w-[320px] rounded-md border shadow-lg flex flex-col gap-1 p-2"
                style={{ background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" }}
              >
                <div className="flex items-center justify-between border-b pb-1.5 mb-1" style={{ borderColor: "var(--border)" }}>
                  <span className="text-xs font-semibold" style={{ color: "var(--muted)" }}>Visible Columns</span>
                  <div className="flex items-center gap-2">
                    <button
                      type="button"
                      onClick={showAllColumns}
                      className="text-xs underline hover:opacity-80"
                      style={{ color: "var(--accent)" }}
                    >
                      All
                    </button>
                    <button
                      type="button"
                      onClick={resetDefaultColumns}
                      className="text-xs underline hover:opacity-80"
                      style={{ color: "var(--muted)" }}
                    >
                      Reset
                    </button>
                  </div>
                </div>
                <div className="max-h-64 overflow-y-auto flex flex-col gap-0.5">
                  {resource.columns.map((c) => {
                    const isChecked = visibleKeys.has(c.key);
                    const isOnly = isChecked && visibleKeys.size === 1;
                    return (
                      <label
                        key={c.key}
                        className="flex items-center gap-2 px-2 py-1 rounded text-xs cursor-pointer hover:bg-[var(--accent-soft)]"
                      >
                        <input
                          type="checkbox"
                          checked={isChecked}
                          disabled={isOnly}
                          onChange={() => toggleColumn(c.key)}
                        />
                        <span className="truncate">{c.label}</span>
                      </label>
                    );
                  })}
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {error && <p className="text-sm" style={{ color: "var(--bad)" }}>{error}</p>}
      {busy && rows.length === 0 && <p className="text-sm" style={{ color: "var(--muted)" }}>Loading...</p>}

      {resource && (noMatchWithMore || filterActive || hasMore) && (
        <div role="status" className="text-sm flex flex-col gap-0.5" style={{ color: "var(--muted)" }}>
          {noMatchWithMore ? (
            <p>No loaded row matches. More rows can be loaded.</p>
          ) : (
            <>
              {filterActive && <p>Showing {shownCount} of {loadedCount} loaded rows</p>}
              {hasMore && <p>Sorting and filtering cover only the {loadedCount} rows loaded so far</p>}
            </>
          )}
        </div>
      )}

      {resource && (
        <div className="connector-grid-viewport">
          <Grid
            data={gridRows}
            columns={columns}
            select={false}
            autoRowHeight
            sortMarks={sortMarks}
            init={(api) => {
              // Cancel the grid's own reorder (see design.md "The grid renders the sort state; it
              // does not own the ordering"): returning false from an intercept stops the default
              // sort-rows handler from running, so `data` is never reordered and `sortMarks` is
              // never set by the grid itself. `ev.add` (set on Ctrl/Meta-click) is deliberately
              // never read, so such a click cycles the clicked column exactly like a plain click
              // and can never accumulate a second sorted column.
              api.intercept("sort-rows", (ev) => {
                const key = String(ev.key);
                setSortState((prev) => {
                  if (!prev || prev.key !== key) return { key, direction: "asc" };
                  if (prev.direction === "asc") return { key, direction: "desc" };
                  return null;
                });
                return false;
              });
            }}
          />
        </div>
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
