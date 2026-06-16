# CSV Import Editable Grid Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the M5 CSV Import screen (#24): drop a CSV, review/edit rows in an interactive grid, set options (per-row via CSV `option.<name>` columns, global via a manual strip for options the CSV omits, per spec §4) plus a copies multiplier, and batch print or download via `POST /api/batch`.

**Architecture:** Frontend-only against the existing `/api/batch` endpoint (no backend change). A standalone, reusable `LabelGrid` component (built on `react-data-grid`) operates on a formalized `LabelGridRow` model; the CSV screen is its first consumer and M7's Homebox mapping will be the second. CSV parsing (papaparse), the row/expansion/cap logic, and the grid are separate, independently testable units; the page wires them to the picker, toolbar, and `/api/batch` client.

**Tech Stack:** React 19 + TypeScript, Vite, Tailwind v3, react-router-dom 7, @tanstack/react-query 5, `react-data-grid` 7.x (editable grid), `papaparse` 5.x (CSV), Vitest 4 + React Testing Library.

---

## Context the implementer needs

- **Spec (source of truth):** `docs/superpowers/specs/2026-06-15-m5-web-ui-design.md`, section **§4 "CSV Import — `/import` (#24)"** and **"Reusable label grid + row model"**. Read those two sections before starting.
- **The `/api/batch` contract** (ADR-0011, already shipped): `POST /api/batch { template, labels: [{ data, option? }], mode: "download" | "print", printer?, start_slot? }`.
  - `mode=download` → a binary blob (zip for single, pdf for sheet).
  - `mode=print` → `200` JSON `BatchSummary { total, succeeded, failed: [{ index, error }], jobs }`.
  - A bad row fails the **whole** request with `422 BatchInvalid`, `details.failures: [{ index, code, message }]`, in **both** modes, before anything is produced.
  - `413 BatchTooLarge` if a batch exceeds 500 labels.
- **Client wrappers already exist** in `ui/src/api/client.ts`: `submitBatch(body): Promise<BatchResult>` (discriminates download-blob vs JSON summary), `saveBlob(blob, filename)`, `ApiError` (carries `code`, `status`, `details`). **Reuse them — do not re-implement fetch.**
- **Field/option helpers already exist** in `ui/src/lib/templateFields.ts`: `referencedFields(layout, selected)`, `defaultOptions(options)`. Reuse them.
- **Existing patterns to mirror:**
  - `ui/src/pages/print/PrintForm.tsx` — how a screen resolves labels, calls `submitBatch`, maps `BatchInvalid` failures, toasts via `useToast()`, omits `option` for option-less templates and `start_slot` when 0.
  - `ui/src/pages/Print.test.tsx` — the canonical `stubFetch()` + `MemoryRouter` + `QueryClientProvider` + `ToastProvider` test harness. Copy this structure.
  - `ui/src/api/queries.ts` — `useTemplates()`, `useTemplate(id)`, `usePrinters()`.
- **The toast provider is imported from `../app/toast`** (the `ToastProvider`), context hook `useToast()` from `../app/toast-context`. The `Print.test.tsx` import `import { ToastProvider } from "../app/toast";` is correct.
- **Lint constraints (will fail CI otherwise):** `react-hooks/set-state-in-effect` (no synchronous `setState` in an effect body — only inside async/timer callbacks), `react-hooks/refs` (no `ref.current` read during render), `react-refresh/only-export-components` (a `.tsx` file may only export components; put types/helpers in `.ts` files), `noUnusedLocals`. No `any` (use `unknown` + narrowing). No em dashes in code comments or docs.
- **No backend or API change.** The screen consumes existing endpoints (`/api/batch`, `/api/render/label`), not `/api/import/csv`. SPEC gets only a short clarification note plus a changelog entry distinguishing this client-side screen from the `/api/import/csv` API (Task 6), mirroring how ADR-0013/#20 added a "No API change" changelog line.
- **Branch:** do this work on a short-lived branch `m5-csv-grid` off `main`; the final task merges to `main` and pushes (`Fixes #24`).

## File structure

| File | Responsibility |
| --- | --- |
| `ui/package.json` | Add `react-data-grid`, `papaparse` deps + `@types/papaparse` dev dep. |
| `ui/src/setupTests.ts` (modify) | Add a `ResizeObserver` mock (react-data-grid needs it under jsdom). |
| `ui/src/lib/labelGrid.ts` (create) | `LabelGridRow` model + pure logic: `MAX_BATCH_LABELS`, `expandedCount`, `resolveLabels`, `sourceRowForExpandedIndex`, `duplicateRow`, `removeRow`, `validateOptionCell`. No React. |
| `ui/src/lib/labelGrid.test.ts` (create) | Unit tests for the pure logic. |
| `ui/src/lib/csv.ts` (create) | `parseCsv(text)` → `{ fields, optionColumns, rows, issues }`. Wraps papaparse; splits `option.<name>` columns; flags empty/duplicate headers, ragged rows, oversize input. No React. |
| `ui/src/lib/csv.test.ts` (create) | Unit tests for parsing. |
| `ui/src/components/LabelGrid.tsx` (create) | Reusable editable grid over `LabelGridRow[]` (react-data-grid). Data cells = text editors, option cells = dropdowns, validation styling, annotation column, duplicate/remove actions. |
| `ui/src/components/LabelGrid.test.tsx` (create) | RTL tests: render, edit a data cell, edit an option, duplicate/remove, validation + annotation display. |
| `ui/src/pages/Import.tsx` (replace stub) | The screen: template picker, dropzone/parse, manual-options strip, copies stepper, reset, cap gating, start-slot, Run download/print, summary annotation, error mapping. |
| `ui/src/pages/Import.test.tsx` (create) | Integration tests with stubbed fetch. |
| `docs/adr/0014-csv-import-grid.md` (create) | ADR recording the grid architecture and decisions. |
| `docs/adr/README.md` (modify) | Index row for ADR-0014. |
| `docs/PLAN-phase-1.md` (modify) | Mark P1-54 DONE. |

---

### Task 0: Branch setup

- [ ] **Step 1: Create the short-lived feature branch**

```bash
git checkout main && git pull && git checkout -b m5-csv-grid
```
All subsequent task commits land on `m5-csv-grid`; Task 7 merges it into `main`.

---

### Task 1: Dependencies and test setup

**Files:**
- Modify: `ui/package.json`
- Modify: `ui/src/setupTests.ts`
- Modify: `ui/vite.config.ts`

- [ ] **Step 1: Install the dependencies**

Run (from `ui/`):
```bash
npm install react-data-grid@^7.0.0-beta.59 papaparse@^5.5.3
npm install --save-dev @types/papaparse@^5.3.16
```
`react-data-grid` 7.x declares `react: "^19.2"` as a peer dep (matches this repo). It ships its own types (no `@types/react-data-grid`). papaparse strips a leading BOM automatically and handles quoted fields with embedded commas/newlines.

- [ ] **Step 2: Add the `ResizeObserver` mock to the test setup**

react-data-grid uses `ResizeObserver`, which jsdom does not implement; without this every grid test throws `ResizeObserver is not defined`. Append to `ui/src/setupTests.ts`:

```ts
// react-data-grid uses ResizeObserver for column sizing; jsdom lacks it.
if (!("ResizeObserver" in globalThis)) {
  (globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
}
```

- [ ] **Step 3: Guard react-data-grid's modern CSS under Vite 8**

react-data-grid's `lib/styles.css` uses modern CSS (CSS nesting, `@layer`) that Vite 8's default CSS minifier can mangle. Set a modern CSS target so the production `build` keeps the grid styles intact. In `ui/vite.config.ts`, add `cssTarget: "esnext"` to the existing `build` block:
```ts
  build: { outDir: "dist", cssTarget: "esnext" },
```

- [ ] **Step 4: Verify the existing suite and a production build**

Run (from `ui/`): `npm run test && npm run build`
Expected: tests PASS (deps resolve, setup file loads) and `build` succeeds with no CSS errors. (The build confirms the react-data-grid CSS import resolves and minifies cleanly under Vite 8.)

- [ ] **Step 5: Commit**

```bash
git add ui/package.json ui/package-lock.json ui/src/setupTests.ts ui/vite.config.ts
git commit -m "build(ui): add react-data-grid + papaparse for the CSV grid"
```

---

### Task 2: Row model and pure grid logic

**Files:**
- Create: `ui/src/lib/labelGrid.ts`
- Test: `ui/src/lib/labelGrid.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `ui/src/lib/labelGrid.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import {
  MAX_BATCH_LABELS,
  expandedCount,
  resolveLabels,
  sourceRowForExpandedIndex,
  duplicateRow,
  removeRow,
  validateOptionCell,
  type LabelGridRow,
} from "./labelGrid";

