# Rendering

## Purpose

Covers the endpoints that turn a template and data into output (`/render/label`, `/render`, `/print`): output formats, sheet pagination, all-or-nothing validation and the print summary, plus the HTTP surface they sit on.

## Requirements

### Requirement: HTTP surface

The API SHALL be served under `/api` on `0.0.0.0:$PORT` (default `8080`); paths below are relative to `/api`. An unknown `/api/*` path SHALL return `404 NotFound` in the JSON error envelope. `GET /health` SHALL return `200 {"status":"ok"}`, `GET /openapi.json` the OpenAPI 3 document, and `/docs/` the Swagger UI. Outside `/api`, `/assets/*` SHALL serve files from `LABELER_UI_DIR` (default `ui/dist`), a missing one being `404`, and any other path SHALL return its `index.html`, or `404` "UI not built" when absent.

#### Scenario: Unknown API path

- **WHEN** a client requests `GET /api/nope`
- **THEN** the response is `404` with `error.code` `NotFound`, not HTML

### Requirement: Single-label render

`POST /render/label` SHALL render one label of a `single` template and return the raw bytes. The body SHALL hold exactly `template` and `data` (object), both required. Query parameters, where an empty value means the default and each refusal is `400 InvalidRequest`:

| Parameter | Values | Default | Refusal reason |
|---|---|---|---|
| `format` | `png`, `pdf` | `png` | `format_unknown` |
| `color_mode` | `color`, `bilevel` | `color` | `color_mode_unknown`; `bilevel` with `pdf` is `bilevel_requires_png` |
| `resolution` | integer `1..1200` | template `dpi` | `resolution_invalid`, checked even for `pdf` |

`png` SHALL return `image/png` rasterized at the effective DPI; `pdf` SHALL return vector `application/pdf`. `bilevel` SHALL make every pixel opaque black (luminance below one half) or white, without dithering. An unknown template SHALL be `404 NotFound`; a `sheet` template SHALL be `400 InvalidRequest` with reason `format_unsupported`. A failing label SHALL return its own error, not `BatchInvalid`.

#### Scenario: Out-of-range resolution with PDF

- **WHEN** a client posts with `?format=pdf&resolution=0`
- **THEN** the response is `400 InvalidRequest` with reason `resolution_invalid`

#### Scenario: Sheet template

- **WHEN** a client posts a `sheet` template
- **THEN** the response is `400 InvalidRequest` with reason `format_unsupported`

### Requirement: Batch request

`POST /render` SHALL render labels for one template to a file, and `POST /print` SHALL send them to a printer. Both take this body:

| Field | Type | Required | Rules |
|---|---|---|---|
| `template` | string | yes | Unknown is `404 NotFound`. |
| `labels` | array of `{ "data": object, "copies"?: integer }` | yes | Each entry holds `data` and optionally `copies`. Over 500 labels is `413 PayloadTooLarge`; none is `400` `batch_empty`. |
| `start_slot` | integer ≥ 0 | no | `sheet` only, default `0`; at or past the sheet's slot count is `400` `start_slot_out_of_range`. |
| `format` | `png`, `pdf` | no | `/render` of a `single` template only, default `png`; unknown is `400` `format_unknown`. |
| `printer` | string | `/print` only, required | Unknown is `404 NotFound`. |
| `chain_and_continue` | boolean | `/print` only, optional | Default false; job finishing is defined by `printing`. |

An entry's `copies` SHALL default to `1`; a value that is not an integer of at least `1`, or `null`, is `400 InvalidRequest` with reason `json_malformed`. An entry with `copies: N` SHALL stand for N labels with its `data`, in its place in the request. Labels SHALL be counted and indexed from 0 in that order, so the label cap, ZIP entries, sheet slots, failure indices and the print summary all count these labels.

A field sent where its row does not apply (`start_slot` for a `single` template, even `0`; `format` for a `sheet` template) SHALL be `400 InvalidRequest` with reason `field_not_applicable`, whatever its value. `format` on `/print`, and `printer` or `chain_and_continue` on `/render` are unlisted keys (`errors`).

Output by format and endpoint:

| | `/render` | `/print` |
|---|---|---|
| `single` | `<template id>.zip`, one file per label in `format` | one job per entry, carrying its copies (`printing`) |
| `sheet` | one paginated `<template id>.pdf` | that PDF as one job |

`/print` SHALL render each entry of a `single` template once, whatever its `copies`. `/render` SHALL answer with the file and `Content-Disposition: attachment`; `/print` SHALL answer with the print summary. ZIP entries SHALL be named by 1-based label index zero-padded to the digit count of the label total, plus the extension.

#### Scenario: Single download

- **WHEN** a `single` batch of 10 labels is sent to `/render` with `format: pdf`
- **THEN** the response is `<id>.zip` holding `01.pdf` to `10.pdf`

#### Scenario: Copies count as labels

- **WHEN** a `single` batch holds entry A with `copies: 2` and entry B without `copies`, sent to `/render`
- **THEN** the ZIP holds `1.png` and `2.png` rendered from A and `3.png` rendered from B

