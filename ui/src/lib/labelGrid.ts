import type { ParamValue } from "../api/types";

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
  data: Record<string, ParamValue>; // editable fields
  copyGroup?: string; // links rows produced by a duplicate
  validation: { field?: Record<string, string> };
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
  data: Record<string, ParamValue>;
}

export function resolveLabels(
  rows: LabelGridRow[],
  copies: number,
): ResolvedLabel[] {
  const out: ResolvedLabel[] = [];
  for (const row of rows) {
    const label: ResolvedLabel = { data: row.data };
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