function row(id: string, data: Record<string, string>, option: Record<string, string> = {}): LabelGridRow {
  return { id, origin: "csv", data, option, validation: {} };
}

describe("labelGrid logic", () => {
  it("expandedCount multiplies rows by copies", () => {
    expect(expandedCount(3, 2)).toBe(6);
    expect(expandedCount(0, 5)).toBe(0);
  });

  it("MAX_BATCH_LABELS is the backend cap", () => {
    expect(MAX_BATCH_LABELS).toBe(500);
  });

  it("resolveLabels expands copies adjacently and merges options (row wins over manual)", () => {
    const rows = [row("a", { sku: "1" }, { color: "red" }), row("b", { sku: "2" })];
    const out = resolveLabels(rows, { color: "blue", size: "L" }, 2);
    expect(out).toEqual([
      { data: { sku: "1" }, option: { color: "red", size: "L" } },
      { data: { sku: "1" }, option: { color: "red", size: "L" } },
      { data: { sku: "2" }, option: { color: "blue", size: "L" } },
      { data: { sku: "2" }, option: { color: "blue", size: "L" } },
    ]);
  });

  it("resolveLabels omits option entirely when the merged map is empty", () => {
    const out = resolveLabels([row("a", { sku: "1" })], {}, 1);
    expect(out).toEqual([{ data: { sku: "1" } }]);
    expect("option" in out[0]).toBe(false);
  });

  it("sourceRowForExpandedIndex maps an expanded index back to its source row", () => {
    // 2 rows x 3 copies => [0,0,0,1,1,1]
    expect(sourceRowForExpandedIndex(0, 3)).toBe(0);
    expect(sourceRowForExpandedIndex(2, 3)).toBe(0);
    expect(sourceRowForExpandedIndex(3, 3)).toBe(1);
    expect(sourceRowForExpandedIndex(5, 3)).toBe(1);
  });

  it("duplicateRow inserts a copy right after the source with a new id and shared copyGroup", () => {
    const rows = [row("a", { sku: "1" }), row("b", { sku: "2" })];
    const out = duplicateRow(rows, "a");
    expect(out).toHaveLength(3);
    expect(out[1].id).not.toBe("a");
    expect(out[1].data).toEqual({ sku: "1" });
    expect(out[1].copyGroup).toBeDefined();
    expect(out[0].copyGroup).toBe(out[1].copyGroup);
  });

  it("removeRow drops the row by id", () => {
    const rows = [row("a", { sku: "1" }), row("b", { sku: "2" })];
    expect(removeRow(rows, "a")).toEqual([rows[1]]);
  });

  it("validateOptionCell returns an error for a value not in the allowed set", () => {
    expect(validateOptionCell("red", ["red", "blue"])).toBeUndefined();
    expect(validateOptionCell("green", ["red", "blue"])).toMatch(/not allowed/i);
    // an undeclared option (no allowed list) is flagged
    expect(validateOptionCell("x", undefined)).toMatch(/not a declared option/i);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run (from `ui/`): `npm run test -- labelGrid`
Expected: FAIL ("Cannot find module './labelGrid'").

- [ ] **Step 3: Implement `ui/src/lib/labelGrid.ts`**

```ts
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
```

- [ ] **Step 4: Run to verify it passes**

Run (from `ui/`): `npm run test -- labelGrid`
Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
git add ui/src/lib/labelGrid.ts ui/src/lib/labelGrid.test.ts
git commit -m "feat(ui): add LabelGridRow model and pure grid logic"
```

---

### Task 3: CSV parsing

**Files:**
- Create: `ui/src/lib/csv.ts`
- Test: `ui/src/lib/csv.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `ui/src/lib/csv.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import { parseCsv, MAX_CSV_BYTES } from "./csv";

describe("parseCsv", () => {
  it("splits data fields from option.<name> columns and maps each row", () => {
    const r = parseCsv("sku,name,option.color\n1,Widget,red\n2,Gadget,blue\n");
    expect(r.fields).toEqual(["sku", "name"]);
    expect(r.optionColumns).toEqual(["color"]);
    expect(r.rows).toEqual([
      { data: { sku: "1", name: "Widget" }, option: { color: "red" } },
      { data: { sku: "2", name: "Gadget" }, option: { color: "blue" } },
    ]);
    expect(r.issues).toEqual([]);
  });

  it("strips a leading BOM from the first header", () => {
    const r = parseCsv("﻿sku,name\n1,Widget\n");
    expect(r.fields).toEqual(["sku", "name"]);
  });

  it("handles quoted fields with embedded commas", () => {
    const r = parseCsv('sku,name\n1,"Widget, large"\n');
    expect(r.rows[0].data.name).toBe("Widget, large");
  });

  it("flags empty and duplicate headers", () => {
    const r = parseCsv("sku,,sku\n1,2,3\n");
    expect(r.issues.join(" ")).toMatch(/empty header/i);
    expect(r.issues.join(" ")).toMatch(/duplicate header/i);
  });

  it("flags a ragged row but still maps the present cells", () => {
    const r = parseCsv("sku,name\n1\n");
    expect(r.issues.join(" ")).toMatch(/row 1/i);
    expect(r.rows[0].data).toEqual({ sku: "1", name: "" });
  });

  it("returns an error issue and no rows for an empty document", () => {
    const r = parseCsv("");
    expect(r.rows).toEqual([]);
    expect(r.issues.join(" ")).toMatch(/no rows|empty/i);
  });

  it("flags a header-only CSV with no data rows", () => {
    const r = parseCsv("sku,name\n");
    expect(r.rows).toEqual([]);
    expect(r.issues.join(" ")).toMatch(/no data rows/i);
  });

  it("marks malformed CSV (unterminated quote) as fatal", () => {
    // papaparse emits a "Quoted field unterminated" error for an opened-but-unclosed quote.
    const r = parseCsv('sku\n"open');
    expect(r.fatal).toBe(true);
    expect(r.issues.join(" ")).toMatch(/parse error/i);
  });

  it("parses a valid single-column CSV without marking it fatal", () => {
    // A single-column CSV must not trip papaparse's delimiter-guess warning (delimiter is pinned to ",").
    const r = parseCsv("sku\n1\n2\n");
    expect(r.fatal).toBe(false);
    expect(r.rows).toEqual([
      { data: { sku: "1" }, option: {} },
      { data: { sku: "2" }, option: {} },
    ]);
  });

  it("rejects input larger than the size cap", () => {
    const big = "a,b\n" + "x,y\n".repeat(MAX_CSV_BYTES);
    const r = parseCsv(big);
    expect(r.issues.join(" ")).toMatch(/too large/i);
    expect(r.rows).toEqual([]);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run (from `ui/`): `npm run test -- csv`
Expected: FAIL ("Cannot find module './csv'").

- [ ] **Step 3: Implement `ui/src/lib/csv.ts`**

```ts
// Named import: @types/papaparse has no default export, and this repo's tsconfig sets
// verbatimModuleSyntax with no esModuleInterop, so `import Papa from "papaparse"` fails to build.
import { parse } from "papaparse";

export const MAX_CSV_BYTES = 2_000_000; // ~2 MB guard; label CSVs are small (<= 500 rows after expansion).
const OPTION_PREFIX = "option.";

export interface CsvRow {
  data: Record<string, string>;
  option: Record<string, string>;
}

export interface CsvParseResult {
  fields: string[]; // data column names (headers without the option. prefix)
  optionColumns: string[]; // option names (from `option.<name>` headers)
  rows: CsvRow[];
  issues: string[]; // human-readable structural problems
  fatal: boolean; // papaparse reported a malformed-CSV error; the result must not be submitted
}

// Parse a CSV STRING (already read from the file) into labelable rows. papaparse strips a leading
// BOM and handles quoted fields; we add header/raggedness validation and the option.<name> split.
export function parseCsv(text: string): CsvParseResult {
  const empty: CsvParseResult = { fields: [], optionColumns: [], rows: [], issues: [], fatal: false };
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
  // columns[i] = { name, kind } describing how to route cell i; null = skip (empty header).
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
    return name.startsWith(OPTION_PREFIX)
      ? { name: name.slice(OPTION_PREFIX.length), kind: "option" as const }
      : { name, kind: "field" as const };
  });

  const fields = columns.filter((c): c is { name: string; kind: "field" } => c?.kind === "field").map((c) => c.name);
  const optionColumns = columns
    .filter((c): c is { name: string; kind: "option" } => c?.kind === "option")
    .map((c) => c.name);

  if (grid.length < 2) issues.push("CSV has no data rows.");

  const rows: CsvRow[] = [];
  for (let r = 1; r < grid.length; r += 1) {
    const cells = grid[r];
    if (cells.length !== header.length) {
      issues.push(`Row ${r} has ${cells.length} cells but the header has ${header.length}.`);
    }
    const data: Record<string, string> = {};
    const option: Record<string, string> = {};
    columns.forEach((col, i) => {
      if (!col) return;
      const value = cells[i] ?? "";
      if (col.kind === "field") data[col.name] = value;
      else option[col.name] = value;
    });
    rows.push({ data, option });
  }

  return { fields, optionColumns, rows, issues, fatal: parseErrors.length > 0 };
}
```

> **Decision (recorded in ADR-0014):** the M5 design spec mentioned "parsed off the main thread for large files," but the screen rejects CSVs over the 500-row / 2 MB cap at load (Task 5), so the grid never parses a large file. A bounded synchronous parse is therefore correct and worker offloading (`papaparse` `worker: true`) is unnecessary. This consciously supersedes the spec's off-main-thread aspiration; ADR-0014 and the SPEC changelog record it so the docs agree.
>
> **Decision:** empty/duplicate headers and ragged rows are surfaced as `issues` (flagged), not blocked, exactly as spec §4 requires ("flagged in the UI"). The dangerous case (a missing *required* field from a short row) is independently caught by required-field validation, which disables Run. Only papaparse-level malformed CSV (the `fatal` flag) blocks load.

- [ ] **Step 4: Run to verify it passes**

Run (from `ui/`): `npm run test -- csv`
Expected: PASS (10 tests).

- [ ] **Step 5: Commit**

```bash
git add ui/src/lib/csv.ts ui/src/lib/csv.test.ts
git commit -m "feat(ui): add client-side CSV parsing with option-column split"
```

---

### Task 4: Reusable LabelGrid component

**Files:**
- Create: `ui/src/components/LabelGrid.tsx`
- Test: `ui/src/components/LabelGrid.test.tsx`

This component is standalone (not private to the CSV screen) so M7's Homebox mapping reuses it. It renders `LabelGridRow[]` with editable data cells (text), option cells (dropdowns of allowed values), per-cell validation styling, an annotation column, and per-row duplicate/remove actions. Virtualization is disabled (`enableVirtualization={false}`): the 500-row cap makes it unnecessary, and it keeps the grid testable under jsdom (which reports 0 layout height). Exporting `LabelGridProps` (a type) from this `.tsx` is lint-clean: `react-refresh/only-export-components` ignores type-only exports, and `FieldForm.tsx` already exports `type FormValue` the same way. Columns are memoized with `useMemo` and `rowKeyGetter` is a stable module-level constant, the idiomatic react-data-grid pattern that avoids recalculating columns on every render.

- [ ] **Step 1: Write the failing test**

Create `ui/src/components/LabelGrid.test.tsx`:
```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { LabelGrid } from "./LabelGrid";
import type { LabelGridRow } from "../lib/labelGrid";

function rows(): LabelGridRow[] {
  return [
    { id: "a", origin: "csv", data: { sku: "1" }, option: { color: "red" }, validation: {} },
    {
      id: "b",
      origin: "csv",
      data: { sku: "2" },
      option: { color: "green" },
      validation: { option: { color: "value not allowed" } },
      annotation: { status: "failed", message: "boom" },
    },
  ];
}

const props = {
  fields: ["sku"],
  optionNames: ["color"],
  optionValues: { color: ["red", "blue"] },
};

describe("LabelGrid", () => {
  it("renders data and option cell values", () => {
    render(<LabelGrid rows={rows()} {...props} onRowsChange={() => {}} onDuplicate={() => {}} onRemove={() => {}} />);
    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.getByText("red")).toBeInTheDocument();
  });

  it("shows the annotation message for a failed row", () => {
    render(<LabelGrid rows={rows()} {...props} onRowsChange={() => {}} onDuplicate={() => {}} onRemove={() => {}} />);
    expect(screen.getByText(/boom/)).toBeInTheDocument();
  });

  it("shows validation errors: an invalid option and an empty required field", () => {
    const rs: LabelGridRow[] = [
      { id: "a", origin: "csv", data: { sku: "" }, option: { color: "red" }, validation: { field: { sku: "required" } } },
      { id: "b", origin: "csv", data: { sku: "2" }, option: { color: "green" }, validation: { option: { color: "value not allowed" } } },
    ];
    render(<LabelGrid rows={rs} {...props} onRowsChange={() => {}} onDuplicate={() => {}} onRemove={() => {}} />);
    expect(screen.getByLabelText(/sku required/i)).toBeInTheDocument();
    expect(screen.getByTitle(/value not allowed/i)).toBeInTheDocument();
  });

  it("calls onDuplicate and onRemove with the row id", () => {
    const onDuplicate = vi.fn();
    const onRemove = vi.fn();
    render(<LabelGrid rows={rows()} {...props} onRowsChange={() => {}} onDuplicate={onDuplicate} onRemove={onRemove} />);
    fireEvent.click(screen.getAllByRole("button", { name: /duplicate/i })[0]);
    fireEvent.click(screen.getAllByRole("button", { name: /remove/i })[0]);
    expect(onDuplicate).toHaveBeenCalledWith("a");
    expect(onRemove).toHaveBeenCalledWith("a");
  });

  it("commits a nested data-cell edit through onRowsChange", async () => {
    const onRowsChange = vi.fn();
    render(<LabelGrid rows={rows()} {...props} onRowsChange={onRowsChange} onDuplicate={() => {}} onRemove={() => {}} />);
    // Double-click the cell to enter edit mode (react-data-grid default), then change the input.
    fireEvent.doubleClick(screen.getByText("1"));
    const input = (await screen.findByLabelText("edit sku")) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "9" } });
    fireEvent.blur(input);
    await waitFor(() => expect(onRowsChange).toHaveBeenCalled());
    const updated = onRowsChange.mock.calls.at(-1)![0] as LabelGridRow[];
    expect(updated[0].data.sku).toBe("9");
  });

  it("commits an option-cell edit (dropdown) through onRowsChange", async () => {
    const onRowsChange = vi.fn();
    render(<LabelGrid rows={rows()} {...props} onRowsChange={onRowsChange} onDuplicate={() => {}} onRemove={() => {}} />);
    fireEvent.doubleClick(screen.getByText("red"));
    const select = (await screen.findByLabelText("edit color")) as HTMLSelectElement;
    fireEvent.change(select, { target: { value: "blue" } });
    await waitFor(() => expect(onRowsChange).toHaveBeenCalled());
    const updated = onRowsChange.mock.calls.at(-1)![0] as LabelGridRow[];
    expect(updated[0].option.color).toBe("blue");
  });
});
```

> If react-data-grid does not open the editor on `doubleClick` under jsdom, select the cell first with a `click`, then press Enter (`fireEvent.keyDown(cell, { key: "Enter" })`) to enter edit mode. The select commits immediately via `onRowChange(row, true)`; the text input commits on `blur` via `onClose(true)`. Both surface through `onRowsChange`.

- [ ] **Step 2: Run to verify it fails**

Run (from `ui/`): `npm run test -- LabelGrid`
Expected: FAIL ("Cannot find module './LabelGrid'").

- [ ] **Step 3: Implement `ui/src/components/LabelGrid.tsx`**

```tsx
import "react-data-grid/lib/styles.css";
import { useMemo } from "react";
import { DataGrid, type Column, type RenderEditCellProps, type RenderCellProps, type RowsChangeData } from "react-data-grid";
import type { LabelGridRow } from "../lib/labelGrid";

