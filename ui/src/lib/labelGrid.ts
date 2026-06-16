// Formalized row model for the reusable label grid. CSV is its first consumer (origin "csv");
// M7's Homebox mapping fills origin "connector" + source. See the M5 design spec.
export interface RowSource {
  connector: string;
  connection: string;
  resource: string;
  key: string;
}

export interface LabelGridRow {
  id: string; // stable client row id (survives edits/duplication)
  origin: "csv" | "manual" | "connector";
  source?: RowSource; // set by M7; absent for csv/manual
  data: Record<string, string>; // editable fields
  option: Record<string, string>; // per-row template options
  copyGroup?: string; // links rows produced by a duplicate
  validation: { field?: Record<string, string>; option?: Record<string, string> };
  annotation?: { status: "ok" | "failed"; message?: string }; // from a print summary
}

// The backend caps a batch at 500 labels (413 BatchTooLarge); the grid enforces it client-side.
export const MAX_BATCH_LABELS = 500;

export function newId(): string {
  return crypto.randomUUID();
}

export function expandedCount(rowCount: number, copies: number): number {
  return rowCount * copies;
}

export interface ResolvedLabel {
  data: Record<string, string>;
  option?: Record<string, string>;
}

// Resolve rows to labels for /api/batch: merge manual options under each row's options (row wins),
// omit option entirely when empty (the backend rejects `option` on option-less templates), and
// expand `copies` adjacently (row0 x copies, row1 x copies, ...).
export function resolveLabels(
  rows: LabelGridRow[],
  manualOptions: Record<string, string>,
  copies: number,
): ResolvedLabel[] {
  const out: ResolvedLabel[] = [];
  for (const row of rows) {
    // A row's CSV option value wins over the manual/global default, including a blank value: a blank
    // CSV option cell is treated as invalid (caught by validateOptionCell), not as a fallback to manual.
    const option = { ...manualOptions, ...row.option };
    const label: ResolvedLabel = Object.keys(option).length ? { data: row.data, option } : { data: row.data };
    for (let i = 0; i < copies; i += 1) out.push(label);
  }
  return out;
}

// Map an index in the expanded label array back to its source row index (for annotating failures).
export function sourceRowForExpandedIndex(expandedIndex: number, copies: number): number {
  return Math.floor(expandedIndex / copies);
}

export function duplicateRow(rows: LabelGridRow[], id: string): LabelGridRow[] {
  const i = rows.findIndex((r) => r.id === id);
  if (i === -1) return rows;
  const src = rows[i];
  const group = src.copyGroup ?? newId();
  const copy: LabelGridRow = {
    ...src,
    id: newId(),
    data: { ...src.data },
    option: { ...src.option },
    validation: {},
    annotation: undefined,
    copyGroup: group,
  };
  const next = rows.slice();
  next[i] = { ...src, copyGroup: group };
  next.splice(i + 1, 0, copy);
  return next;
}

export function removeRow(rows: LabelGridRow[], id: string): LabelGridRow[] {
  return rows.filter((r) => r.id !== id);
}

// Validate one option cell against the template's allowed values. `allowed` undefined => the column
// is not a declared option of this template.
export function validateOptionCell(value: string, allowed: string[] | undefined): string | undefined {
  if (allowed === undefined) return "not a declared option";
  if (!allowed.includes(value)) return `value not allowed (expected one of: ${allowed.join(", ")})`;
  return undefined;
}
