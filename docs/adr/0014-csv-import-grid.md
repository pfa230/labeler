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
