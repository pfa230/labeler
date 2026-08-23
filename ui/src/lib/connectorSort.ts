import type { CellValue, DisplayRow, FieldSpec, FieldType } from "../api/connectors";

export type SortDirection = "asc" | "desc";

const ISO_DATE_RE = /^\d{4}-\d{2}-\d{2}$/;
const ISO_DATETIME_RE = /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:?\d{2})?$/;

// A present, non-empty value is interpretable as text/badge; absence and "" are not.
function textKey(value: CellValue | undefined): string | null {
  if (value === undefined) return null;
  const text = typeof value === "number" ? String(value) : value;
  return text.length === 0 ? null : text.toLowerCase();
}

// Number("") is 0, so an empty string must be rejected before the conversion, not after.
// A money cell that arrives as a currency-formatted string (e.g. "$12.50") is deliberately left
// uninterpretable rather than stripped and re-parsed: the spec defines interpretable as "yields a
// finite number", and guessing at currency formatting would be exactly the silent, unreasoned-about
// rule the spec forbids for the date/number case.
function numberKey(value: CellValue | undefined): number | null {
  if (value === undefined) return null;
  if (typeof value === "number") return Number.isFinite(value) ? value : null;
  const trimmed = value.trim();
  if (trimmed.length === 0) return null;
  const n = Number(trimmed);
  return Number.isFinite(n) ? n : null;
}

// Only string cells are considered: a bare number like 2026 is not ISO-8601 text and must not be
// read as a date. The shape is validated before Date.parse is trusted, since Date.parse accepts a
// wide range of non-ISO formats we don't want to treat as interpretable here.
function dateKey(value: CellValue | undefined): number | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  if (!ISO_DATE_RE.test(trimmed) && !ISO_DATETIME_RE.test(trimmed)) return null;
  const ms = Date.parse(trimmed);
  return Number.isNaN(ms) ? null : ms;
}

function keyExtractorFor(ty: FieldType): (value: CellValue | undefined) => string | number | null {
  switch (ty) {
    case "text":
    case "badge":
      return textKey;
    case "number":
    case "money":
      return numberKey;
    case "date":
      return dateKey;
  }
}

// Array.prototype.sort is stable in every engine this app targets, so returning 0 for a tie is
// enough to preserve the connector's order; no index needs threading through.
export function compareRowsBy(
  field: FieldSpec,
  direction: SortDirection,
): (a: DisplayRow, b: DisplayRow) => number {
  const keyOf = keyExtractorFor(field.ty);
  const sign = direction === "asc" ? 1 : -1;

  return (a, b) => {
    const ka = keyOf(a.cells[field.key]);
    const kb = keyOf(b.cells[field.key]);

    // Absent or uninterpretable cells order after every interpretable value, in both directions,
    // so a blank never displaces a real value at the top of the list.
    if (ka === null && kb === null) return 0;
    if (ka === null) return 1;
    if (kb === null) return -1;

    if (ka < kb) return -sign;
    if (ka > kb) return sign;
    return 0;
  };
}
