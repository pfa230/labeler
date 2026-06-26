# Labeler: Specification

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

All API routes are under `/api` (per ADR-0008). The root serves the React SPA (`ui/`, built by Vite to
`ui/dist`): hashed assets at `/assets/*` (a missing asset is a `404`, not HTML), and every other non-`/api`
path falls back to `index.html` for client-side routing. The served UI dir is `LABELER_UI_DIR`
(default `ui/dist`); if absent the root returns a "UI not built" `404`. An unknown `/api/*` path returns
`404 NotFound` (the JSON error contract), never an HTML page.

| Method | Path | Purpose | Success |
| --- | --- | --- | --- |
| GET | `/api/health` | Liveness check | `200 {"status":"ok"}` |
| GET | `/api/templates` | List template summaries (sorted by id) | `200 {"templates":[…]}` |
| POST | `/api/templates` | Create a template (raw YAML body) | `201` / `409` / `422` |
| POST | `/api/templates/reload` | Re-scan the templates dir | `200 {"count":N}` / `422` |
| GET | `/api/templates/{id}` | Full template detail incl. layout | `200` / `404` |
| GET | `/api/templates/{id}/source` | Raw stored template YAML | `200 text/yaml` / `400` / `404` |
| GET | `/api/templates/{id}/thumbnail` | Rendered PNG preview with placeholder data | `200 image/png` / `304` / `404` / `422` |
| PUT | `/api/templates/{id}` | Replace a template (raw YAML body) | `200` / `400` / `404` / `422` |
| DELETE | `/api/templates/{id}` | Delete a template | `204` / `404` |
| POST | `/api/render/label` | Render one label for preview (`?format=png\|pdf`) | `200 image/png` or `application/pdf` |
| POST | `/api/batch` | Render/print a batch of labels | `200` / `413` / `422` |
| GET / POST | `/api/printers` | List / create printers | `200` / `201` |
| GET / PUT / DELETE | `/api/printers/{id}` | Printer detail / replace / delete | `200` / `204` / `404` |
| GET | `/api/variables` | All template variables as a key/value object | `200 {…}` |
| PUT | `/api/variables/{key}` | Upsert a variable | `200` / `400` |
| GET | `/api/settings` | Resolved app settings (effective value + `is_default` per key) | `200` |
| PUT / DELETE | `/api/settings/{key}` | Set an override / reset to default | `200` / `204` / `400` / `404` |
| POST | `/api/print` | Inbound print webhook: single label, N copies, to a named printer | `200` / `400` / `404` / `409` / `413` / `422` / `502` |
| POST | `/api/import/csv` | Render one label per CSV row (ZIP download or per-row print) | `200` / `400` / `404` / `422` / `502` |
| POST | `/api/auth/setup`, `/api/auth/login`, `/api/auth/logout` | First-run setup / login / logout | see §11 |
| GET | `/api/auth/me` | SPA auth state | `200` |
| POST | `/api/auth/password` | Change own password | see §11 |
| GET / POST / DELETE | `/api/users`, `/api/users/{id}` | User management (flat) | see §11 |
| GET / POST / DELETE | `/api/tokens`, `/api/tokens/{id}` | API-token management | see §11 |
| GET / POST | `/api/connections`, `/api/connections/{id}` | Connection CRUD (credential redacted) | see §12 |
| GET | `/api/connections/{id}/schema` | Connector schema (resources, fields, filters) | `200` / `404` / `502` |
| POST | `/api/connections/{id}/browse` | Page through a resource's rows | `200` / `400` / `404` / `502` |
| POST | `/api/connections/{id}/materialize` | Selected rows to label data | `200` / `400` / `404` / `502` |
| GET | `/api/openapi.json` | OpenAPI 3 document | `200` |
| GET | `/api/docs/` | Swagger UI (trailing slash) | `200` |

Route references in the sections below are written relative to `/api` for brevity (e.g. `POST /batch`
means `POST /api/batch`).

The server binds `0.0.0.0:$PORT` (default `8080`). **Every `/api` route requires authentication** (a
session cookie or a `Authorization: Bearer` token) except `/api/health`, `/api/auth/login`,
`/api/auth/setup`, `/api/auth/me`, `/api/openapi.json`, and `/api/docs`; see §11.

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
- `GET /templates/{id}/source` returns the raw stored YAML (`text/yaml`) for the read-only source view;
  `400` on an invalid id, `404` if the file is missing.
- `GET /templates/{id}/thumbnail` renders a representative PNG for the template using placeholder data
  (each field filled with its field name, each QR code encoded with its field name). For `single`
  templates it renders the one label. For `sheet` templates it renders a single label slot (not a full
  sheet) so the preview is label-sized regardless of format. Variables (`{vars.X}`) are resolved from
  the store; undefined variable references cause a `422`. The default option selection (first allowed
  value per option key) is used automatically. The response carries `ETag` (a quoted SHA-256 of the
  template YAML) and `Cache-Control: no-cache`; callers that send `If-None-Match` with a matching ETag
  receive `304 Not Modified`. Error codes: `404 TemplateNotFound` for unknown ids, `422` for
  render/interpolation failures.

The CSV Import and Homebox Connect pages render an on-demand, selected-row preview using the same
endpoints: `POST /render/label` for single templates, `POST /batch` (mode `download`, one label) for
sheet templates. The preview reuses the full option-resolution path (the same one download/print use),
so the preview matches the actual rendered output. A preview failure (non-2xx) surfaces inline in the
`PreviewPane` and never disables Download or Print.

