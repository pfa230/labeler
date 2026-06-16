# M5 — Web UI (design)

Status: approved (2026-06-15; refreshed 2026-06-16 after the backend dependencies shipped).
Issues: #15 (shell), #17 (templates), #20 (render/print), #24 (CSV), #23 (settings).
Delivery model: ADR-0008 (React + TypeScript SPA in `ui/`, Vite, served by axum, API under `/api`).
Mockups: [`m5-ui-mockups.html`](m5-ui-mockups.html) (open in a browser).

## Goal

A basic operational web UI so a home/SMB user runs the service without curl: browse templates and
preview them, fill a render/print form, batch-import from CSV, and manage settings and printers. The
UI is an idiomatic React SPA consuming the JSON REST API; the backend stays API-only.

## Backend contracts (shipped; the UI builds against them)

Both backend pieces the UI depends on are now implemented:

1. **Unified batch endpoint** ([ADR-0011](../../adr/0011-unified-batch-endpoint.md), #30). The UI posts
   one batch and renders whatever comes back; the backend owns the single-vs-sheet decision.
   `POST /api/batch { template, labels: [{ data, option }], mode: "download" | "print", printer?,
   format?, start_slot? }`. download → a binary (`application/zip` for single, `application/pdf` for
   sheet); print → `200 { total, succeeded, failed: [{ index, error }], jobs }`. `/render/label` remains
   for single-label preview / one-off raw render; `/render/batch` and `/print` were removed.
2. **Job options** ([ADR-0012](../../adr/0012-job-options.md), #31). Format-intrinsic `/batch` params;
   `start_slot` (skip a leading N slots on a partly-used sheet) is the only one, sheet-only. The UI
   exposes it for sheet templates (see §3, §4).

> Paths shown as `/api/...` assume the `/api` namespacing that ships as part of the #15 shell work (a
> breaking move of the REST API from root). Until that lands the routes are root-mounted (`/batch`, …).

## Architecture

- **Stack:** React + TypeScript, Vite, Tailwind + headless components (per ADR-0008). Client-side
  routing; the data layer below wraps `/api`.
- **Batch principle (action), with a preview exception.** For the **action** (Print/Download of a
  batch), the render/print and CSV screens build a list of resolved labels `{ data, option }` and
  `POST /api/batch`, branching only on **download** (binary blob → save) vs **print** (JSON summary).
  **Preview is the documented exception:** previews need a fast raw image, so single previews call
  `POST /api/render/label` (PNG/PDF) and **sheet** previews call `POST /api/batch` `mode=download` (the
  composed one-or-more-page PDF). The single render/print screen's **Download** action also uses
  `/api/render/label` for a single template (a clean raw file), not a `/batch` ZIP-of-one; `/batch` is
  used for CSV batches and for printing.

### `/api` namespacing and static serving (#15)

This is a breaking move of the REST API from root to `/api`, and the axum app must serve the SPA
without the two colliding:

- Nest all existing JSON routes under `/api` (`/api/templates`, `/api/batch`, `/api/render/label`,
  `/api/printers`, `/api/settings`, …). Swagger and the OpenAPI doc move to `/api/docs` and
  `/api/openapi.json`; utoipa `path = "/…"` annotations update so the generated doc matches.
- An unknown `/api/*` path returns the JSON `404` error contract, **not** the SPA `index.html`.
- A `ServeDir` serves the built `ui/dist` assets; any non-`/api`, non-asset path falls back to
  `index.html` so client-side deep links (`/templates/:id`, `/print`, …) load the SPA.
- Routing order: `/api` and asset routes are matched before the SPA fallback.
- Same change updates `docs/SPEC.md` (endpoint paths), the sample `scripts/*.sh`, and the existing
  backend HTTP tests in `src/lib.rs` (which currently hit root paths).

### Data layer

- **Typed API client** over `/api`. Types are generated from the OpenAPI doc (or hand-written and
  checked against it); responses are validated at the boundary, including the **binary-or-JSON**
  endpoints (`/batch` returns a blob on success or a JSON error, `/render/label` returns image/PDF
  bytes or JSON error), so the client branches on status + content-type before reading the body.
- **Server-state caching** (React Query or equivalent) for templates/printers/settings, with explicit
  invalidation after mutations (create/replace/delete template, printer CRUD, settings PUT).
- **Cross-screen prefill:** "Use to print" carries the chosen template (and any sample data) into the
  render screen via router state.
- **Toasts** are deduped (same code+message within a short window collapses).

### Reusable label grid + row model

The editable label grid (introduced for CSV, §4) is a standalone component, **not** private to the CSV
screen, so M7's Homebox record→label mapping materializes into the same grid with no rework. It is built
on **react-data-grid** (built-in cell editing, row/column virtualization, selection, keyboard a11y,
copy/paste, theming hooks), chosen over headless TanStack Table to avoid re-implementing spreadsheet
behavior, wrapped in a thin label-grid we own. The grid operates on a formalized row model:

```
LabelGridRow {
  id: string            // stable client row id (survives edits/duplication)
  origin: "csv" | "manual" | "connector"
  source?: { connector, connection, resource, key }  // set by M7; null for CSV/manual
  data: Record<string, string>                        // editable fields
  option: Record<string, string>                      // per-row template options
  copyGroup?: string    // links rows produced by a duplicate/copies expansion
  validation: { field?: errors; option?: errors }     // inline error state
  annotation?: { status: "ok" | "failed"; message?: string }  // from a print summary
}
```

CSV is its first consumer; M7 fills `origin: "connector"` + `source`. The Integrations screen/nav itself
is M7, not M5.

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
Landing page. Responsive grid of template cards: id (monospace chip), name, single/sheet badge, and a
**lazy, best-effort thumbnail**. Single cards render a small PNG via `/api/render/label` with sample
data; **sheet cards do not render a live page per card** (too expensive on a grid), they show a sheet
placeholder/badge and defer the real preview to the detail page. Thumbnails load lazily (on view) and
failures degrade to the placeholder. Search by id. Empty state. Card → detail. A **"new template"**
action opens a raw-YAML create view (the GUI editor is out of scope; this is a textarea posting to
`POST /api/templates`).

### 2. Template detail — `/templates/:id` (#17)
Large preview (single via `/api/render/label`; sheet via `/api/batch` `mode=download`), metadata (unit,
dpi, format + dimensions, declared options), the **referenced field names** (extracted client-side by
walking the template `layout` for `text`/`qr` `name`/`value` interpolation tokens), the **settings keys**
the template interpolates, a read-only collapsible **raw YAML source**, and "Use to print" (prefills
Render & Print). Single vs sheet indicated.

> **Backend touch (small):** the current `GET /api/templates/{id}` returns the normalized
> `TemplateDetail` (no raw YAML). The raw-YAML source view needs either a `?source` variant / a
> `GET /api/templates/{id}/source` endpoint returning the stored YAML, or the view is dropped.
> The plan must resolve this; "referenced fields" and "settings used" are derived client-side from the
> existing `layout`, no backend change.

### 3. Render & Print — `/print` (#20)
Two-pane. Left: template picker, a form auto-generated from the template's referenced fields (extracted
from `layout` as in §2), an options section (selects from declared `options`), printer select, and a
sheet-only **start slot** input (`start_slot`, hidden for single templates). Right: live preview and
**Print** (primary) / **Download** (secondary) actions.

