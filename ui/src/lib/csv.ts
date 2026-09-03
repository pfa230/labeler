// Named import: @types/papaparse has no default export, and this repo's tsconfig sets
// verbatimModuleSyntax with no esModuleInterop, so `import Papa from "papaparse"` fails to build.
import { parse } from "papaparse";

export const MAX_CSV_BYTES = 2_000_000; // ~2 MB guard; label CSVs are small (<= 500 rows after expansion).

export interface CsvRow {
  data: Record<string, string>;
}

export interface CsvParseResult {
  fields: string[]; // data column names
  rows: CsvRow[];
  issues: string[]; // human-readable structural problems
  fatal: boolean; // papaparse reported a malformed-CSV error; the result must not be submitted
}

// Parse a CSV STRING (already read from the file) into labelable rows. papaparse strips a leading
// BOM and handles quoted fields; we add header/raggedness validation.
export function parseCsv(text: string): CsvParseResult {
  const empty: CsvParseResult = { fields: [], rows: [], issues: [], fatal: false };
  if (new Blob([text]).size > MAX_CSV_BYTES) {
    // Blob size is the true UTF-8 byte length (text.length counts UTF-16 code units, not bytes).
    return { ...empty, issues: [`CSV is too large (over ${Math.round(MAX_CSV_BYTES / 1_000_000)} MB).`] };
  }

  // Pin the delimiter to "," so papaparse does not run delimiter auto-detection, which emits a spurious
  // `UndetectableDelimiter` warning for valid single-column CSV (e.g. `sku\n1\n2`) common for label templates.
  const parsed = parse<string[]>(text, { header: false, skipEmptyLines: "greedy", delimiter: "," });
  const grid = parsed.data;
  if (grid.length < 1 || grid[0].length === 0) {
    return { ...empty, issues: ["CSV has no rows."] };
  }

  // Real parse errors (e.g. an unterminated quote) make the result unsubmittable; the delimiter-guess
  // warning is filtered out since we fixed the delimiter above.
  const parseErrors = parsed.errors.filter((e) => e.code !== "UndetectableDelimiter");
  const issues: string[] = parseErrors.map((e) => `CSV parse error${e.row !== undefined ? ` (row ${e.row})` : ""}: ${e.message}`);
  const header = grid[0].map((h) => h.trim());
  const seen = new Set<string>();
  // columns[i] = header name describing how to route cell i; null = skip (empty header).
  const columns = header.map((name) => {
    if (name === "") {
      issues.push("A column has an empty header and is ignored.");
      return null;
    }
    if (seen.has(name)) {
      issues.push(`Duplicate header "${name}"; only the first is used.`);
      return null;
    }
    seen.add(name);
    return name;
  });

  const fields = columns.filter((c): c is string => c !== null);

  if (grid.length < 2) issues.push("CSV has no data rows.");

  const rows: CsvRow[] = [];
  for (let r = 1; r < grid.length; r += 1) {
    const cells = grid[r];
    if (cells.length !== header.length) {
      issues.push(`Row ${r} has ${cells.length} cells but the header has ${header.length}.`);
    }
    const data: Record<string, string> = {};
    columns.forEach((col, i) => {
      if (!col) return;
      data[col] = cells[i] ?? "";
    });
    rows.push({ data });
  }

  return { fields, rows, issues, fatal: parseErrors.length > 0 };
}