#### Scenario: Copies fill sheet slots

- **WHEN** a `sheet` of 24 slots holds entry A with `copies: 3` and entry B, with `start_slot: 22`
- **THEN** A fills slots 22 and 23 of page 1 and slot 0 of page 2, and B fills slot 1 of page 2

#### Scenario: Copies count toward the cap

- **WHEN** a batch holds two entries with `copies: 300` each
- **THEN** the response is `413 PayloadTooLarge`
- **AND** two entries with `copies: 250` each are accepted

#### Scenario: Zero copies

- **WHEN** an entry carries `copies: 0`
- **THEN** the response is `400 InvalidRequest` with reason `json_malformed`

#### Scenario: A failing entry fails each of its labels

- **WHEN** entry 0 carries `copies: 2` and an undeclared key
- **THEN** the response is `422 BatchInvalid` with failures at index 0 and index 1, both reason `data_key_unknown`

#### Scenario: A zero start slot on a single template

- **WHEN** a `single` batch carries `start_slot: 0`
- **THEN** the response is `400 InvalidRequest` with reason `field_not_applicable`

#### Scenario: Continuation is print-only

- **WHEN** a client sends `chain_and_continue` to `/render`
- **THEN** the response is `400 InvalidRequest` with reason `json_malformed`

#### Scenario: Malformed continuation value

- **WHEN** a print request carries a non-boolean or null `chain_and_continue` value
- **THEN** the response is `400 InvalidRequest` with reason `json_malformed`

### Requirement: Sheet pagination

A `sheet` batch SHALL fill the sheet's `positions` in declared order with labels in request order, starting at `start_slot` on page 1 and at slot 0 on each later page, adding pages as needed.

#### Scenario: Batch overflows the first page

- **WHEN** 5 labels go to a 4-slot sheet with `start_slot: 2`
- **THEN** the PDF has 2 pages: labels 0 and 1 in slots 2 and 3, labels 2 to 4 in slots 0 to 2 of page 2

### Requirement: All-or-nothing validation

`/render` and `/print` SHALL judge every label before executing anything: its `data` keys name declared parameters, its parameters resolve (both `parameters`), and it measures and renders. If any label fails, the request SHALL return `422 BatchInvalid` whose `details.failures` (shape: `errors`) holds one entry per failing label, and SHALL produce no file, page or print job. Request-level refusals (unknown template or printer, label cap, empty batch, `format`, `start_slot`, unsupported continuation (`printing`), a template whose snapshot does not resolve: `interpolation`) SHALL keep their own status and are decided before any label (`errors`).

#### Scenario: Two labels failing different ways

- **WHEN** label 0 carries an undeclared key and label 1 sends `abc` for an `integer` parameter
- **THEN** the response is `422 BatchInvalid` with index 0 reason `data_key_unknown` and index 1 reason `param_value_invalid`, and no ZIP

#### Scenario: Unsupported continuation outranks a label fault

- **WHEN** a print request sets `chain_and_continue: true` without printer support and a label has invalid data
- **THEN** the request reports the continuation refusal defined by `printing` and dispatches no jobs

### Requirement: Print summary

`/print` SHALL render with the printer's effective render profile (`printing`), then send every job. A send failure SHALL NOT be fatal: the response SHALL be `200` with `{ "total", "sent", "failed": [{ "index", "error" }], "jobs" }`, where `total` counts labels, `jobs` counts jobs dispatched, `failed` lists every label index of each job the printer refused or that could not reach it, and `sent` is `total` minus that count. `sent` means the printer accepted the job; what it prints is the printer's outcome.

#### Scenario: One tape job fails

- **WHEN** a `single` print of three entries without `copies` has the job for label 1 refused by the printer
- **THEN** the response is `200` with `total` 3, `sent` 2, `jobs` 3 and `failed` `[{ "index": 1, … }]`

#### Scenario: Three copies in one accepted job

- **WHEN** a `single` print holds one entry with `copies: 3` and the printer accepts its job
- **THEN** the response is `200` with `total` 3, `sent` 3, `jobs` 1 and empty `failed`

#### Scenario: A refused copies job fails all its labels

- **WHEN** a `single` print holds entry A without `copies` and entry B with `copies: 3`, and the printer refuses B's job
- **THEN** the response is `200` with `total` 4, `sent` 1, `jobs` 2 and `failed` at indices 1, 2 and 3

### Requirement: Sheet stock clipping

Every rendered sheet slot, including a thumbnail (`templates`), SHALL clip all ink to its stock outline.

#### Scenario: Decoration reaches a curved edge

- **WHEN** a full-slot colored background renders on an ellipse sheet
- **THEN** its central ink remains and its bounding-box corners are absent in both the one-slot PNG and each PDF slot

#### Scenario: Rounded rectangle clips only its curved corners

- **WHEN** a 60-by-40 rectangular sheet slot with `rounded: 5` renders a full-slot background
- **THEN** the corner at `[0, 0]` is clipped while the edge midpoint at `[0, 20]` and the center remain, in PNG and PDF