const rowKeyGetter = (r: LabelGridRow) => r.id; // stable module-level identity (avoids grid recalculation)

export interface LabelGridProps {
  rows: LabelGridRow[];
  fields: string[];
  optionNames: string[];
  optionValues: Record<string, string[]>; // allowed values per declared option
  // RDG passes the full updated rows plus which indexes changed, so the caller can normalize edited rows.
  onRowsChange: (rows: LabelGridRow[], data: RowsChangeData<LabelGridRow>) => void;
  onDuplicate: (id: string) => void;
  onRemove: (id: string) => void;
  disabled?: boolean; // read-only while a batch is in flight (no editing/duplicate/remove)
}

const cellErrorStyle = { color: "var(--bad)" } as const;
// Namespaced column keys so a CSV/template field literally named "actions"/"annotation"/"data:x"
// cannot collide with the grid's own columns. Keys are decoded back to field/option names in the cells.
const DATA_PREFIX = "data:";
const OPTION_PREFIX = "option:";

function DataEditCell({ row, column, onRowChange, onClose }: RenderEditCellProps<LabelGridRow>) {
  const field = column.key.slice(DATA_PREFIX.length);
  return (
    <input
      autoFocus
      aria-label={`edit ${field}`}
      className="w-full bg-transparent px-2"
      value={row.data[field] ?? ""}
      onChange={(e) => onRowChange({ ...row, data: { ...row.data, [field]: e.target.value } })}
      onBlur={() => onClose(true)}
    />
  );
}

