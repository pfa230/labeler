import type { CellValue, DisplayRow } from "../api/connectors";

// Column key -> filter needle. A needle that's empty, or only whitespace, does not restrict
// that column (treated the same as no filter).
export type ColumnFilters = Record<string, string>;

// Mirrors how the browse table renders a cell (row.cells[key] ?? ""), so filtering can never
// disagree with what's on screen. A number becomes its string form, e.g. 10 matches needle "1".
function displayedCell(row: DisplayRow, key: string): string {
  const value: CellValue | undefined = row.cells[key];
  return value === undefined ? "" : String(value);
}

// Trimmed before matching: a stray leading/trailing space typed into the filter box shouldn't
// stop it matching, and a needle of only spaces reads as "no filter" rather than "match nothing",
// which is the less surprising behavior for a live-as-you-type filter.
function matchesOne(row: DisplayRow, key: string, needle: string): boolean {
  const trimmed = needle.trim();
  if (trimmed === "") return true;
  return displayedCell(row, key).toLowerCase().includes(trimmed.toLowerCase());
}

// AND across columns: a row passes only if every non-empty filter matches it. An absent cell
// displays as "", so it can never contain a non-empty needle and fails that column's filter,
// while an empty filter still passes it.
export function matchesFilters(row: DisplayRow, filters: ColumnFilters): boolean {
  return Object.entries(filters).every(([key, needle]) => matchesOne(row, key, needle));
}