API-managed templates are written as `<id>.yaml` under the templates dir (atomic temp-then-rename), and
`id` must contain only letters, digits, `-`, or `_` (path-traversal guard). Parse errors and validation
failures return `422 TemplateInvalid` with a path-aware message; the GUI-owned store is Phase 2
(ADR-0006).

### 2.1 `POST /render/label`

```json
{
  "template": "brother_18mm_qr",
  "data": { "message": "Hello", "code": "QR-123" }
}
```

- Renders a single label as a preview or one-off; for multi-label work and printing, use `POST /batch`.
- `template` must reference a template whose `format.type` is `single`; otherwise `422 UnsupportedFormat`.
- `data` binds field names referenced by `text`/`qr` layout items.
- `option` is optional and validated against the template's declared `options`.
- `?format=png|pdf` (default `png`) selects the output: `image/png` (rasterized at the template DPI) or
  `application/pdf` (vector). An unknown value is `400 InvalidRequest`. The output is the raw image/PDF,
  not a ZIP.
- `?color_mode=color|bilevel` (default `color`): post-processing applied to the PNG raster after Typst
  renders it. `bilevel` converts the image to pure 1-bit black/white by applying a global luminance
  threshold (no dithering in this slice); it is useful for preparing a download that matches what a
  bilevel printer will produce. `bilevel` is PNG-only: combining `format=pdf` with
  `color_mode=bilevel` is `400 InvalidRequest`. Requires `format=png` (explicit or default). See
  [ADR-0033](adr/0033-capability-aware-rendering.md) (this is slice 1; printer-driven color-mode and
  resolution selection come in a later slice).
- `?resolution=<dpi>` (default: the template `dpi` field): integer DPI override for the PNG raster. Lets
  callers preview at a target device resolution without editing the template. Valid range is `[1, 1200]`;
  a non-numeric value, `0`, or a value above `1200` is `400 InvalidRequest`. PNG only (ignored for PDF,
  since PDF is vector). See [ADR-0033](adr/0033-capability-aware-rendering.md).

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

`start_slot` is the first **job option**: a job-level knob intrinsic to the template's format (distinct
from per-row `data` and per-row template `options`), passed as an optional `/batch` field and validated
against the format. Future job options (skip arbitrary sheet slots, per-job margins, continuous-tape
cut/gap) follow the same pattern and are catalogued in [ADR-0012](adr/0012-job-options.md); none are
implemented yet.

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

### 2.3 `POST /print`

An inbound print webhook for integrations (Grocy, scripts, local automation). It prints one label,
optionally repeated, to a named printer. See [ADR-0031](adr/0031-inbound-print-webhook.md).

```json
{
  "template": "brother_24mm",
  "printer":  "office",
  "fields":   { "name": "Tomato Soup", "qty": "3" },
  "option":   { "orientation": "horizontal" },
  "copies":   2
}
```

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `template` | string | Yes | Template id; `404` if not found. |
| `printer` | string | Yes | Printer id; `404` if not found, `409` if disabled. |
| `fields` | object | No (defaults to `{}`) | Mapped to the label `data` for field binding. |
| `option` | object | No | Template variant selection; validated against the template's declared `options`. |
| `copies` | integer 1..100 | No | Number of label instances to print (default `1`). |

`copies` counts **label instances**, not printer copies. A value of 2 sends two identical labels: two
separate print jobs for `single`/tape templates, or two slots on a sheet (paginated like `/batch`).
Callers needing more than 100 copies should use `POST /batch` directly.

**Response.** Always `BatchSummary` on success:

```json
{ "total": 2, "succeeded": 2, "failed": [], "jobs": 2 }
```

(`jobs: 2` is for a `single`/tape template with `copies: 2`; a `sheet` template would report `jobs: 1`.)

`total` and `succeeded` count label instances (copies). `jobs` counts actual printer dispatches: equal
to `copies` for `single`/tape templates; fewer for `sheet` templates (the sheet is paginated and sent
as a single print job per page, mirroring `/batch`). Send failures are reported in `failed[]` with a
`200`; they are not fatal (mirrors `/batch` best-effort transport).

**Error contract.**

| Status | Code | When |
| --- | --- | --- |
| 400 | `InvalidRequest` | Malformed JSON or `copies` outside `[1, 100]`. |
| 404 | `TemplateNotFound` | Unknown `template` id. |
| 404 | `PrinterNotFound` | Unknown `printer` id. |
| 409 | `PrinterDisabled` | Printer exists but is disabled. |
| 413 | `PayloadTooLarge` | Request body exceeds 64 KiB. |
| 422 | `BatchInvalid` | A rendered label is invalid (same render path as `/batch`). |
| 502 | `PrintFailed` | Pre-send dispatch failure (before any job is accepted). |

**Trusted-LAN posture.** This endpoint is intended for a trusted LAN (homelab, local automation bus).
Do not expose it to the internet. The API token is the access gate; there is no IP allowlisting or
rate limiting beyond normal auth middleware. `LABELER_NO_AUTH=true` disables authentication uniformly
for local use.

**Example.**

```bash
curl -X POST http://labeler.lan:8080/api/print \
  -H "Authorization: Bearer $LABELER_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"template":"brother_24mm","printer":"office","fields":{"name":"Tomato Soup","qty":"3"},"copies":2}'
```