function OptionEditCell(
  { row, column, onRowChange }: RenderEditCellProps<LabelGridRow>,
  allowed: string[],
) {
  const name = column.key.slice(OPTION_PREFIX.length);
  const value = row.option[name] ?? "";
  // Render the current value even if it is not allowed, so an invalid CSV value stays selectable/visible.
  const options = allowed.includes(value) ? allowed : [value, ...allowed];
  return (
    <select
      autoFocus
      aria-label={`edit ${name}`}
      className="w-full bg-transparent px-2"
      value={value}
      onChange={(e) => onRowChange({ ...row, option: { ...row.option, [name]: e.target.value } }, true)}
    >
      {options.map((v) => (
        <option key={v} value={v}>
          {v === "" ? "(none)" : v}
        </option>
      ))}
    </select>
  );
}

export function LabelGrid({ rows, fields, optionNames, optionValues, onRowsChange, onDuplicate, onRemove, disabled }: LabelGridProps) {
  // Memoized so react-data-grid does not recalculate columns on every render (it keys off array identity).
  const columns = useMemo<Column<LabelGridRow>[]>(() => [
    ...fields.map<Column<LabelGridRow>>((field) => ({
      key: `${DATA_PREFIX}${field}`,
      name: field,
      renderCell: ({ row }: RenderCellProps<LabelGridRow>) => {
        const err = row.validation.field?.[field];
        const value = row.data[field] ?? "";
        // An empty required field renders an explicit, accessible marker (not just a tooltip on empty text).
        if (err && value === "") {
          return (
            <span style={cellErrorStyle} aria-label={`${field} ${err}`} title={err}>
              ⚠ {err}
            </span>
          );
        }
        return <span style={err ? cellErrorStyle : undefined} title={err}>{value}</span>;
      },
      renderEditCell: disabled ? undefined : DataEditCell,
    })),
    ...optionNames.map<Column<LabelGridRow>>((name) => ({
      key: `${OPTION_PREFIX}${name}`,
      name: `option.${name}`,
      renderCell: ({ row }: RenderCellProps<LabelGridRow>) => {
        const err = row.validation.option?.[name];
        return <span style={err ? cellErrorStyle : undefined} title={err}>{row.option[name] ?? ""}</span>;
      },
      renderEditCell: disabled ? undefined : (p: RenderEditCellProps<LabelGridRow>) => OptionEditCell(p, optionValues[name] ?? []),
    })),
    {
      key: "__annotation",
      name: "Status",
      renderCell: ({ row }: RenderCellProps<LabelGridRow>) => {
        if (!row.annotation) return null;
        const ok = row.annotation.status === "ok";
        return (
          <span style={{ color: ok ? "var(--ok, green)" : "var(--bad)" }}>
            {ok ? "ok" : `failed: ${row.annotation.message ?? ""}`}
          </span>
        );
      },
    },
    {
      key: "__actions",
      name: "",
      width: 110,
      renderCell: ({ row }: RenderCellProps<LabelGridRow>) => (
        <span className="flex gap-2">
          <button type="button" aria-label="duplicate row" disabled={disabled} onClick={() => onDuplicate(row.id)}>
            ⧉
          </button>
          <button type="button" aria-label="remove row" disabled={disabled} onClick={() => onRemove(row.id)}>
            ✕
          </button>
        </span>
      ),
    },
  ], [fields, optionNames, optionValues, onDuplicate, onRemove, disabled]);

  return (
    <DataGrid
      aria-label="label rows"
      columns={columns}
      rows={rows}
      rowKeyGetter={rowKeyGetter}
      onRowsChange={onRowsChange}
      enableVirtualization={false}
    />
  );
}
```

> If react-data-grid's edit cell does not commit a `<select>` change reliably under the test, the `onRowChange(..., true)` second argument forces an immediate commit (verified API). The data text cell commits on blur via `onClose(true)`.

- [ ] **Step 4: Run to verify it passes**

Run (from `ui/`): `npm run test -- LabelGrid`
Expected: PASS (6 tests). If a test cannot find a rendered cell, confirm `enableVirtualization={false}` is set and the `ResizeObserver` mock from Task 1 is in `setupTests.ts`.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/LabelGrid.tsx ui/src/components/LabelGrid.test.tsx
git commit -m "feat(ui): add reusable LabelGrid component (react-data-grid)"
```

---

### Task 5: CSV Import page

**Files:**
- Replace: `ui/src/pages/Import.tsx`
- Test: `ui/src/pages/Import.test.tsx`

The page composes everything: pick a template, drop a CSV, review/edit in the grid, set manual options + copies, and Run (download or print). It mirrors `PrintForm.tsx` for the `/api/batch` call shape, option-omission, `start_slot` handling, `BatchInvalid` mapping, and toasts.

- [ ] **Step 1: Write the failing integration test**

