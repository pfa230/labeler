# Labeler — Specification

**Status:** Living document. Update this file on every major decision or behavior change, and record
the decision as an ADR under [`docs/adr/`](adr/).

**Version:** 0.1.0

## 1. Overview

Labeler is a stateless REST service that renders labels from declarative YAML templates. It supports
two output modes:

- **Single label → PNG** (`POST /render/label`), for continuous-roll label printers.
- **Sheet of labels → PDF** (`POST /render/batch`), for pre-cut label sheets on standard paper.

Templates are loaded once at startup and held immutably. Rendering works by generating
[Typst](https://typst.app/) source on the fly and compiling it in-process via `typst-as-lib`
(`typst-render` for PNG, `typst-pdf` for PDF).

## 2. HTTP API

| Method | Path | Purpose | Success |
| --- | --- | --- | --- |
| GET | `/health` | Liveness check | `200 {"status":"ok"}` |
| GET | `/templates` | List template summaries (sorted by id) | `200 {"templates":[…]}` |
| GET | `/templates/{id}` | Full template detail incl. layout | `200` / `404` |
| POST | `/render/label` | Render one label (`?format=png\|pdf`) | `200 image/png` or `application/pdf` |
| POST | `/render/batch` | Render a label sheet | `200 application/pdf` |
| GET | `/openapi.json` | OpenAPI 3 document | `200` |
| GET | `/docs` | Swagger UI | `200` |

The server binds `0.0.0.0:$PORT` (default `8080`).

### 2.1 `POST /render/label`

```json
{
  "template": "brother12mm",
  "data": { "message": "Hello", "code": "QR-123" },
  "option": { "variant": "default" }
}
```

- `template` must reference a template whose `format.type` is `single`; otherwise `422 UnsupportedFormat`.
- `data` binds field names referenced by `text`/`qr` layout items.
- `option` is optional and validated against the template's declared `options`.
- `?format=png|pdf` (default `png`) selects the output: `image/png` (rasterized at the template DPI) or
  `application/pdf` (vector). An unknown value is `400 InvalidRequest`.

### 2.2 `POST /render/batch`

```json
{
  "template": "avery5163",
  "start_slot": 0,
  "labels": [
    { "option": { "orientation": "horizontal", "outline": "yes" },
      "data": { "id": "A1", "url": "https://example.com/A1", "name": "…", "tags": "…", "description": "…" } }
  ]
}
```

- `template` must be `format.type: sheet`; otherwise `422 UnsupportedFormat`.
- `start_slot` (default `0`) is the zero-based index into the template's `positions`. Each label fills
  the next slot. `start_slot + labels.len()` must not exceed the number of positions.

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

`auto` size resolves to `max_w`/`max_h` if present; for `container` and `line` it falls back to the
parent frame's dimension. A non-`auto` numeric size must be > 0 (lines may be 0 on one axis).

### 4.1 Item types

- **`text`** — `name` (data key), placement, `font_size`, `multiline` (default `false`),
  `alignment` (`horizontal`: left/center/right, `vertical`: top/center/bottom).
  `font_size` is either a fixed number or a range `{ min, max }`. A range auto-shrinks the text to fit
  the box (0.5pt steps, `fontdue` metrics) and truncates with an ellipsis if it still overflows.
  Single-line text collapses spaces to non-breaking and renders only the first line.
- **`qr`** — `name` (data key), placement, optional `params`:
  `error_correction` (`L`/`M`/`Q`/`H`, default `M`), `module_size`, `quiet_zone`.
  Rendered as an SVG via the `qrcode` crate, embedded as a Typst image.
- **`image`** — exactly one of `src` (a path to a bundled asset, resolved under the assets root with a
  traversal guard) or `name` (a data key whose value is a base64 data URI, `data:<mime>;base64,...`),
  plus placement and optional `fit` (`contain` default, `cover`, `stretch`). Formats: PNG, JPEG, SVG.
  Bytes are decoded server-side and injected into Typst as a virtual file; there is no server-side URL
  fetching (see ADR-0009). The assets root is `LABELER_ASSETS_DIR` (default `assets/`). Missing data
  key → `MissingField`; bad base64 / unsupported format / asset path problems → `UnsupportedLayoutItem`.
- **`line`** — placement where `size` is the delta `[dx, dy]` from `at`, plus `thickness` (> 0).
- **`container`** — a recursive group. Fields: placement (size defaults to `auto`/`auto` = fill parent),
  optional `option` gate (§5), optional `frame` (`thickness` > 0, `rounded` bool), `padding`, and
  `items` (nested layout). Children are positioned relative to the container's padded inner box.
  `padding` is either a single number (uniform) or `[top, right, bottom, left]`; values must be ≥ 0;
  default `0`.

Layout item `name`s (text/qr, and a data-bound `image`) must be unique and non-empty within a sibling
list.

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

`text` and `qr` items resolve their `name` against the request `data` map. A missing key is
`422 MissingField`. JSON scalars are stringified (`value_to_string`): strings as-is, numbers/bools via
their textual form, `null` as empty, other values via JSON.

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
| `RenderFailed` | 500 | Typst compile/encode failure. |

`code` strings are part of the contract — keep them stable.

## Changelog

- **Unreleased** — `POST /render/label` gained `?format=png|pdf` (single-label PDF output). Issue #4.
- **Unreleased** — Added the `image` layout item (static asset under the assets root, and data-bound
  base64 data URI; PNG/JPEG/SVG; injected into Typst as virtual files). See ADR-0009. Issue #3.
- **0.1.0** — Initial spec captured from the implemented service (single PNG + sheet PDF rendering,
  recursive containers, options gating, two-stage parsing). See ADRs 0001–0005.