- **Preview** is debounced (~300ms), gated on the required fields being present, cancellable
  (`AbortController` on each keystroke so a stale render can't overwrite a newer one), and cached by a
  hash of `{template, data, option, start_slot}` so re-renders of unchanged input are free. Single →
  `/api/render/label`; sheet → `/api/batch` `mode=download`.
- **Download** uses `/api/render/label?format=png|pdf` for a single template (one raw file) and
  `/api/batch` `mode=download` for a sheet (the PDF). **Print** posts a batch of one to `/api/batch`
  `mode=print` and shows the summary.
- Inline field errors; backend errors surface near the action and in a toast.

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
  the table (via `LabelGridRow.annotation`).
- **Cap + large files:** the backend caps a batch at 500 labels (`413 BatchTooLarge`). The grid shows
  the **expanded total** (`rows × copies`) and **disables Run above 500** with a clear message, rather
  than discovering it server-side; expansion is computed lazily (not materialized into a giant array
  until submit). CSV parsing is bounded (size limit, parsed off the main thread for large files) so the
  UI stays responsive.
- **Sheet templates:** a **start slot** input (`start_slot`) lets you skip used cells on a partly-used
  sheet; passed through to `/api/batch`. Hidden for single templates.
This re-scopes #24 to an editable grid (the reusable label-grid component above); it stays frontend-only
against `/api/batch`.

### 5. Settings & Printers — `/settings` (#23)
One page, two sections. *Settings*: key/value editor seeded with `qr_base_url`
(`GET /api/settings` for all, `PUT /api/settings/{key}` to upsert). *Printers*: table with
add/edit/delete (`/api/printers` CRUD), kind + config (cups uri), enabled toggle.

## Error handling

Field-level validation inline; API errors (the stable `{ error: { code, message, details } }` schema)
surface in toasts with the message, and near the triggering control where one exists. Row-level errors
have **two distinct shapes** the grid must adapt:

- **`mode=download` (atomic):** a bad batch fails with `422 BatchInvalid` and
  `details.failures: [{ index, code, message }]`; the grid maps each `index` to its row.
- **`mode=print` (best-effort):** `200` with `failed: [{ index, error }]` (no `code`); the grid
  annotates those rows as failed and the rest as ok.
- `413 BatchTooLarge` is prevented client-side (the cap check), but still handled if it slips through.

Binary endpoints (`/batch` download, `/render/label`) are read as a blob **only** on a 2xx with the
expected content-type; a non-2xx or `application/json` content-type is parsed as the error contract
first. Object URLs created for downloads are revoked after the save.

## Accessibility

The Ink & Tape theme is custom, so a11y is explicit, not assumed: WCAG-AA contrast for text and the
orange accent (checked in both light and dark tokens, the accent-on-paper combo is the risk), visible
focus rings, `prefers-reduced-motion` honored, keyboard cell editing/navigation in the grid (react-data-
grid provides the base), a focus trap on the mobile nav drawer, and `aria-live` on the toast region.

## Testing

- **Backend:** `/api` namespacing and SPA-fallback tests, `/api/templates` and `/api/docs` resolve,
  `/templates/:id` and asset URLs fall back to `index.html`, a bogus `/api/nope` returns the JSON 404
  (not the SPA). Existing root-path HTTP tests in `src/lib.rs` migrate to `/api`.
- **Frontend component:** the generated render form; the CSV grid (parse, inline edit, copies expansion,
  duplicate/remove/reset, option precedence, the 500-cap disabling Run); the settings/printers tables;
  the two row-error adapters (`BatchInvalid.details.failures` vs print `failed[]`); binary-download
  handling (content-type branch, filename, object-URL revoke); preview cancellation/stale-suppression.
- **e2e smoke** (Playwright): Templates → pick → fill → preview → download (single via `/render/label`);
  a sheet template preview via `/batch`; CSV load → edit a cell → run; settings `PUT`; a deep link and
  mobile-drawer nav. The batch/render layer runs against the real endpoints in e2e and a typed contract
  mock in component tests.

## Out of scope (M5)

GUI template editor (Later), interactive CSV field-mapping (Phase 2; header-name match only here),
API token auth, printer status read-back, and the **Integrations / Homebox** screen + nav (M7, which
reuses M5's label grid). Sheet printing and multi-page sheets are delivered by the unified batch
endpoint, not by the UI.

## Resolved since first approval

- Unified batch endpoint shipped (ADR-0011, #30); `/render/batch` + `/print` removed.
- Job options shipped (ADR-0012, #31); `start_slot` exposed in the render/print and CSV screens.
- Optional `option.<name>` columns for the self-contained `/import/csv` API filed as #32 (low priority,
  not part of this UI).
