# Labeler — Specification

**Status:** Living document. Update this file on every major decision or behavior change, and record
the decision as an ADR under [`docs/adr/`](adr/).

**Version:** 0.1.0

## 1. Overview

Labeler is a stateless REST service that renders labels from declarative YAML templates. It supports
two output modes:

- **Single label → PNG** (`POST /render/label`), for previews and one-off continuous-roll labels.
- **Batches of labels** (`POST /batch`), which dispatches on the template format: single templates
  yield a ZIP of per-label files (download) or one print job per label, sheet templates lay labels into
  slots across one paginated PDF.

Templates are loaded once at startup and held immutably. Rendering works by generating
[Typst](https://typst.app/) source on the fly and compiling it in-process via `typst-as-lib`
(`typst-render` for PNG, `typst-pdf` for PDF).

## 2. HTTP API

| Method | Path | Purpose | Success |
| --- | --- | --- | --- |
| GET | `/health` | Liveness check | `200 {"status":"ok"}` |
| GET | `/templates` | List template summaries (sorted by id) | `200 {"templates":[…]}` |
| POST | `/templates` | Create a template (raw YAML body) | `201` / `409` / `422` |
| POST | `/templates/reload` | Re-scan the templates dir | `200 {"count":N}` / `422` |
| GET | `/templates/{id}` | Full template detail incl. layout | `200` / `404` |
| PUT | `/templates/{id}` | Replace a template (raw YAML body) | `200` / `400` / `404` / `422` |
| DELETE | `/templates/{id}` | Delete a template | `204` / `404` |
| POST | `/render/label` | Render one label for preview (`?format=png\|pdf`) | `200 image/png` or `application/pdf` |
| POST | `/batch` | Render/print a batch of labels | `200` / `413` / `422` |
| GET / POST | `/printers` | List / create printers | `200` / `201` |
| GET / PUT / DELETE | `/printers/{id}` | Printer detail / replace / delete | `200` / `204` / `404` |
| GET | `/settings` | All settings as a key/value object | `200 {…}` |
| PUT | `/settings/{key}` | Upsert a setting | `200` / `400` |
| POST | `/import/csv` | Render one label per CSV row (ZIP download or per-row print) | `200` / `400` / `404` / `422` / `502` |
| GET | `/openapi.json` | OpenAPI 3 document | `200` |
| GET | `/docs` | Swagger UI | `200` |

The server binds `0.0.0.0:$PORT` (default `8080`).

> **Planned (ADR-0008):** when the web UI lands (M5), the REST API moves under an `/api` prefix
> (`/api/templates`, `/api/render/label`, …) and the root serves a React SPA. The paths in this section
> describe the current root-mounted API.

### 2.0 Template management

Templates are hand-authored YAML in the templates dir and may also be managed over the API. The
registry is held in an `ArcSwap` for lock-free reads; `reload` and every mutation rebuild it from disk
and swap atomically. A reload that fails (an invalid file on disk) returns `422` and keeps the
previously-loaded set, so a bad file never takes the service down.

- `POST /templates/reload` re-scans the dir and returns `{ "count": N }`.
- `POST /templates` creates from a raw YAML body; the `id` comes from the body; `409 Conflict` if it
  already exists.
- `PUT /templates/{id}` replaces from a raw YAML body; the body `id` must equal the path `{id}` (else
  `400`); `404` if it does not exist.
- `DELETE /templates/{id}` removes the template.

API-managed templates are written as `<id>.yaml` under the templates dir (atomic temp-then-rename), and
`id` must contain only letters, digits, `-`, or `_` (path-traversal guard). Parse errors and validation
failures return `422 TemplateInvalid` with a path-aware message; the GUI-owned store is Phase 2
(ADR-0006).

### 2.1 `POST /render/label`

```json
{
  "template": "brother12mm",
  "data": { "message": "Hello", "code": "QR-123" },
  "option": { "variant": "default" }
}
```

- Renders a single label as a preview or one-off; for multi-label work and printing, use `POST /batch`.
- `template` must reference a template whose `format.type` is `single`; otherwise `422 UnsupportedFormat`.
- `data` binds field names referenced by `text`/`qr` layout items.
- `option` is optional and validated against the template's declared `options`.
- `?format=png|pdf` (default `png`) selects the output: `image/png` (rasterized at the template DPI) or
  `application/pdf` (vector). An unknown value is `400 InvalidRequest`. The output is the raw image/PDF,
  not a ZIP.

### 2.2 `POST /batch`

One endpoint for rendering and printing batches of labels. It owns the format decision so clients never
branch on single vs. sheet: they post a list of resolved labels plus a `mode` and branch only on the
response (download yields a binary, print yields a JSON summary). See [ADR-0011](adr/0011-unified-batch-endpoint.md).

```json
{
  "template": "avery5163",
  "mode": "download",
  "start_slot": 0,
  "labels": [
    { "option": { "orientation": "horizontal", "outline": "yes" },
      "data": { "id": "A1", "url": "https://example.com/A1", "name": "…", "tags": "…", "description": "…" } }
  ]
}
```

- `mode` is `download` or `print`. `print` requires `printer` and rejects `format` (`400 InvalidRequest`).
- `format` (`png`|`pdf`) applies to single+download and selects the per-file render format. Sheet output
  is always PDF, so `format` is ignored for sheet templates.
- `start_slot` (default `0`) applies to sheet templates only: it is the zero-based index of the first
  slot on the first page. Supplying it for a single template is `400 InvalidRequest`.
- `printer` names a registered printer for `print` mode (unknown printer → `404`).

**Dispatch matrix** (by template `format.type` × `mode`):

| | `download` | `print` |
| --- | --- | --- |
| `single` | ZIP of per-label files (`application/zip`), one per label in the `format` render format | one print job per label |
| `sheet` | labels laid into slots, paginated across pages, as one `application/pdf` | that paginated PDF sent as a single print job |

**Validate-then-execute.** Every label is render-validated first. If any label has bad data, the request
returns `422 BatchInvalid` with `details.failures: [{ index, code, message }]` listing every failing
label, and nothing is produced or printed (atomic in both modes and both formats). Only once all labels
validate does the endpoint execute: `download` streams the blob; `print` dispatches jobs and returns a
`200` summary.

**Print summary (`BatchSummary`).** Print transport is best-effort: a label that fails to send is
reported, not fatal.

```json
{ "total": 3, "succeeded": 2, "failed": [{ "index": 1, "error": "…" }], "jobs": 1 }
```

`jobs` counts jobs dispatched: one per label for single templates, `1` for a sheet (the whole sheet is
one job). `failed[].index` is the zero-based label index.

- Batches over 500 labels return `413 BatchTooLarge`.
- `template` referencing an unknown id → `404`; `option` validated per the template's declared `options`.

## 3. Template schema

Templates are `*.yaml` / `*.yml` files in the `templates/` directory. Top-level fields:

| Field | Type | Notes |
| --- | --- | --- |
| `id` | string | Required, non-empty, unique across the directory. |
| `name` | string | Required, non-empty. |
| `description` | string | Optional. |
| `unit` | `"mm"` \| `"in"` | Length unit for all coordinates/sizes in the template. |
| `dpi` | integer > 0 | Raster resolution for PNG output. |
| `format` | object | See §3.1. |
| `options` | map | Optional. See §5. |
| `layout` | list | Tree of layout items. See §4. |
| `version` | string | Optional, free-form. |

Parsing rejects unknown fields (`deny_unknown_fields`). An invalid template aborts server startup.

### 3.1 `format`

Tagged by `type`:

**`single`** — one label of possibly dynamic size:

```yaml
format:
  type: single
  width: { min: 10.0, max: 100.0 }   # Dimension
  height: 12.0                        # Dimension
```

A `Dimension` is either a fixed number, or a dynamic object `{ min?, max? }` (at least one required).
Dynamic dimensions currently resolve to `max` (falling back to `min`) for both layout bounds and the
rendered page size.

**`sheet`** — a grid of identical label slots on a fixed page:

```yaml
format:
  type: sheet
  paper_width: 8.5
  paper_height: 11.0
  label_width: 4.0
  label_height: 2.0
  positions:           # bottom-left corner of each slot, page origin bottom-left
    - [0.18, 8.5]
    - [4.32, 8.5]
```

## 4. Layout

`layout` is an ordered list of layout items, rendered back-to-front (later items draw on top). Items
are tagged by `type`. All items share a **placement** (flattened into the item):

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `at` | `[x, y]` | `[0, 0]` | Lower-left anchor, in template units (see §6). |
| `size` | `[w, h]` | — | Each entry is a number or `auto`. |
| `max_w` / `max_h` | number | — | Upper bound used to resolve `auto`. |
| `rotate` | number (deg) | — | Rotates the rendered item. |

`auto` size resolves to `max_w`/`max_h` if present; for `container` it falls back to the parent frame's
dimension. A non-`auto` numeric size must be > 0. (`line` does not use `size`; see §4.1.)

### 4.1 Item types

- **`text`** — exactly one of `name` (data key) or `value` (interpolated template, see §8), plus
  placement, `font_size`, `multiline` (default `false`),
  `alignment` (`horizontal`: left/center/right, `vertical`: top/center/bottom).
  `font_size` is either a fixed number or a range `{ min, max }`. A range auto-shrinks the text to fit
  the box (0.5pt steps, `fontdue` metrics) and truncates with an ellipsis if it still overflows.
  Single-line text collapses spaces to non-breaking and renders only the first line.
- **`qr`** — exactly one of `name` (data key) or `value` (interpolated template, see §8), plus
  placement, optional `params`:
  `error_correction` (`L`/`M`/`Q`/`H`, default `M`), `module_size`, `quiet_zone`.
  Rendered as an SVG via the `qrcode` crate, embedded as a Typst image.
- **`image`** — exactly one of `src` (a path to a bundled asset, resolved under the assets root with a
  traversal guard) or `name` (a data key whose value is a base64 data URI, `data:<mime>;base64,...`),
  plus placement and optional `fit` (`contain` default, `cover`, `stretch`). Formats: PNG, JPEG, SVG.
  Bytes are decoded server-side and injected into Typst as a virtual file; there is no server-side URL
  fetching (see ADR-0009). The assets root is `LABELER_ASSETS_DIR` (default `assets/`). Missing data
  key → `MissingField`; bad base64 / unsupported format / asset path problems → `UnsupportedLayoutItem`.
- **`line`** — `at` (start, default `[0,0]`) and `to` (end), both absolute in frame coordinates, plus
  `thickness` (> 0). Lines have no box `size`/`fit`/rotation. Endpoints must differ and lie within the
  layout bounds.
- **`container`** — a recursive group. Fields: placement (size defaults to `auto`/`auto` = fill parent),
  optional `option` gate (§5), optional `frame` (`thickness` > 0, `rounded` bool), `padding`, and
  `items` (nested layout). Children are positioned relative to the container's padded inner box.
  `padding` is either a single number (uniform) or `[top, right, bottom, left]`; values must be ≥ 0;
  default `0`.

Layout item `name`s (text/qr, and a data-bound `image`) must be unique and non-empty within a sibling
list. `value`-based text/qr items are anonymous and are exempt from this check.

## 5. Options

A template may declare `options` as a map of option name → list of allowed values:

```yaml
options:
  orientation: [horizontal, vertical]
  outline: [yes]
```

- A request's `option` selection is validated: each key must exist and its value must be allowed,
  else `422 InvalidOptionValue`. Supplying `option` to a template without `options` is `400`.
- A `container` may carry an `option` map. The container (and its subtree) renders only when the
  request's selection matches all of the container's option entries. This is how one template supports
  multiple layouts (e.g. horizontal vs. vertical) — see `templates/avery5163.yaml`.

## 6. Coordinate system

All template coordinates use a **bottom-left origin with y pointing up**, expressed in the template's
`unit`. Typst uses a top-left origin, so the renderer converts every placement with
`dy = frame_height - (y + height)`. A `container` establishes a new coordinate frame: its children are
measured against the container's **padded inner** width/height, not the page.

When changing placement math, this conversion and the per-container reframing are the two things to get
right.

## 7. Rendering pipeline

1. **Parse** (`parse.rs`): YAML → `raw.rs` structs (`deny_unknown_fields`) using `serde_path_to_error`
   to attach a path to every error.
2. **Convert** (`convert.rs`): raw structs → domain model (`models.rs`) via `TryFrom`, normalizing
   shorthands (e.g. padding, default container size).
3. **Validate** (`templates.rs`): structural and bounds checks; recurses through containers.
4. **Render** (`render/mod.rs`): walk the layout, emit Typst markup (`#place`/`#box`/`#text`/`#image`/
   `#line`/`#rect`), compile, and encode PNG (single) or PDF (sheet).

Sizing/bounds logic is intentionally duplicated between validation (compile time) and rendering
(request time); the two must stay in sync.

## 8. Data binding

`text` and `qr` items bind in one of two ways (exactly one of `name` / `value`):

- `name` resolves a single data key against the request `data` map.
- `value` is an interpolated template string. `{field}` resolves from `data`, `{settings.<key>}`
  resolves from the settings store, and `{{` / `}}` emit literal braces. There are no operators or
  functions; this is substitution only (ADR-0010). Interpolation applies to text content and QR content.

A missing key or unresolved token is `422 MissingField`. JSON scalars are stringified
(`value_to_string`): strings as-is, numbers/bools via their textual form, `null` as empty, other values
via JSON.

## 9. Fonts

Inter is the only bundled font. It is embedded through `typst-kit` plus `fonts/InterVariable.ttf`
(also loaded by `fontdue` for text measurement). Typst is configured to use `"Inter Variable"`,
falling back to `"Inter"`.

## 10. Error model

All errors return JSON:

```json
{ "error": { "code": "TemplateNotFound", "message": "…", "details": { "template": "xyz" } } }
```

| Code | Status | When |
| --- | --- | --- |
| `TemplateNotFound` | 404 | Unknown template id. |
| `InvalidRequest` | 400 | Malformed JSON, bad path/param, out-of-range `start_slot`. |
| `UnsupportedMediaType` | 415 | Missing/incorrect `Content-Type`. |
| `InvalidOptionValue` | 422 | Option selection not allowed by the template. |
| `MissingField` | 422 | A referenced `data` field is absent. |
| `UnsupportedLayoutItem` | 422 | Layout item cannot be rendered (e.g. bad size/qr param). |
| `UnsupportedFormat` | 422 | Endpoint/format mismatch or unknown unit. |
| `BatchInvalid` | 422 | One or more `/batch` labels failed render-validation; `details.failures` lists them. |
| `BatchTooLarge` | 413 | A `/batch` request exceeds the label cap (500). |
| `RenderFailed` | 500 | Typst compile/encode failure. |

`code` strings are part of the contract — keep them stable.

## Printing

Architecture: [ADR-0007](adr/0007-printer-architecture-and-transport-model.md). App state (printers,
settings, a job log) lives in SQLite under the data dir (`LABELER_DATA_DIR`, default `data/`), behind a
`store` module.

- **Printers** are "machine" instances `{ id, name, kind, config, enabled }` with an opaque
  per-`kind` JSON `config`, managed via `/printers` CRUD (`id` is a validated slug). `kind` selects a
  `PrinterDriver`; create/replace validate the config for that driver.
- **Printing is driven by `POST /batch` with `mode: print`** (see §2.2). With a `printer`, it builds that
  printer's driver and sends each artifact in the driver's accepted format, recording a job and returning
  a `BatchSummary`. Single templates dispatch one job per label; sheet templates send the paginated PDF
  as one job. Unknown template/printer → 404; transport failures are best-effort and reported per-label
  in the summary. (`POST /print` was removed and absorbed by `/batch`; ADR-0011.)
- **Phase 1 driver:** `cups` sends the rendered PDF over IPP (pure-Rust `ipp` crate, no `lp` binary) to
  a CUPS queue or IPP-Everywhere printer URI. Later families (Zebra ZPL, Brother raster, Dymo) register
  as new drivers without changing dispatch.
- **Deferred:** printer status read-back, USB/browser printing. (Batch-to-printer and multi-page sheets
  are now delivered by `/batch`; `copies` is expanded client-side, so #28 is moot.)

## Settings

A generic key/value store backs integration settings, persisted in the SQLite `settings` table.
`GET /settings` returns all pairs as a JSON object; `PUT /settings/{key}` with `{ "value": "…" }`
upserts one (the key is a slug of letters, digits, `_`, `-`, `.`; otherwise `400`). Settings are
readable from templates through `{settings.<key>}` interpolation (see §8). The only key used in
Phase 1 is `qr_base_url`; the generic shape leaves room for later integration config (e.g. a Homebox
URL/token).

## CSV import

**`POST /import/csv?template=<id>&mode=download|print&printer=<id>&format=png|pdf`** renders one label
per CSV row. The request body is raw `text/csv`: the header row names the fields, each subsequent row
supplies one label's `data` (all values are strings). A leading UTF-8 BOM is stripped, and the `csv`
crate handles quoted fields. It targets single-format templates; sheet composition from rows is out of
scope (→ #28).

- **Structural CSV problems** are a whole-request precondition failure with `400` in **both** modes,
  reported before any rendering or printing: ragged rows (a row's field count differs from the header),
  empty or duplicate header column names, and no data rows.
Internally, `/import/csv` parses the CSV into labels and delegates to the shared `/batch` path
(ADR-0011), so it inherits the validate-then-execute model.

- **`mode=download`** (default) returns `application/zip` with one file per row, named by 1-based
  zero-padded row index (`001.png`, …) in the template's render format (`format` selects png/pdf).
  Download is **atomic** over per-row render failures of otherwise well-formed rows: any row that fails
  to render (e.g. unresolved interpolation field) fails the whole request with `422 BatchInvalid` and a
  `details.failures` list; no partial archive.
- **`mode=print`** requires `printer` (and rejects `format`). Because `/import/csv` shares the `/batch`
  path, sheet CSVs are supported: the rows compose a paginated PDF that prints as one job. For single
  templates it dispatches one print job per row (so a continuous-tape printer auto-cuts between labels),
  recording each job, and **continues past** per-row print transport failures. It returns `200` with a
  `BatchSummary` `{ total, succeeded, failed: [{ index, error }], jobs }`. Unknown template/printer → 404;
  disabled printer → 409.
- **Out of scope (v1):** per-row option selection, multipart upload.

## Changelog

- **2026-06-16**: Unified batch endpoint `POST /batch` (ADR-0011, #30). One endpoint that dispatches on
  template format (single → ZIP or per-label jobs, sheet → one paginated PDF or one job), with a
  validate-then-execute model (`422 BatchInvalid`), a label cap (`413 BatchTooLarge`), and a
  `BatchSummary` print response. `POST /print` and `POST /render/batch` were removed and absorbed;
  sheet printing and multi-page sheets are now delivered (previously deferred in #28). `/render/label`
  remains for single-label preview; `/import/csv` now shares the `/batch` path.
- **2026-06-15** — Added CSV import (`POST /import/csv`): one label per row, ZIP download (atomic)
  or per-row print jobs with a `{ total, succeeded, failed }` summary. Added a generic settings
  store with `GET /settings` and `PUT /settings/{key}`. Issues #21, #14.
- **2026-06-15** — Added a `value` field on `text`/`qr` items: a substitution interpolation string
  (`{field}` from request data, `{settings.<key>}` from the settings store, `{{`/`}}` literal braces;
  unresolved token → `422 MissingField`), as an exactly-one-of alternative to `name`. See ADR-0010 and
  the `homebox-qr` demo template. Issue #14.
- **Unreleased** — M3 state and printing: SQLite app-state store (#8), printer CRUD (#12), CUPS/IPP
  driver (#16), and `POST /print` with file download (#13) and printer dispatch (#19).
- **Unreleased** — Accepted ADR-0008 (web UI delivery: React SPA served by axum, API to move under
  `/api`); implementation is M5.
- **Unreleased** — Accepted ADR-0007 (printer architecture and transport model); implementation is M3.
- **Unreleased** — Added the `brother18mm` and `brother24mm` continuous-tape starter templates (#11).
- **Unreleased** — Template management API: `POST /templates/reload` (#7) and raw-YAML
  `POST`/`PUT /templates/{id}`/`DELETE /templates/{id}` (#10); registry is now runtime-mutable via
  arc-swap.
- **Unreleased** — `line` now uses explicit `at`/`to` endpoints instead of `size` as a delta (breaking
  template change). Issue #6.
- **Unreleased** — `POST /render/label` gained `?format=png|pdf` (single-label PDF output). Issue #4.
- **Unreleased** — Added the `image` layout item (static asset under the assets root, and data-bound
  base64 data URI; PNG/JPEG/SVG; injected into Typst as virtual files). See ADR-0009. Issue #3.
- **0.1.0** — Initial spec captured from the implemented service (single PNG + sheet PDF rendering,
  recursive containers, options gating, two-stage parsing). See ADRs 0001–0005.