Create `ui/src/pages/Import.test.tsx`:
```tsx
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "../app/toast";
import { Import } from "./Import";

const detail = {
  id: "t1",
  name: "Tag",
  description: "",
  unit: "mm",
  dpi: 300,
  format: { type: "single", width: 80, height: 24 },
  options: { color: ["red", "blue"] },
  layout: [{ type: "text", name: "sku" }],
};
const list = { templates: [{ id: "t1", name: "Tag", description: "", unit: "mm", dpi: 300, format: detail.format, options: detail.options }] };
const printers = [{ id: "p1", name: "Label Printer", kind: "cups", config: null, enabled: true }];
const summary = { total: 2, succeeded: 2, failed: [], jobs: 1 };

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

// Optional `batch` override lets a test return a custom /api/batch response (failures, 422, etc.).
function stubFetch(batch?: (body: Record<string, unknown>) => Response) {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    if (url.startsWith("/api/templates/t1")) return json(detail);
    if (url.startsWith("/api/templates")) return json(list);
    if (url.startsWith("/api/printers")) return json(printers);
    if (url.startsWith("/api/batch")) {
      const body = (init?.body ? JSON.parse(init.body as string) : {}) as Record<string, unknown>;
      if (batch) return batch(body);
      // download returns a binary blob; print returns the JSON summary (submitBatch discriminates on content-type).
      if (body.mode === "download") {
        return new Response(new Blob(["zip"]), { status: 200, headers: { "content-type": "application/zip" } });
      }
      return json(summary);
    }
    throw new Error(`unexpected fetch: ${url}`);
  });
}

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter initialEntries={["/import"]}>
          <Import />
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

let fetchMock: ReturnType<typeof stubFetch>;
const lastCall = (path: string) => [...fetchMock.mock.calls].reverse().find(([u]) => String(u).startsWith(path));
const countCalls = (path: string) => fetchMock.mock.calls.filter(([u]) => String(u).startsWith(path)).length;

async function loadTemplateAndCsv() {
  const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
  await screen.findByRole("option", { name: "Tag" });
  fireEvent.change(picker, { target: { value: "t1" } });
  const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
  fireEvent.change(csv, { target: { value: "sku,option.color\n1,red\n2,blue\n" } });
  fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
}

describe("CSV Import screen", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    fetchMock = stubFetch();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("loads a CSV into the grid and reports the expanded total", async () => {
    renderPage();
    await loadTemplateAndCsv();
    expect(await screen.findByText("1")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
    expect(screen.getByText(/2 labels/i)).toBeInTheDocument();
  });

  it("loads a CSV from a selected file", async () => {
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const fileInput = (await screen.findByLabelText(/csv file/i)) as HTMLInputElement;
    const file = new File(["sku,option.color\n7,blue\n"], "labels.csv", { type: "text/csv" });
    fireEvent.change(fileInput, { target: { files: [file] } });
    expect(await screen.findByText("7")).toBeInTheDocument();
  });

  it("loads a CSV dropped onto the dropzone", async () => {
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const zone = await screen.findByLabelText(/csv dropzone/i);
    const file = new File(["sku,option.color\n8,red\n"], "labels.csv", { type: "text/csv" });
    fireEvent.drop(zone, { dataTransfer: { files: [file] } });
    expect(await screen.findByText("8")).toBeInTheDocument();
  });

  it("posts a download batch for all resolved rows and saves the file", async () => {
    const createUrl = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:x");
    renderPage();
    await loadTemplateAndCsv();
    fireEvent.click(await screen.findByRole("button", { name: /download/i }));
    await waitFor(() => expect(countCalls("/api/batch")).toBe(1));
    const body = JSON.parse((lastCall("/api/batch")![1] as RequestInit).body as string);
    expect(body.template).toBe("t1");
    expect(body.mode).toBe("download");
    expect(body.labels).toHaveLength(2);
    expect(body.labels[0]).toEqual({ data: { sku: "1" }, option: { color: "red" } });
    // submitBatch read a binary blob and saved it via an object URL.
    await waitFor(() => expect(createUrl).toHaveBeenCalled());
    expect(body.start_slot).toBeUndefined(); // single template: start_slot omitted
  });

  it("includes manual (global) options in the request when the CSV omits the column", async () => {
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: "sku\n1\n2\n" } }); // no option.color column
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    await screen.findByText("1");
    fireEvent.click(await screen.findByRole("button", { name: /download/i }));
    await waitFor(() => expect(countCalls("/api/batch")).toBe(1));
    const body = JSON.parse((lastCall("/api/batch")![1] as RequestInit).body as string);
    // The manual strip defaults color to its first declared value and applies it to every row.
    expect(body.labels[0]).toEqual({ data: { sku: "1" }, option: { color: "red" } });
  });

  it("disables Run above the 500-label cap", async () => {
    renderPage();
    await loadTemplateAndCsv();
    const copies = screen.getByLabelText(/copies/i) as HTMLInputElement;
    fireEvent.change(copies, { target: { value: "300" } }); // 2 rows x 300 = 600 > 500
    await waitFor(() => expect(screen.getByRole("button", { name: /download/i })).toBeDisabled());
    expect(screen.getByText(/over the 500/i)).toBeInTheDocument();
  });

  it("prints and annotates rows from the summary", async () => {
    renderPage();
    await loadTemplateAndCsv();
    fireEvent.change(screen.getByLabelText(/printer/i), { target: { value: "p1" } });
    fireEvent.click(await screen.findByRole("button", { name: /^print$/i }));
    await waitFor(() => expect(countCalls("/api/batch")).toBe(1));
    const body = JSON.parse((lastCall("/api/batch")![1] as RequestInit).body as string);
    expect(body.template).toBe("t1");
    expect(body.mode).toBe("print");
    expect(body.printer).toBe("p1");
    expect(await screen.findByText(/printed 2\/2/i)).toBeInTheDocument();
    // both rows are annotated ok in the grid (regression guard for successful-row annotations)
    expect(await screen.findAllByText("ok")).toHaveLength(2);
  });

  it("maps a print failure to the right source row via copy expansion", async () => {
    fetchMock = stubFetch(() => json({ total: 4, succeeded: 3, failed: [{ index: 3, error: "boom" }], jobs: 1 }));
    vi.stubGlobal("fetch", fetchMock);
    renderPage();
    await loadTemplateAndCsv();
    fireEvent.change(screen.getByLabelText(/copies/i), { target: { value: "2" } });
    fireEvent.change(screen.getByLabelText(/printer/i), { target: { value: "p1" } });
    fireEvent.click(await screen.findByRole("button", { name: /^print$/i }));
    // index 3 with copies=2 maps to source row 1 (sku=2), NOT row 0/row 3: assert it lands on the sku=2 row.
    const failedRow = (await screen.findByText(/failed: boom/i)).closest('[role="row"]') as HTMLElement;
    expect(within(failedRow).getByText("2")).toBeInTheDocument();
    expect(within(failedRow).queryByText("1")).not.toBeInTheDocument();
  });

  it("maps a 422 BatchInvalid failure to its row and shows a form error", async () => {
    fetchMock = stubFetch(() =>
      json(
        { error: { code: "BatchInvalid", message: "row invalid", details: { failures: [{ index: 0, code: "MissingField", message: "missing sku" }] } } },
        422,
      ),
    );
    vi.stubGlobal("fetch", fetchMock);
    renderPage();
    await loadTemplateAndCsv();
    fireEvent.click(await screen.findByRole("button", { name: /download/i }));
    // index 0 maps to the first CSV row (sku=1): the annotation lands on that row.
    const failedRow = (await screen.findByText(/failed: missing sku/i)).closest('[role="row"]') as HTMLElement;
    expect(within(failedRow).getByText("1")).toBeInTheDocument();
    // a form-level error (the <p>, not the row annotation span) is also shown.
    expect(screen.getByText("missing sku", { selector: "p" })).toBeInTheDocument();
  });

  it("blocks a malformed CSV from being submitted", async () => {
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    fireEvent.change(csv, { target: { value: 'sku\n"open' } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    expect(await screen.findByText(/parse error/i)).toBeInTheDocument();
    // No grid or Run buttons render, so nothing can be posted.
    expect(screen.queryByRole("button", { name: /download/i })).not.toBeInTheDocument();
    expect(countCalls("/api/batch")).toBe(0);
  });

  it("blocks a CSV with more rows than the 500 cap at load", async () => {
    renderPage();
    const picker = (await screen.findByLabelText(/template/i)) as HTMLSelectElement;
    await screen.findByRole("option", { name: "Tag" });
    fireEvent.change(picker, { target: { value: "t1" } });
    const csv = (await screen.findByLabelText(/paste csv/i)) as HTMLTextAreaElement;
    const big = "sku\n" + Array.from({ length: 501 }, (_, i) => String(i)).join("\n");
    fireEvent.change(csv, { target: { value: big } });
    fireEvent.click(screen.getByRole("button", { name: /load csv/i }));
    expect(await screen.findByText(/limit is 500/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /download/i })).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run (from `ui/`): `npm run test -- Import`
Expected: FAIL (the stub `Import` has no picker/CSV UI).

- [ ] **Step 3: Implement `ui/src/pages/Import.tsx`**

```tsx
import { useRef, useState } from "react";
import { useTemplates, useTemplate, usePrinters } from "../api/queries";
import { defaultOptions, referencedFields } from "../lib/templateFields";
import {
  MAX_BATCH_LABELS,
  expandedCount,
  resolveLabels,
  sourceRowForExpandedIndex,
  duplicateRow,
  removeRow,
  validateOptionCell,
  newId,
  type LabelGridRow,
} from "../lib/labelGrid";
import { parseCsv } from "../lib/csv";
import { LabelGrid } from "../components/LabelGrid";
import { ApiError, saveBlob, submitBatch } from "../api/client";
import { useToast } from "../app/toast-context";
import type { TemplateDetail } from "../api/types";

type BatchFailures = { failures?: { index: number; code: string; message: string }[] };
const buttonBase = "rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2";
const inputClass = "rounded-md border px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2";
const inputStyle = { background: "var(--surface)", borderColor: "var(--border)", color: "var(--ink)" } as const;

export function Import() {
  const { data: templates } = useTemplates();
  const { data: printers } = usePrinters();
  const { push } = useToast();

  const [templateId, setTemplateId] = useState("");
  const { data: detail } = useTemplate(templateId);

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">Import</h1>
      <label className="flex flex-col gap-1 max-w-sm">
        <span className="text-sm font-medium">Template</span>
        <select aria-label="template" value={templateId} onChange={(e) => setTemplateId(e.target.value)} className={inputClass} style={inputStyle}>
          <option value="">choose a template</option>
          {(templates?.templates ?? []).map((t) => (
            <option key={t.id} value={t.id}>
              {t.name}
            </option>
          ))}
        </select>
      </label>
      {detail && <CsvEditor key={detail.id} detail={detail} printers={(printers ?? []).filter((p) => p.enabled)} push={push} />}
    </div>
  );
}