## 3. Template schema

Templates are `*.yaml` / `*.yml` files in the templates directory (`LABELER_TEMPLATES_DIR`, default
`templates/`). Top-level fields:

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

A `Dimension` is either a fixed number, or a dynamic object `{ min, max }` (both required when used
on `format.width` of a `single` template). A `single` template with a dynamic `format.width` is
**auto-length**: the label width is determined at render time by measuring the content, clamping to
`[min, max]`, and choosing the largest font that fits the budget (then ellipsis if still over). `auto`
item width on a dynamic-width label resolves to the content width (`label_width - at.x`). Both `min`
and `max` must be present; a missing bound is `422 TemplateInvalid`. Sheet templates and fixed-width
single templates are unaffected. See [ADR-0026](adr/0026-auto-length-dynamic-width.md) and
[ADR-0030](adr/0030-multiline-auto-length-tape.md).

**Multiline text on dynamic-width singles.** `multiline: true` is supported on auto-length `single`
templates (ADR-0030). Wrapping uses the item's auto-width budget (`width.max - at.x`, minus container
padding). The renderer shrinks from `font_size.max` toward `font_size.min` (0.5 pt steps) until the
text fits; line count is emergent (`floor(available_height / line_height)` at the chosen font size,
where the available height is the text item's box height). If the
content still overflows at `font_size.min`, the fitting lines are kept and the last is ellipsized.
The tape label extent is `at.x + longest_wrapped_line_width`, clamped to `[width.min, width.max]`.
Wrapped lines are precomputed in the measurement pass and emitted verbatim in the render pass
(each line's spaces replaced with non-breaking spaces so Typst cannot re-break them).

**`alignment.vertical` on auto-length items.** The schema default is `top`. Auto-length items that
omit `alignment.vertical` use `top`. To keep text centered, set `vertical: center` explicitly. The
bundled tape templates already set `vertical: center` and are unaffected.

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
dimension. On a dynamic-width `single` template (§3.1), `auto` width resolves to the content width
(`label_width - at.x`) derived from the pre-render measurement pass. A non-`auto` numeric size must
be > 0. (`line` does not use `size`; see §4.1.)

### 4.1 Item types

- **`text`** — exactly one of `name` (data key) or `value` (interpolated template, see §8), plus
  placement, `font_size`, `multiline` (default `false`),
  `alignment` (`horizontal`: left/center/right, `vertical`: top/center/bottom; default `top`).
  `font_size` is either a fixed number or a range `{ min, max }`. A range auto-shrinks the text to fit
  the box (0.5pt steps, `fontdue` metrics) and truncates with an ellipsis if it still overflows.
  Single-line text collapses spaces to non-breaking and renders only the first line. On dynamic-width
  `single` templates, `multiline: true` is also supported; see §3.1 for the wrap and sizing rules.
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
- `value` is an interpolated template string. Tokens are resolved in precedence order, then `{{`
  and `}}` emit literal braces. There are no operators or functions; this is substitution only
  (ADR-0010). Interpolation applies to text content and QR content.

**Token types and precedence** (highest to lowest):

1. **`{datetime}`** (bare) resolves to the current local date formatted as ISO `%Y-%m-%d`
   (e.g. `2026-06-25`). Always succeeds; no configuration required.
2. **`{datetime.<name>}`** resolves a named strftime format from the `datetime_formats` app setting
   (e.g. `{datetime.iso_date_time}` with the default `iso_date_time` format yields
   `2026-06-25 14:30`). An unknown `<name>` is `422 MissingField`. The `datetime` namespace is
   reserved; a data field or variable with the same name is shadowed. See [ADR-0028](adr/0028-datetime-interpolation-token.md).
3. **`{vars.<key>}`** resolves from the variables store.
4. **`{field}`** resolves from the request `data` map.

`now` is captured once per render request (a single `Local::now()` call), so every datetime token
on a multi-label sheet shows the same instant. The server-local timezone (controlled by `TZ`)
applies.

A missing key or unresolved token is `422 MissingField`. JSON scalars are stringified
(`value_to_string`): strings as-is, numbers/bools via their textual form, `null` as empty, other values
via JSON.

## 9. Fonts

Inter is the only bundled font. It is embedded through `typst-kit` plus `InterVariable.ttf`
(also loaded by `fontdue` for text measurement). The fonts directory is `LABELER_FONTS_DIR` (default
`fonts/`); a custom fonts dir must contain `InterVariable.ttf`. Typst is configured to use
`"Inter Variable"`, falling back to `"Inter"`.

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
| `TemplateInvalid` | 422 | Template fails structural validation (e.g. a dynamic `format.width` missing one bound). |
| `UnsupportedFormat` | 422 | Endpoint/format mismatch or unknown unit. |
| `BatchInvalid` | 422 | One or more `/batch` labels failed render-validation; `details.failures` lists them. |
| `BatchTooLarge` | 413 | A `/batch` request exceeds the label cap (500). Note: `/batch` (and other JSON endpoints) can also return 413 with code `PayloadTooLarge` if the raw body exceeds the server's global limit. |
| `PayloadTooLarge` | 413 | Request body exceeds the configured limit. On `POST /print` the limit is 64 KiB; on other JSON endpoints it is the server's global default (~2 MiB). Applies API-wide to any endpoint that reads a JSON body. |
| `NotFound` | 404 | Unknown `/api/*` route (the API fallback). |
| `RenderFailed` | 500 | Typst compile/encode failure. |
| `SettingNotFound` | 404 | Unknown application setting key. |

`code` strings are part of the contract; keep them stable. Authentication adds `Unauthorized` (401)
and `Forbidden` (403); see §11.

## 11. Authentication

Decision: [ADR-0017](adr/0017-app-authentication.md). Flat authentication (real user accounts, no
roles): every authenticated user is equal and may manage users and API tokens.

**Gating.** Every `/api` route requires authentication except `GET /api/health`,
`POST /api/auth/login`, `POST /api/auth/setup`, `GET /api/auth/me`, `GET /api/openapi.json`, and
`/api/docs`. A request with neither a valid session cookie nor a valid bearer token gets
`401 Unauthorized`.

**Credentials.** One middleware resolves the caller in order:

- `Authorization: Bearer <token>`: machine/automation. The token is hashed and looked up in
  `api_tokens`; a hit authenticates as a machine principal. Token requests skip the origin check.
- Session cookie (`labeler_session`): browser. The cookie value is hashed and looked up in `sessions`
  joined to `users`, checking expiry and that the user still exists.
- Otherwise `401`.

**Session cookie.** Opaque 256-bit random value, stored server-side only as a SHA-256 hash, set
`HttpOnly`, `SameSite=Lax`, `Path=/`, and `Secure` when the effective scheme is https
(`LABELER_TRUST_PROXY=true` honors `X-Forwarded-Proto` behind a TLS-terminating proxy). 30-day sliding
expiry with throttled writes. Login rotates the session id; logout deletes the row and clears the
cookie.

**Origin check (CSRF).** Every cookie-authenticated state-changing request (POST/PUT/DELETE/PATCH),
including `login`/`setup`/`logout`, must carry an `Origin` (or `Referer`) whose authority matches the
request `Host`, else `403 Forbidden`. Bearer-token requests are exempt.

**First-run setup.** While zero users exist, `POST /api/auth/setup` `{username, password}` creates the
first account and logs it in; afterwards it returns `409`. For headless deploys, `LABELER_INIT_USER`/
`LABELER_INIT_PASSWORD` seed the first user at startup when no users exist (see `docs/DEPLOY.md`).

**API tokens.** A token is a random 256-bit secret (display prefix `lbl_`) returned once at creation and
stored only as a SHA-256 hash. Automation sends it as `Authorization: Bearer $LABELER_API_TOKEN`.

**Endpoints.**

| Method | Path | Purpose | Success |
| --- | --- | --- | --- |
| POST | `/api/auth/setup` | Create the first user (only while zero users exist); logs in | `200` / `409` |
| POST | `/api/auth/login` | Verify credentials; set a session cookie | `200` / `401` / `403` |
| POST | `/api/auth/logout` | Delete the session; clear the cookie | `200` / `401` / `403` |
| GET | `/api/auth/me` | Auth state for the SPA (`authed`, `needsSetup`, optional `me`) | `200` |
| POST | `/api/auth/password` | Change own password (verifies current); revokes other sessions | `200` / `401` / `403` |
| GET / POST | `/api/users` | List / create users (flat) | `200` / `201` |
| DELETE | `/api/users/{id}` | Delete a user (cannot delete the last user or your own account) | `204` / `404` / `409` |
| GET / POST | `/api/tokens` | List tokens / create a token (secret shown once) | `200` / `201` |
| DELETE | `/api/tokens/{id}` | Revoke a token | `204` / `404` |

`GET /api/auth/me` is auth-exempt and always returns `200` with `{ authed, needsSetup, me? }`. Deleting
a user cascades their sessions; a user cannot delete their own account (it would cascade the caller's
own session and silently log them out), so that request is rejected `409`. Changing a password revokes
the user's other sessions but keeps the current one. Passwords are argon2id; secrets at rest (sessions,
tokens) are SHA-256 hashes.

Passwords must be non-empty (no length minimum). Setting `LABELER_NO_AUTH=true` turns authentication off
for LAN-trust homelab use (never the default): data routes are open via an internal local principal,
but the credential-management endpoints (`/auth/setup`, `/auth/login`, `/auth/logout`, `/auth/password`,
`/users`, `/tokens`) return `403` so no durable credential can be created through the API while auth is
off (the operator `LABELER_INIT_USER` startup bootstrap is unaffected), and a relaxed origin check still
rejects browser drive-by writes with a mismatched `Origin`. `GET /api/auth/me`
then returns `{ authed: true, needsSetup: false, me: { id: "local", username: "local" }, noAuth: true }`,
and the SPA hides the login wall and credential-management UI.

## 12. Integrations (connectors)

Decision: [ADR-0018](adr/0018-api-integration-spine.md). A connector pulls label data straight from an
external system of record (first Homebox) so a user can browse and materialize rows instead of re-keying.

**Connections.** A connection is `{ id, connector, name, base_url, credential, enabled }` stored in
SQLite. The credential (a pasted Homebox API key) is stored as-is for now (at-rest encryption deferred)
and is **never** returned by the API: responses expose only `has_credential`.

| Method | Path | Purpose | Success |
| --- | --- | --- | --- |
| GET | `/api/connections` | List connections (credential redacted) | `200` |
| POST | `/api/connections` | Create a connection (`connector`, `name`, `base_url`, `credential`) | `201` / `400` |
| GET | `/api/connections/{id}` | Connection detail (credential redacted) | `200` / `404` |
| PUT | `/api/connections/{id}` | Update; omitting `credential` keeps the stored one | `200` / `404` |
| DELETE | `/api/connections/{id}` | Delete a connection | `204` / `404` |

`POST` rejects an unknown `connector`, a missing `credential`, or an invalid `base_url` with `400`.

**Browse model.** A connector describes its data as a schema and is paged through with browse, then a
selection is turned into label data with materialize.

- **`GET /connections/{id}/schema`** returns `{ version, resources, relationships }`. A resource is
  `{ id, label, view, columns, filters }`; `view` is `table` or `tree`. Each column (`FieldSpec`) carries
  a `tier`: `cheap` (free from the list call), `hydrated` (needs a per-row fetch), or `derived`
  (computed). Filters (`FilterSpec`) are typed (`search`, `location_id`, `label_id`).
- **`POST /connections/{id}/browse`** takes `{ resource, filters?, parent?, cursor?, page_size? }` and
  returns `{ rows, next_cursor, has_more, count? }`. Each row is `{ id: { resource, key }, cells, url? }`
  (`url` is the row's link to its page in the source system, used to make the name clickable).
  Cursors are opaque, HMAC-signed, and bound to {connector, connection, resource, filter, page}; the
  signing key is per process lifetime, so cursors do not survive a restart (the UI re-browses).
- **`POST /connections/{id}/materialize`** takes `{ rows: [{ resource, key }], fields, expansion }` and
  returns label rows `[{ source, data }]`, where `data` is a string map ready to bind to a template.

**Egress policy.** All outbound calls go through one shared `reqwest`/rustls client with connect/read
timeouts, an 8 MiB streamed response cap, no redirects, and no proxy-env use; bearer tokens are redacted
from logs and errors. The target IP is allow-checked: loopback, link-local, unspecified, and multicast
are blocked, while private LAN ranges are allowed (the target Homebox commonly lives on the LAN).
Upstream failures surface as `502`; bad filters and budget overruns as `400`.

**Using a connection (UI).** Settings > Connections adds and edits connections (connector, name, base
URL, API key). The key is write-only: the API returns only `has_credential`, the form shows it as a
password field, and editing with the field left blank keeps the stored key. The Connect page drives the
flow top to bottom: pick a connection (header), then a template and field mapping, then browse the
connector (a generic schema-driven table with typed filters, cursor pagination, and direct drill-down via
relationships) and select rows. Each row's name links to its page in the source system. Selection
persists across filters, drill-down, and resource tabs; a persistent summary shows the whole selection
with a visible/hidden split ("in this view" = currently-loaded rows) plus a reviewable, removable list
grouped by resource, so a bulk add never silently includes unseen rows. Selecting is blocked at the
200-row materialize cap. Materialize turns the selection into label-grid rows; the grid/batch caps at 500
(§2.2). For Homebox specifically, the connector lists items and locations as two flat resources off the
unified `/v1/entities` endpoint (`isLocation=false`/`true`); see [ADR-0021](adr/0021-homebox-connect-hardening.md).

## Printing

Architecture: [ADR-0007](adr/0007-printer-architecture-and-transport-model.md). App state (printers,
variables, a job log) lives in SQLite under the data dir (`LABELER_DATA_DIR`, default `data/`), behind a
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
  as new drivers without changing dispatch. The cups config schema is:

  | Field | Type | Required | Notes |
  | --- | --- | --- | --- |
  | `uri` | string | Yes | Must start with `ipp://` or `ipps://`. |
  | `username` | string | No | Used only when `password` is also set. |
  | `password` | string | No | **Write-only.** Never returned in API responses (omitted, not nulled). On `PUT`, omit to keep the stored value; send `null` to clear it; send a string to replace it. Stored plaintext (must be replayed to the printer). |
  | `ca_cert` | string | No | Inline PEM certificate (`-----BEGIN CERTIFICATE-----`). Trusted only for this printer's TLS handshake. Ignored when `insecure` is `true`. |
  | `insecure` | bool | No | Default `false`. When `true`, TLS certificate verification is skipped entirely (`insecure` dominates `ca_cert`). Combining `insecure` with credentials enables MITM credential theft; use only in isolated lab setups. |

  Credentials sent over `ipp://` (not `ipps://`) travel unencrypted; use `ipps://` for any
  credential-carrying connection. See [ADR-0032](adr/0032-ipp-auth-custom-ca.md).
- **Deferred:** printer status read-back, USB/browser printing. (Batch-to-printer and multi-page sheets
  are now delivered by `/batch`; `copies` is expanded client-side, so #28 is moot.)

## Variables

A generic key/value store holds template substitution variables, persisted in the SQLite `variables`
table. `GET /variables` returns all pairs as a JSON object; `PUT /variables/{key}` with `{ "value": "…" }`
upserts one (the key is a slug of letters, digits, `_`, `-`, `.`; otherwise `400`). Variables are
readable from templates through `{vars.<key>}` interpolation (see §8). The only key used in
Phase 1 is `qr_base_url`. This store is for *content* interpolated into labels; typed application
configuration (e.g. job-log retention) is a separate concern, kept out of the interpolation namespace
(ADR-0020).

## Settings

Application settings are typed configuration, stored separately from `variables` and never interpolated
into labels. `GET /api/settings` returns the resolved value for every known setting with an `is_default`
flag, so the effective config is always visible; a row is written only when an operator overrides a
default. `PUT /api/settings/{key}` validates the value per setting, and `DELETE /api/settings/{key}`
resets it to the in-code default (idempotent). Unknown keys are `404 SettingNotFound`.

| Method | Path | Purpose | Status |
| --- | --- | --- | --- |
| GET | `/api/settings` | Resolved app settings (effective value + `is_default` per key) | `200` |
| PUT | `/api/settings/{key}` | Set an override (validated per setting) | `200` / `400` / `404` |
| DELETE | `/api/settings/{key}` | Reset a setting to its in-code default (idempotent) | `204` / `404` |

Known settings:

- **`job_log_retention_days`** (default `90`; `0` disables job-log pruning). The daily prune reads
  the live value, so changes take effect without a restart.
- **`datetime_formats`** (default: a JSON object with five seeded entries). A JSON object mapping
  format names to strftime patterns. Patterns are validated via `chrono::format::StrftimeItems` at
  write time; an invalid pattern is `400`. Used by `{datetime.<name>}` interpolation (see §8).
  Seeded defaults: `iso_date` (`%Y-%m-%d`), `iso_date_time` (`%Y-%m-%d %H:%M`),
  `short_date` (`%m/%d/%Y`), `long_date` (`%B %-d, %Y`), `time` (`%H:%M`).

## CSV import

**`POST /import/csv?template=<id>&mode=download|print&printer=<id>&format=png|pdf`** renders one label
per CSV row. The request body is raw `text/csv`: the header row names the fields, each subsequent row
supplies one label's `data` (all values are strings). A leading UTF-8 BOM is stripped, and the `csv`
crate handles quoted fields. A header named `option.<name>` is routed to that row's template **option**
(not `data`); any declared option the CSV omits defaults to its first allowed value, and a disallowed
value fails the row (`BatchInvalid` / `InvalidOptionValue`). Output follows the template format via the
shared `/batch` path: single templates yield per-row artifacts, sheet templates compose the rows into
paginated pages.

The web UI's CSV Import screen (`/import`, ADR-0014, ADR-0022) is a separate client-side path: it parses
and edits the CSV in the browser and posts resolved labels to `POST /api/batch`. A CSV can be loaded
before any template is chosen (data columns show; option columns and validation activate once a template
is selected), and the loaded CSV survives switching templates. Every declared option is an always-present
per-row column defaulting to its first allowed value (a CSV `option.<name>` value wins), with a per-option
"Apply to all rows" control; single-valued options are read-only. It does not call `/api/import/csv`,
which remains the self-contained automation endpoint.

- **Structural CSV problems** are a whole-request precondition failure with `400` in **both** modes,
  reported before any rendering or printing: ragged rows (a row's field count differs from the header),
  empty or duplicate header column names, and no data rows.
Internally, `/import/csv` parses the CSV into labels and delegates to the shared `/batch` path
(ADR-0011), so it inherits the validate-then-execute model.

- **`mode=download`** (default): for a single template, returns `application/zip` with one file per row,
  named by 1-based zero-padded index (`001.png`, …) in the requested `format` (png/pdf); for a sheet
  template, returns one composed `application/pdf`. Download is **atomic** over per-row render failures
  of otherwise well-formed rows: any row that fails to render (e.g. unresolved interpolation field) fails
  the whole request with `422 BatchInvalid` and a `details.failures` list; no partial output.
- **`mode=print`** requires `printer` (and rejects `format`). Because `/import/csv` shares the `/batch`
  path, sheet CSVs are supported: the rows compose a paginated PDF that prints as one job. For single
  templates it dispatches one print job per row (so a continuous-tape printer auto-cuts between labels),
  recording each job, and **continues past** per-row print transport failures. It returns `200` with a
  `BatchSummary` `{ total, succeeded, failed: [{ index, error }], jobs }`. Unknown template/printer → 404;
  disabled printer → 409.
- **Out of scope (v1):** multipart upload. (Per-row option selection via `option.<name>` columns is now supported, #32.)

## Changelog

- **2026-06-26**: `POST /render/label` gains two optional PNG render params (ADR-0033 slice 1; #92).
  `?color_mode=color|bilevel` (default `color`): `bilevel` post-processes the raster to pure 1-bit
  black/white via a global luminance threshold (no dithering). PNG only; `format=pdf` +
  `color_mode=bilevel` is `400 InvalidRequest`. `?resolution=<dpi>` (default: template `dpi`):
  integer DPI override in `[1, 1200]`; non-numeric, `0`, or above `1200` is `400 InvalidRequest`.
  See §2.1.

- **2026-06-26**: CUPS driver now supports basic-auth, custom-CA PEM, and insecure skip-verify (ADR-0032; #39). The `cups` printer config expands to `{ uri, username?, password?, ca_cert?, insecure? }`. `password` is write-only: never returned in API responses (key omitted entirely). On `PUT /printers/{id}`, omit `password` to keep the stored value, send `null` to clear it, or send a string to replace it. `ca_cert` is an inline PEM certificate trusted only for that printer's TLS. `insecure: true` skips TLS verification and overrides `ca_cert`; combining it with credentials on an untrusted network risks MITM credential theft. Credentials over `ipp://` travel unencrypted; use `ipps://`. See the Printing section for the full field table.

- **2026-06-26**: Oversized JSON bodies now return `413 PayloadTooLarge` API-wide (#22). Any endpoint that reads a JSON body returns 413 when the body exceeds the configured limit (64 KiB on `POST /print`; the server's global default ~2 MiB on other endpoints). Previously these cases returned 400.

- **2026-06-26**: Inbound print webhook (ADR-0031; #22). `POST /print` accepts a flat payload
  (`template`, `printer`, `fields`, `option?`, `copies?`) and prints one label, optionally repeated
  N times (default 1, max 100), delegating to the shared `/batch` dispatch path. `copies` counts
  label instances; `total`/`succeeded` in the `BatchSummary` count instances; `jobs` counts actual
  printer dispatches (mirrors `/batch`). Request body capped at 64 KiB; oversized bodies map to
  `413 PayloadTooLarge`. Send failures are reported in `failed[]` with a `200` (best-effort
  transport, mirrors `/batch`). Intended for trusted-LAN integrations (Grocy, scripts); do not
  expose to the internet. See §2.3.

- **2026-06-25**: The templates and fonts directories are now env-configurable (#38) via
  `LABELER_TEMPLATES_DIR` (default `templates/`) and `LABELER_FONTS_DIR` (default `fonts/`), mirroring
  `LABELER_DATA_DIR`/`LABELER_UI_DIR`. A custom fonts dir must contain `InterVariable.ttf` (used by both
  Typst glyph search and `fontdue` measurement). Defaults are unchanged, so existing deploys and the
  container image are unaffected.

- **2026-06-25**: Current-time interpolation token (#76; ADR-0028). `{datetime}` (bare) resolves to
  the current local date in ISO format (`%Y-%m-%d`). `{datetime.<name>}` resolves a named strftime
  format from the new `datetime_formats` app setting (default: five seeded entries). An unknown name
  is `422 MissingField`. Token precedence: datetime, then `vars.`, then data. `now` is captured once
  per render request. See §8 (updated token list) and the Settings section (`datetime_formats`).
- **2026-06-25**: Multiline text on dynamic-width `single` (continuous-tape) templates is now
  supported (ADR-0030; #78). Text wraps inside the auto-width budget (`width.max - at.x`), font
  shrinks from `font_size.max` toward `font_size.min` (line count emergent), last line ellipsized on
  overflow. Tape extent = `at.x + longest wrapped line`, clamped to `[min,max]`. Wrapped lines are
  precomputed and emitted NBSP-treated so Typst cannot re-break them. `alignment.vertical` is now
  honored on all auto-length items (schema default `top`; the bundled tape templates set
  `vertical: center` explicitly and are unaffected). A bundled example template
  `brother_24mm_multiline.yaml` ships the feature.
- **2026-06-22**: Continuous-tape (`single` with `width: {min,max}`) labels are now auto-length (M11;
  ADR-0026; #77): the label fits its single-line content clamped to `[min,max]` (largest font that fits,
  then ellipsis), instead of always rendering at `max`. A dynamic-width single must declare both bounds.
- **2026-06-21**: Corrected the bundled Brother tape templates (M11; #66/#67). Replaced the three
  300-dpi templates with a five-template set at 180 dpi using the real TZe printable heights
  (9.9/15.8/18.1mm): `brother_12mm`/`brother_18mm`/`brother_24mm` (text only) and
  `brother_18mm_qr`/`brother_24mm_qr` (QR left + padded text). Established the render -> inspect -> fix
  loop as the definition of done for template work.
- **2026-06-21**: Loosened auth for homelab (M10; ADR-0025; #54). Dropped the 8-char password minimum
  (non-empty only). Added optional `LABELER_NO_AUTH=true`: the auth subsystem is off (open data routes,
  credential management returns `403`, relaxed origin check), with `noAuth: true` on `/auth/me`. Auth
  stays the default.
- **2026-06-19**: Typed app settings (M10; ADR-0024; #53). `GET/PUT/DELETE /api/settings` expose
  resolved application config, stored separately from `variables` and never interpolated. First
  setting: `job_log_retention_days` (default 90; 0 disables). The daily job-log prune reads the live
  value; the `LABELER_JOB_LOG_RETENTION_DAYS` env var is removed.
- **2026-06-19**: `DELETE /api/users/{id}` now rejects deleting your own account `409` (M10; #72);
  it would cascade the caller's own session and silently log them out. The Users UI disables the
  current user's delete control. Completed the `settings` to `variables` UI rename (M10; #71).
- **2026-06-18**: Selected-row preview in Import and Connect (M9; #64). The CSV Import and Homebox
  Connect pages display an on-demand preview for the selected grid row, rendered via `POST /render/label`
  (single templates) or `POST /batch` (sheet templates). Preview failures are shown inline and never
  gate Download or Print.
- **2026-06-18**: Label thumbnails (M9; ADR-0023; #73). `GET /templates/{id}/thumbnail` renders a
  single-label PNG preview using placeholder data, with content-hash ETag for cheap 304 revalidation.
  Sheets render one slot, not a full sheet. Variables resolve from the store; default option selection
  applied automatically.
- **2026-06-18**: Import & Print UX (M8; ADR-0022; #55 #56 #65 #57 #32). The CSV Import editor keeps a
  loaded CSV across template switches and can load a CSV before any template is chosen; every declared
  option is an always-present per-row column defaulting to its first allowed value, with a per-option
  "Apply to all rows" control (single-valued options read-only). Switching templates on the Print page no
  longer clears entered fields. Print/Download actions sit in a sticky bar so they stay reachable below a
  long grid (Import + Connect). `POST /import/csv` accepts `option.<name>` columns (defaulting missing
  declared options), reaching parity with the UI's per-row options.
- **2026-06-18**: Homebox & Connect hardening (M7; ADR-0021; #60 #61 #62 #58 #59). The Homebox connector
  lists items (`/v1/entities?isLocation=false`) and locations (`isLocation=true`) as two flat resources
  (was a combined list + a `/entities/tree` locations view), with populated `description`/`itemCount`
  columns. Browse rows carry a `url` (the Homebox page) and the Connect table renders the name as a link.
  The Connect page header is Connection-only; template + field mapping moved above the browser. The
  cross-view selection is now persistent and reviewable (visible/hidden split, removable list grouped by
  resource, 200-row cap), so a bulk add never includes unseen rows.
- **2026-06-18**: Renamed the key/value settings store to "variables" (ADR-0020, #52). API is now
  `GET /api/variables` + `PUT /api/variables/{key}`, interpolation is `{vars.X}` (was `{settings.X}`),
  and the UI section is "Variables". This frees "settings" for typed application config (#53). No
  behavior change; nothing was released under the old names.
- **2026-06-17**: Job-log retention (#29). The append-only `jobs` table is now pruned by age:
  `job_log_retention_days` (default 90, `0` disables) bounds history, enforced by a startup
  prune plus a daily background task; a `ts` index was added. No API change.
- **2026-06-17**: CI and image publishing (ADR-0019, #37). CI now also builds/tests the UI and builds +
  smoke-tests the Docker image; images publish to `ghcr.io/pfa230/labeler` (`:edge` + `:sha-` on `main`,
  `:X.Y.Z`/`:X.Y`/`:latest` on a `vX.Y.Z` tag) via the built-in `GITHUB_TOKEN`. Base images are pinned to
  digests with Dependabot bumps. amd64 only (arm64 deferred, #36). No API change.
- **2026-06-17**: Homebox integration UI (#35). Settings > Connections manages connections (API key
  write-only: password field, redacted display, blank-on-edit keeps the stored key). New Connect page:
  pick a connection + template, browse the connector (generic schema-driven table/tree, typed filters,
  cursor pagination, direct drill-down), select and map fields, materialize into the label grid, and
  download/print a batch. See §12 "Using a connection (UI)".
- **2026-06-17**: API integration spine (ADR-0018). Adds connector-backed connections: `GET/POST
  /api/connections` and `GET/PUT/DELETE /api/connections/{id}` (credential stored as-is, never returned;
  responses expose only `has_credential`), plus `GET /api/connections/{id}/schema`,
  `POST /api/connections/{id}/browse` (opaque HMAC-signed, restart-ephemeral cursors), and
  `POST /api/connections/{id}/materialize` (rows to label data). The browse model has resources with
  tiered fields (cheap/hydrated/derived) and typed filters. Outbound HTTP goes through one hardened
  egress client (timeouts, 8 MiB cap, no redirects/proxy, IP allow-check that blocks
  loopback/link-local/multicast but permits private LAN, bearer redaction). Upstream failures map to
  `502`. First connector is Homebox over `/v1/entities`. See §12.
- **2026-06-16**: App authentication (ADR-0017, #33). Flat user accounts (no roles): every `/api` route
  now requires a session cookie or `Authorization: Bearer` token except `/api/health`,
  `/api/auth/login`, `/api/auth/setup`, `/api/auth/me`, `/api/openapi.json`, and `/api/docs`. Adds
  session cookies (opaque, hashed at rest, `SameSite=Lax` + Origin check on cookie state changes) for
  browsers and API tokens for machines, first-run `POST /api/auth/setup` plus optional
  `LABELER_INIT_USER`/`LABELER_INIT_PASSWORD` bootstrap, and `/api/auth/*`, `/api/users`, `/api/tokens`
  endpoints (see §11). Breaking for existing clients: requests must now authenticate; `scripts/*.sh`
  send a bearer token.
- **2026-06-16**: Packaging & deployment (M6): a multi-stage `Dockerfile` (distroless), `docker-compose.yml`
  with seeded+owned named volumes, `.env.sample`, and `docs/DEPLOY.md` (build, volumes/backups, CUPS/IPP).
  No API change (ADR-0016; #18, #25, #26, #9).
- **2026-06-16**: Web UI Settings & Printers screen (`/settings`): a key/value settings editor over
  `GET /api/settings` + `PUT /api/settings/{key}` (with `qr_base_url` suggested), and a printers CRUD
  table over `/api/printers` (ADR-0015, #23). No API change.
- **2026-06-16**: Web UI CSV Import screen (`/import`): parse a CSV client-side, review/edit rows and
  per-row options in an editable grid, then batch print or download via `POST /api/batch` (ADR-0014,
  #24). No API change; the screen does not use `/api/import/csv`.
- **2026-06-16**: Web UI Render & Print screen (`/print`): pick a template, fill the auto-generated
  field/option form, live preview, then print to a printer or download (ADR-0013, #20). No API change.
- **2026-06-16**: REST API moved under `/api` (ADR-0008, #15); the root is reserved for the web UI.
  Unknown `/api/*` paths return `404 NotFound` (JSON). Added `GET /api/templates/{id}/source` (raw YAML)
  for the UI's read-only source view. Swagger UI is at `/api/docs/`, the doc at `/api/openapi.json`.
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
