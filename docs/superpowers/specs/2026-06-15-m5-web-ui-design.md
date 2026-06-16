# M5 — Web UI (design)

Status: approved (2026-06-15)
Issues: #15 (shell), #17 (templates), #20 (render/print), #24 (CSV), #23 (settings).
Delivery model: ADR-0008 (React + TypeScript SPA in `ui/`, Vite, served by axum, API under `/api`).
Mockups: [`m5-ui-mockups.html`](m5-ui-mockups.html) (open in a browser).

## Goal

A basic operational web UI so a home/SMB user runs the service without curl: browse templates and
preview them, fill a render/print form, batch-import from CSV, and manage settings and printers. The
UI is an idiomatic React SPA consuming the JSON REST API; the backend stays API-only.

## Dependencies (designed separately, before UI implementation)

The UI assumes two backend pieces that are **their own designs**, not part of this spec:

1. **Unified batch render/print endpoint** (its own ADR). The UI posts one batch and renders whatever
   comes back; the backend owns the single-vs-sheet decision. Contract:
   `POST /api/batch { template, labels: [{ data, option }], mode: "download" | "print", printer?,
   format?, start_slot? }`. Dispatch by template format, full matrix:
   - single + download → render each, ZIP, `application/zip`
   - single + print → one job per label, `{ total, succeeded, failed[] }`
   - sheet + download → lay rows into slots, paginate across pages, `application/pdf`
   - sheet + print → compose the PDF, send to the printer, summary
   This absorbs `/render/batch` and the server side of `/import/csv`; `/import/csv` remains only as the
   self-contained CSV API/CLI path. New backend work: single batch (zip + per-row jobs), multi-page
   sheet pagination, sheet→printer.
2. **Template-level / job options** (its own design). The class of job-level knobs distinct from
   per-row data and per-row template options. `start_slot` (skip a leading N slots on a partly-used
   sheet) is the first; others to taxonomize: skipping arbitrary slots, per-job margins, tape gap/cut.
   This design defines how they are declared on the template and how they extend the batch contract and
   the UI.

The UI is built against these contracts; this spec does not redefine them.

## Architecture

- **Stack:** React + TypeScript, Vite, Tailwind + headless components (per ADR-0008). A typed API
  client wraps `/api`. Client-side routing (the SPA fallback serves `index.html`).
- **Uniform batch principle:** the render/print and CSV screens build a list of resolved labels
  `{ data, option }` client-side (forms, inline edits, copies expansion) and `POST /api/batch`. The UI
  branches only on **download** (response is a blob → trigger save) vs **print** (response is JSON →
  show summary). It never branches on single vs sheet. A single-label render is a batch of one.
- **Per-label preview:** previews call `POST /api/render/label` (single) for a live thumbnail; sheet
  previews call the batch/`render/batch` path. (Preview transport is finalized with the batch design.)

### Shell (#15)

Left sidebar (Templates, Print, Import, Settings), brand mark, light/dark theme toggle, and a global
toast region for success/error. Responsive: sidebar collapses to a drawer on mobile. No console errors
on load. Depends on the `/api` namespacing landing in #15 (a breaking move of the API from root).

### Visual style — "Ink & Tape"

Themeable tokens, light default with a derived dark theme: warm paper surface, ink-black text,
label-tape orange accent, monospace for ids/codes. Chosen to read as a labeling tool and avoid the
generic SaaS look (ADR-0008 goal). Token names live in the mockup CSS as the reference.

## Pages

### 1. Templates list — `/` (#17)
Landing page. Responsive grid of template cards: preview thumbnail (`/api/render/label` sample data),
id (monospace chip), name, single/sheet badge. Search by id. Empty state. Card → detail. A "new
template (YAML)" affordance.

### 2. Template detail — `/templates/:id` (#17)
Large preview, metadata (unit, dpi, format + dimensions, declared options, referenced fields, settings
used), read-only collapsible YAML source, and "Use to print" (prefills Render & Print). Single vs
sheet indicated.

### 3. Render & Print — `/print` (#20)
Two-pane. Left: template picker, a form auto-generated from the template's referenced fields, an
options section (selects from declared `options`), printer select. Right: sticky live preview
(debounced) and **Print** (primary) / **Download** (secondary, png/pdf) actions. Inline field errors;
backend errors surface near the action and in a toast. Submits a batch of one to `/api/batch`.

### 4. CSV Import — `/import` (#24)
An interactive, client-side **editable grid**, not a raw upload-and-forget:
- Drop/select a CSV; parse client-side. Header row → columns. A leading BOM is stripped; empty or
  duplicate headers and ragged rows are flagged in the UI.
- The dropzone separates **fields** from **options**; a column named `option.<name>` binds a template
  option per row.
- **Inline editing:** every cell is editable, data as text inputs, options as dropdowns of the
  template's allowed values, so an invalid CSV option value is flagged ("fix option") and corrected in
  place.
- **Manual + CSV options:** a manual options strip sets options the CSV omits, applied to all rows; a
  CSV `option.<name>` column overrides per row (CSV wins when both exist).
- **Copies:** a single global multiplier (toolbar stepper). Total = rows × copies, expanded adjacently
  (3 rows × 2 → 1,1,2,2,3,3). Per-row **⧉ duplicate** and **✕ remove** edit the row list; **↺ Reset**
  restores removed rows and sets copies to 1.
- **Run:** the screen expands copies, resolves each row to `{ data, option }`, and `POST /api/batch`
  with `mode` = download (ZIP/PDF blob) or print (summary). Per-row outcomes from the summary annotate
  the table.
This re-scopes #24 to an editable grid; it stays frontend-only against `/api/batch`. `start_slot`
(skip-N) lands with the job-options design.

### 5. Settings & Printers — `/settings` (#23)
One page, two sections. *Settings*: key/value editor seeded with `qr_base_url`
(`GET/PUT /api/settings`). *Printers*: table with add/edit/delete (`/api/printers` CRUD), kind +
config (cups uri), enabled toggle.

## Error handling

Field-level validation inline; API errors (the stable `{ error: { code, message, details } }` schema)
surface in toasts with the message, and near the triggering control where one exists. The CSV grid maps
a print summary's `failed[]` back to row annotations; download failures (atomic) surface the
`details.row`.

## Testing

- Component tests for the generated render form, the CSV grid (parse, inline edit, copies expansion,
  duplicate/remove/reset, option precedence), and the settings/printers tables.
- A smoke e2e (Playwright or similar) for the core flow: open Templates → pick → fill → preview →
  download. CSV: load a small file → edit a cell → run download.
- The batch-call layer is tested against the unified endpoint once it exists; until then, against a
  typed mock of its contract.

## Out of scope (M5)

GUI template editor (Later), interactive CSV field-mapping (Phase 2; header-name match only here),
API token auth, printer status read-back. Sheet printing and multi-page sheets are delivered by the
unified batch endpoint (dependency 1), not by the UI.

## Open items to file

- Issue: unified batch endpoint + ADR (dependency 1).
- Issue: template-level / job options design (dependency 2).
- Issue: optional `option.<name>` columns for the self-contained `/import/csv` API path (low priority).