function CsvEditor({
  detail,
  printers,
  push,
}: {
  detail: TemplateDetail;
  printers: { id: string; name: string }[];
  push: (t: { kind: "ok" | "error"; message: string }) => void;
}) {
  const [text, setText] = useState("");
  const [loadedSource, setLoadedSource] = useState(""); // last successfully parsed CSV text (for Reset)
  const [rows, setRows] = useState<LabelGridRow[]>([]);
  // A ref mirrors `rows` so event handlers (notably run(), which fires right after an edit's blur-commit)
  // read the latest rows synchronously, not a stale render closure. Every mutation goes through commitRows.
  const rowsRef = useRef(rows);
  const commitRows = (next: LabelGridRow[]) => {
    rowsRef.current = next;
    setRows(next);
  };
  const [csvFields, setCsvFields] = useState<string[]>([]);
  const [optionColumns, setOptionColumns] = useState<string[]>([]); // declared options provided as CSV columns
  const [issues, setIssues] = useState<string[]>([]);
  const [manualOptions, setManualOptions] = useState<Record<string, string>>(() => defaultOptions(detail.options));
  const [copies, setCopies] = useState(1);
  const [startSlot, setStartSlot] = useState(0);
  const [printer, setPrinter] = useState<string | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const declaredOptions = detail.options ?? {};
  const declaredNames = Object.keys(declaredOptions);
  const isSheet = detail.format.type === "sheet";

  // Fields required for a row depend on THAT row's effective options (a CSV option.<name> column can
  // vary per row and gate different containers), so this is computed per row, not from manualOptions alone.
  const requiredForRow = (row: LabelGridRow): string[] => referencedFields(detail.layout, { ...manualOptions, ...row.option });
  // Grid columns: CSV columns plus any required field (across all row variants) the CSV omits.
  const requiredUnion = new Set<string>();
  for (const row of rows) for (const f of requiredForRow(row)) requiredUnion.add(f);
  const baseRequired = rows.length ? [...requiredUnion] : referencedFields(detail.layout, manualOptions);
  const displayedFields = [...csvFields, ...baseRequired.filter((f) => !csvFields.includes(f))];
  // Manual strip handles declared options the CSV did not provide a column for.
  const manualOptionNames = declaredNames.filter((n) => !optionColumns.includes(n));

  // One validation function, used both for render (viewRows) and as the run() submit guard, so a value
  // committed on blur right before a click cannot be submitted while the button is still showing enabled.
  const validateRow = (row: LabelGridRow): LabelGridRow["validation"] => {
    const field: Record<string, string> = {};
    for (const f of requiredForRow(row)) if ((row.data[f] ?? "").length === 0) field[f] = "required";
    const option: Record<string, string> = {};
    for (const name of optionColumns) {
      const err = validateOptionCell(row.option[name] ?? "", declaredOptions[name]);
      if (err) option[name] = err;
    }
    const v: LabelGridRow["validation"] = {};
    if (Object.keys(field).length) v.field = field;
    if (Object.keys(option).length) v.option = option;
    return v;
  };
  const rowInvalid = (row: LabelGridRow): boolean => {
    const v = validateRow(row);
    return !!v.field || !!v.option;
  };
  // Validation is derived fresh each render (never stored), so it cannot go stale when options change.
  const viewRows: LabelGridRow[] = rows.map((row) => ({ ...row, validation: validateRow(row) }));
  const hasErrors = viewRows.some(rowInvalid);

  const total = expandedCount(rows.length, copies);
  const overCap = total > MAX_BATCH_LABELS;

  const clearGrid = () => {
    commitRows([]);
    setCsvFields([]);
    setOptionColumns([]);
    setLoadedSource("");
  };

  const loadFrom = (raw: string) => {
    setFormError(null); // a fresh load clears any prior submit error
    const parsed = parseCsv(raw);
    // A malformed CSV (papaparse error) must not be submittable: surface the issues and load nothing.
    if (parsed.fatal) {
      setIssues(parsed.issues);
      clearGrid();
      return;
    }
    // The grid is non-virtualized and a batch caps at 500 labels, so reject CSVs over the row cap up front
    // (a larger file could never submit, and rendering thousands of rows would freeze the UI).
    if (parsed.rows.length > MAX_BATCH_LABELS) {
      setIssues([`CSV has ${parsed.rows.length} rows; the limit is ${MAX_BATCH_LABELS}.`]);
      clearGrid();
      return;
    }
    const usable = parsed.optionColumns.filter((n) => declaredNames.includes(n));
    const undeclared = parsed.optionColumns.filter((n) => !declaredNames.includes(n));
    setCsvFields(parsed.fields);
    setOptionColumns(usable);
    setIssues([...parsed.issues, ...undeclared.map((n) => `Column option.${n} is not a declared option and is ignored.`)]);
    const built = parsed.rows.map<LabelGridRow>((r) => {
      const option: Record<string, string> = {};
      for (const n of usable) option[n] = r.option[n] ?? "";
      return { id: newId(), origin: "csv", data: { ...r.data }, option, validation: {} };
    });
    commitRows(built);
    setLoadedSource(raw);
  };

  // Reset reloads the originally parsed CSV: removed rows return in their original order, edits and
  // duplicates are discarded, and copies returns to 1. Reloading is deterministic and avoids the
  // index-tracking bugs of trying to splice removed rows back into a mutated list.
  const onReset = () => {
    loadFrom(loadedSource);
    setCopies(1);
  };

  const run = async (mode: "download" | "print") => {
    setFormError(null);
    // Imperative submit guards (defense in depth; the buttons are also disabled for these, but the
    // disabled state lags a blur-commit by one render). Validate the live snapshot and the cap/printer.
    const snapshot = rowsRef.current;
    if (snapshot.length === 0) return;
    if (snapshot.some(rowInvalid)) {
      setFormError("Fix the highlighted rows before running.");
      return;
    }
    if (expandedCount(snapshot.length, copies) > MAX_BATCH_LABELS) {
      setFormError(`Too many labels (over the ${MAX_BATCH_LABELS} limit).`);
      return;
    }
    if (mode === "print" && !printer) {
      setFormError("Select a printer to print.");
      return;
    }
    setBusy(true);
    // Clear stale annotations from a previous run so a later validation failure cannot leave old results visible.
    commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined })));
    // Snapshot the submitted rows' ids and the copies used, so a failure index maps to the right ROW
    // even if the grid mutates in flight (annotate by id, not array index). Read rowsRef so a just-committed
    // cell edit (blur fires before this click handler) is included.
    const submittedIds = rowsRef.current.map((r) => r.id);
    const submittedCopies = copies;
    const idForExpandedIndex = (index: number): string | undefined => submittedIds[sourceRowForExpandedIndex(index, submittedCopies)];
    try {
      const labels = resolveLabels(rowsRef.current, manualOptions, submittedCopies);
      const r = await submitBatch({
        template: detail.id,
        labels,
        mode,
        ...(mode === "print" ? { printer } : {}),
        ...(isSheet && startSlot ? { start_slot: startSlot } : {}),
      });
      if (r.kind === "download") {
        // Sheet downloads are a composed PDF; single-template batches are a ZIP.
        saveBlob(r.blob, r.filename ?? `${detail.id}.${isSheet ? "pdf" : "zip"}`);
        push({ kind: "ok", message: `Downloaded ${labels.length} labels` });
      } else {
        const { succeeded, total: t, failed } = r.summary;
        const failById = new Map<string, string>();
        for (const f of failed) {
          const id = idForExpandedIndex(f.index);
          if (id) failById.set(id, failById.has(id) ? `${failById.get(id)}; ${f.error}` : f.error);
        }
        // All submitted rows that still exist are annotated ok unless they failed.
        const submitted = new Set(submittedIds);
        commitRows(
          rowsRef.current.map((row) =>
            submitted.has(row.id)
              ? { ...row, annotation: failById.has(row.id) ? { status: "failed", message: failById.get(row.id) } : { status: "ok" } }
              : row,
          ),
        );
        push({ kind: failed.length ? "error" : "ok", message: `Printed ${succeeded}/${t}` });
      }
    } catch (err) {
      if (err instanceof ApiError && err.code === "BatchInvalid") {
        const failures = (err.details as BatchFailures)?.failures ?? [];
        const failById = new Map<string, string>();
        for (const f of failures) {
          const id = idForExpandedIndex(f.index);
          if (id) failById.set(id, failById.has(id) ? `${failById.get(id)}; ${f.message}` : f.message);
        }
        commitRows(rowsRef.current.map((row) => (failById.has(row.id) ? { ...row, annotation: { status: "failed", message: failById.get(row.id) } } : row)));
        const message = failures.map((f) => f.message).join("; ") || err.message;
        setFormError(message);
        push({ kind: "error", message });
      } else {
        const message = err instanceof Error ? err.message : "Batch failed";
        push({ kind: "error", message });
      }
    } finally {
      setBusy(false);
    }
  };

  const positions = detail.format.type === "sheet" ? detail.format.positions.length : 0;

  return (
    <div className="flex flex-col gap-4">
      <div
        aria-label="csv dropzone"
        onDragOver={(e) => e.preventDefault()}
        onDrop={async (e) => {
          e.preventDefault();
          const file = e.dataTransfer.files?.[0];
          if (file) {
            const content = await file.text();
            setText(content);
            loadFrom(content);
          }
        }}
        className="flex max-w-sm flex-col gap-1 rounded-md border border-dashed p-4"
        style={{ borderColor: "var(--border)" }}
      >
        <label className="flex flex-col gap-1">
          <span className="text-sm font-medium">CSV file</span>
          <input
            type="file"
            accept=".csv,text/csv"
            aria-label="csv file"
            className="text-sm"
            disabled={busy}
            onChange={async (e) => {
              const file = e.target.files?.[0];
              if (file) {
                const content = await file.text();
                setText(content);
                loadFrom(content);
              }
            }}
          />
        </label>
        <span className="text-xs" style={{ color: "var(--muted)" }}>
          or drop a CSV file here
        </span>
      </div>
      <label className="flex flex-col gap-1">
        <span className="text-sm font-medium">Paste CSV</span>
        <textarea aria-label="paste CSV" value={text} onChange={(e) => setText(e.target.value)} rows={4} className={inputClass} style={inputStyle} />
      </label>
      <div>
        <button type="button" onClick={() => loadFrom(text)} disabled={busy} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
          Load CSV
        </button>
      </div>

      {issues.length > 0 && (
        <ul className="text-sm" style={{ color: "var(--bad)" }}>
          {issues.map((m) => (
            <li key={m}>{m}</li>
          ))}
        </ul>
      )}

      {rows.length > 0 && (
        <>
          {manualOptionNames.length > 0 && (
            <div className="flex flex-wrap gap-3">
              {manualOptionNames.map((name) => (
                <label key={name} className="flex flex-col gap-1">
                  <span className="text-sm font-medium">{name}</span>
                  <select
                    aria-label={name}
                    value={manualOptions[name] ?? declaredOptions[name][0] ?? ""}
                    disabled={busy}
                    onChange={(e) => {
                      setManualOptions({ ...manualOptions, [name]: e.target.value });
                      // changing a global option changes the batch input, so clear prior results.
                      commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined })));
                      setFormError(null);
                    }}
                    className={inputClass}
                    style={inputStyle}
                  >
                    {declaredOptions[name].map((v) => (
                      <option key={v} value={v}>
                        {v}
                      </option>
                    ))}
                  </select>
                </label>
              ))}
            </div>
          )}

          <div className="flex flex-wrap items-end gap-3">
            <label className="flex flex-col gap-1">
              <span className="text-sm font-medium">Copies</span>
              <input
                type="number"
                min={1}
                aria-label="copies"
                value={copies}
                disabled={busy}
                onChange={(e) => {
                  setCopies(Math.max(1, Math.floor(Number(e.target.value) || 1)));
                  commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined })));
                  setFormError(null);
                }}
                className={inputClass}
                style={inputStyle}
              />
            </label>
            {isSheet && (
              <label className="flex flex-col gap-1">
                <span className="text-sm font-medium">Start slot</span>
                <input
                  type="number"
                  min={0}
                  max={Math.max(0, positions - 1)}
                  aria-label="start slot"
                  value={startSlot}
                  disabled={busy}
                  onChange={(e) => {
                    setStartSlot(Math.max(0, Math.min(positions - 1, Math.floor(Number(e.target.value) || 0))));
                    commitRows(rowsRef.current.map((r) => ({ ...r, annotation: undefined })));
                    setFormError(null);
                  }}
                  className={inputClass}
                  style={inputStyle}
                />
              </label>
            )}
            <label className="flex flex-col gap-1">
              <span className="text-sm font-medium">Printer</span>
              <select
                aria-label="printer"
                value={printer ?? ""}
                disabled={busy}
                onChange={(e) => {
                  setPrinter(e.target.value || undefined);
                  setFormError(null);
                }}
                className={inputClass}
                style={inputStyle}
              >
                <option value="">none (download only)</option>
                {printers.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>
            <button type="button" onClick={onReset} disabled={busy} className={`${buttonBase} border`} style={{ borderColor: "var(--border)", color: "var(--ink)" }}>
              ↺ Reset
            </button>
            <span className="text-sm" style={{ color: "var(--muted)" }}>
              {total} labels
            </span>
          </div>

          {overCap && (
            <p style={{ color: "var(--bad)" }}>
              {total} labels is over the 500-label limit. Reduce rows or copies.
            </p>
          )}
          {formError && <p style={{ color: "var(--bad)" }}>{formError}</p>}

          <LabelGrid
            rows={viewRows}
            fields={displayedFields}
            optionNames={optionColumns}
            optionValues={declaredOptions}
            onRowsChange={(next, { indexes }) => {
              // viewRows carries derived validation (and rows may carry a prior run's annotation); store
              // only canonical data: drop validation everywhere and clear annotation on the edited rows.
              const dirty = new Set(indexes);
              commitRows(next.map((r, i) => ({ ...r, validation: {}, annotation: dirty.has(i) ? undefined : r.annotation })));
              setFormError(null); // editing invalidates a prior submit error
            }}
            onDuplicate={(id) => {
              // A structural change invalidates the prior run's per-row results, so clear annotations.
              commitRows(duplicateRow(rowsRef.current, id).map((r) => ({ ...r, annotation: undefined })));
              setFormError(null);
            }}
            onRemove={(id) => {
              commitRows(removeRow(rowsRef.current, id).map((r) => ({ ...r, annotation: undefined })));
              setFormError(null);
            }}
            disabled={busy}
          />

          <div className="flex gap-3">
            <button
              type="button"
              onClick={() => run("print")}
              disabled={busy || overCap || hasErrors || !printer}
              className={buttonBase}
              style={{ background: "var(--accent)", color: "var(--accent-ink, #fff)" }}
            >
              Print
            </button>
            <button
              type="button"
              onClick={() => run("download")}
              disabled={busy || overCap || hasErrors}
              className={`${buttonBase} border`}
              style={{ borderColor: "var(--border)", color: "var(--ink)" }}
            >
              Download
            </button>
          </div>
        </>
      )}
    </div>
  );
}
```

> The `CsvEditor` is keyed by `detail.id` so switching templates resets all CSV state cleanly (mirrors the keyed `PrintForm`). The `rowsRef` mirror exists for one real-browser race: clicking Download/Print while a cell editor is open blurs the input (committing the edit via `onRowsChange`) and then fires the button click in the same tick, so a plain `rows` render-closure would be stale; `run()` reads `rowsRef.current` instead. This ordering is not reproducible with jsdom `fireEvent` (which flushes state between synthetic events), so it is covered by the invariant (every mutation goes through `commitRows`), not a flaky test.

- [ ] **Step 4: Run to verify it passes**

Run (from `ui/`): `npm run test -- Import`
Expected: PASS (11 tests).

- [ ] **Step 5: Run the full UI gate**

Run (from `ui/`): `npm run lint && npm run test && npm run build`
Expected: lint clean, all tests pass, build succeeds. Fix any `noUnusedLocals` / `react-hooks` / `react-refresh` violations at the root (do not suppress).

- [ ] **Step 6: Commit**

```bash
git add ui/src/pages/Import.tsx ui/src/pages/Import.test.tsx
git commit -m "feat(ui): CSV import editable grid screen

Fixes #24"
```
This implementation commit carries `Fixes #24` in its body so the issue closes when the branch reaches `main` (whether the later merge fast-forwards or not).

---

### Task 6: Documentation (ADR, SPEC note, plan status)

**Files:**
- Create: `docs/adr/0014-csv-import-grid.md`
- Modify: `docs/adr/README.md`
- Modify: `docs/PLAN-phase-1.md`

- [ ] **Step 1: Write ADR-0014**

Create `docs/adr/0014-csv-import-grid.md`:
```markdown
# 14. CSV import editable grid

**Status:** Accepted

## Context

ADR-0008 named a CSV screen in M5; ADR-0013 settled the Render & Print screen and noted the reusable
`/api/batch` path would also back the CSV grid. Building the CSV screen (#24) fixed concrete choices
about the grid component, the row model, option handling, copies, and the batch-size cap. The screen is
frontend-only: it does not use the `/api/import/csv` backend endpoint (that is the self-contained
automation path); it parses CSV in the browser, lets the user edit, and posts resolved labels to
`/api/batch` (ADR-0011).

## Decision

- **Reusable grid + row model.** The editable grid is a standalone `LabelGrid` component built on
  `react-data-grid`, operating on a formalized `LabelGridRow { id, origin, source?, data, option,
  copyGroup?, validation, annotation? }`. CSV fills `origin: "csv"`; M7's Homebox mapping will fill
  `origin: "connector"` + `source` into the same grid with no rework.
- **Client-side CSV.** `papaparse` parses the pasted/dropped/selected CSV (auto BOM strip, quoted
  fields, delimiter pinned to `,`). A `option.<name>` header binds a per-row template option;
  empty/duplicate headers and ragged rows are flagged as issues (per spec §4), while a malformed CSV
  (papaparse error) blocks load. Parsing is **synchronous**: the screen rejects CSVs over the 500-row /
  2 MB cap at load, so no large file is ever parsed, which supersedes the M5 design spec's
  "off the main thread for large files" note (worker offloading would add complexity for no benefit
  under the cap).
- **Options.** A manual options strip sets declared options the CSV omits (applied to all rows); a CSV
  `option.<name>` column overrides per row (CSV wins). Option values are validated against the
  template's declared values and flagged inline.
- **Copies and cap.** A single global copies multiplier expands rows adjacently (row-major). The grid
  shows the expanded total (`rows x copies`) and disables Run above the 500-label cap client-side rather
  than discovering `413 BatchTooLarge` server-side.
- **Run.** Resolved labels post to `/api/batch` as `mode=download` (blob saved) or `mode=print`
  (summary). `422 BatchInvalid` failures and print-transport failures are mapped from the expanded label
  index back to the source row (`floor(index / copies)`) and annotated on the grid.

## Consequences

- The grid is the M7 integration surface, built and tested once here against CSV.
- No backend or API change: the screen consumes existing endpoints. SPEC gets a clarifying note plus a
  changelog entry distinguishing this client-side screen from the separate `/api/import/csv` API.
- Virtualization is disabled in the grid; the screen rejects CSVs over the 500-row cap at load, so the
  grid renders at most 500 rows (safe without virtualization, and testable under jsdom).

## Alternatives considered

- **Headless TanStack Table.** Rejected: re-implements cell editing, selection, and keyboard a11y that
  react-data-grid provides.
- **Reusing the `/api/import/csv` endpoint from the UI.** Rejected: that endpoint is self-contained (no
  per-row option editing); the screen needs client-side editing before submit, so it posts to `/batch`.
```

- [ ] **Step 2: Add the index row**

In `docs/adr/README.md`, after the ADR-0013 row, add:
```markdown
| [0014](0014-csv-import-grid.md) | CSV import editable grid | Accepted |
```

> P1-54 is **not** marked DONE here. The plan-status update happens in Task 7 after the adversarial review loop, so its commit hash points at the final reviewed work, not a pre-review commit.

- [ ] **Step 3: Add the SPEC clarification and changelog entry**

The repo process requires a SPEC + changelog touch on a behavior change (mirroring how ADR-0013/#20 added a "No API change" changelog line). There is no API change, but SPEC's existing "CSV import" section documents `POST /import/csv`, so add one clarifying sentence there distinguishing the new UI screen, plus a changelog entry.

In `docs/SPEC.md`, at the end of the "## CSV import" section's first paragraph (the one ending "compose the rows into paginated pages."), add:
```markdown

The web UI's CSV Import screen (`/import`, ADR-0014) is a separate client-side path: it parses and
edits the CSV in the browser and posts resolved labels to `POST /api/batch`. It does not call
`/api/import/csv`, which remains the self-contained automation endpoint.
```
Then add this entry at the top of the `## Changelog` list (matching the colon style of the recent entries; no em dash):
```markdown
- **2026-06-16**: Web UI CSV Import screen (`/import`): parse a CSV client-side, review/edit rows and
  per-row options in an editable grid, then batch print or download via `POST /api/batch` (ADR-0014,
  #24). No API change; the screen does not use `/api/import/csv`.
```

- [ ] **Step 4: Commit the docs**

```bash
git add docs/adr/0014-csv-import-grid.md docs/adr/README.md docs/SPEC.md
git commit -m "docs: ADR-0014 CSV import grid; SPEC note"
```

---

### Task 7: Adversarial review loop and integrate

The per-task commits (Tasks 1-6) are local WIP on the `m5-csv-grid` branch; nothing reaches `main` until this end-to-end adversarial review passes. The repo process (CLAUDE.md "Working on an issue") requires this review loop before the work is ready to integrate. Subagent-driven execution's per-task reviews do not replace this end-to-end pass.

- [ ] **Step 1: Adversarial review of the whole diff**

Dispatch an adversarial code reviewer (a `pfa-dev:code-reviewer` agent, briefed to find real problems, not rubber-stamp) against the full branch diff. It must audit against #24's acceptance criteria ("from the UI, a CSV produces a batch via `/batch`; row/field errors shown; download or print selectable"), correctness, edge cases, the tests, and this repo's conventions. Require file:line evidence for each finding.

- [ ] **Step 2: Fix every meaningful finding, then re-review**

Address each finding (fix it, or justify with evidence why it is not a problem). Re-dispatch the reviewer on the updated diff. Repeat until a review pass surfaces no meaningful fixes (consciously declined nits do not count).

- [ ] **Step 3: Mark P1-54 DONE with the final commit hash**

Now that the reviewed work is final, capture the **primary implementation commit's** short hash (the `feat(ui): CSV import editable grid screen` commit from Task 5). Review fixes from Task 7 land as their own commits on the branch; P1-54 cites this one primary hash, matching how every other entry in the file references a single commit. One exact command:
```bash
git log --grep='CSV import editable grid screen' --format=%h -n 1
```
Then, in `docs/PLAN-phase-1.md`, change the P1-54 heading line from:
```markdown
#### P1-54 CSV import screen · GH #24
```
to (matching the existing `· DONE (hash)` style; no em dash):
```markdown
#### P1-54 CSV import screen · GH #24 · DONE (<final-impl-commit-hash>)
```
Then commit:
```bash
git add docs/PLAN-phase-1.md
git commit -m "docs: mark P1-54 (CSV import screen) done"
```

- [ ] **Step 4: Final gate and integrate**

```bash
cd ui && npm run lint && npm run test && npm run build && cd ..
cargo fmt && cargo clippy --all-targets --all-features && cargo test
git checkout main && git merge m5-csv-grid && git push
```
The repo rule requires running `cargo fmt`, `cargo clippy --all-targets --all-features`, and `cargo test` before reporting any change; no Rust changed here, so they pass unchanged but are run to confirm nothing broke. The Task 5 implementation commit already carries `Fixes #24`, so the issue closes on push (fast-forward or not).

---

## Self-Review

**1. Spec coverage (§4 + reusable grid section):**
- Drop/select CSV, parse client-side, header → columns → Task 3 (`parseCsv`) + Task 5 (file input via `File.text()` and a paste textarea, both calling `loadFrom`). True drag-and-drop styling is a follow-up; file selection is implemented and tested.
- BOM strip, empty/duplicate headers, ragged rows, papaparse parse errors flagged → Task 3.
- `option.<name>` column → per-row option; undeclared option columns dropped with an issue → Task 3 (split) + Task 5 (`loadFrom`).
- Required template fields the CSV omits are added as editable columns and flagged when empty, per row's own options → Task 5 (`requiredForRow`/`displayedFields`/`viewRows`).
- Inline editing: data text, options dropdowns, invalid option flagged → Task 4 (`LabelGrid`, edit tests) + Task 5 (`viewRows` derived validation).
- Manual + CSV options, CSV wins; option omitted for option-less templates (undeclared columns dropped) → Task 2 (`resolveLabels`) + Task 5 (manual strip + `loadFrom` filter).
- Copies global multiplier, adjacent expansion → Task 2 (`resolveLabels`, `expandedCount`) + Task 5 (stepper).
- Per-row duplicate / remove / Reset → Task 2 (`duplicateRow`, `removeRow`) + Task 5 (Reset reloads the parsed CSV, restoring removed rows in order and setting copies to 1).
- Run → resolve → `/api/batch` download/print; per-row annotation from summary → Task 5.
- 500 cap shown + Run disabled client-side; CSVs over 500 rows rejected at load (grid renders <= 500 rows); lazy expansion (no giant array until submit) → Task 2/Task 5 (`resolveLabels` only runs on submit).
- Sheet start_slot, hidden for single → Task 5.
- Error shapes: `422 BatchInvalid` (both modes) mapped to rows; `200` print summary `failed` mapped; `413` prevented client-side → Task 5.
- Reusable, not private to CSV screen → Task 4 (standalone component, `LabelGridRow` in `lib`).

**2. Placeholder scan:** No TBD/TODO; every code step has complete code; commands have expected output. The one deferral (worker offloading) is called out explicitly as a conscious YAGNI decision, not a placeholder.

**3. Type consistency:** `LabelGridRow` shape is identical across Tasks 2, 4, 5. `resolveLabels`/`expandedCount`/`sourceRowForExpandedIndex`/`duplicateRow`/`removeRow`/`validateOptionCell`/`newId`/`MAX_BATCH_LABELS` defined in Task 2 and consumed in Task 5 with matching signatures. `parseCsv` return shape (`fields`, `optionColumns`, `rows`, `issues`) defined in Task 3 and consumed in Task 5. `LabelGridProps` (`rows`, `fields`, `optionNames`, `optionValues`, `onRowsChange`, `onDuplicate`, `onRemove`) defined in Task 4 and used identically in Task 5. `submitBatch`/`saveBlob`/`ApiError` signatures match `ui/src/api/client.ts`.

**Known follow-up to consider filing (not in this plan):** drag-and-drop dropzone styling (the plan ships a file input + drop target + paste, parsing identically); data-bound image fields in CSV (rare; treated as plain text cells for now). Worker-based parsing is intentionally not pursued (recorded in ADR-0014: the 500-row/2 MB cap makes it unnecessary).
