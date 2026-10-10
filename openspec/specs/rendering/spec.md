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
| `labels` | array of `{ "data": object }` | yes | Each entry holds exactly `data`. Over 500 is `413 PayloadTooLarge`; none is `400` `batch_empty`. |
| `start_slot` | integer ≥ 0 | no | `sheet` only, default `0`; at or past the sheet's slot count is `400` `start_slot_out_of_range`. |
| `format` | `png`, `pdf` | no | `/render` of a `single` template only, default `png`; unknown is `400` `format_unknown`. |
| `printer` | string | `/print` only, required | Unknown is `404 NotFound`. |

A field sent where its row does not apply (`start_slot` for a `single` template, even `0`; `format` for a `sheet` template) SHALL be `400 InvalidRequest` with reason `field_not_applicable`, whatever its value. `format` on `/print` and `printer` on `/render` are unlisted keys (`errors`).

Output by format and endpoint:

| | `/render` | `/print` |
|---|---|---|
| `single` | `<template id>.zip`, one file per label in `format` | one job per label |
| `sheet` | one paginated `<template id>.pdf` | that PDF as one job |

`/render` SHALL answer with the file and `Content-Disposition: attachment`; `/print` SHALL answer with the print summary. ZIP entries SHALL be named by 1-based label index zero-padded to the digit count of the label total, plus the extension.

#### Scenario: Single download

- **WHEN** a `single` batch of 10 labels is sent to `/render` with `format: pdf`
- **THEN** the response is `<id>.zip` holding `01.pdf` to `10.pdf`

#### Scenario: A zero start slot on a single template

- **WHEN** a `single` batch carries `start_slot: 0`
- **THEN** the response is `400 InvalidRequest` with reason `field_not_applicable`

### Requirement: Sheet pagination

A `sheet` batch SHALL fill the sheet's `positions` in declared order with labels in request order, starting at `start_slot` on page 1 and at slot 0 on each later page, adding pages as needed. Each label SHALL be clipped to a box of the sheet's label size.

#### Scenario: Batch overflows the first page

- **WHEN** 5 labels go to a 4-slot sheet with `start_slot: 2`
- **THEN** the PDF has 2 pages: labels 0 and 1 in slots 2 and 3, labels 2 to 4 in slots 0 to 2 of page 2

### Requirement: All-or-nothing validation

`/render` and `/print` SHALL judge every label before executing anything: its `data` keys name declared parameters, its parameters resolve (both `parameters`), and it measures and renders. If any label fails, the request SHALL return `422 BatchInvalid` whose `details.failures` (shape: `errors`) holds one entry per failing label, and SHALL produce no file, page or print job. Request-level refusals (unknown template or printer, label cap, empty batch, `format`, `start_slot`, a template whose snapshot does not resolve: `interpolation`) SHALL keep their own status and are decided before any label (`errors`).

#### Scenario: Two labels failing different ways

- **WHEN** label 0 carries an undeclared key and label 1 sends `abc` for an `integer` parameter
- **THEN** the response is `422 BatchInvalid` with index 0 reason `data_key_unknown` and index 1 reason `param_value_invalid`, and no ZIP

### Requirement: Print summary

`/print` SHALL render with the printer's effective render profile (`printing`), then send every job. A send failure SHALL NOT be fatal: the response SHALL be `200` with `{ "total", "sent", "failed": [{ "index", "error" }], "jobs" }`, where `total` counts labels, `jobs` counts jobs dispatched, `failed` lists every label index of each job the printer refused or that could not reach it, and `sent` is `total` minus that count. `sent` means the printer accepted the job; what it prints is the printer's outcome.

#### Scenario: One tape job fails

- **WHEN** a 3-label `single` print has the job for label 1 refused by the printer
- **THEN** the response is `200` with `total` 3, `sent` 2, `jobs` 3 and `failed` `[{ "index": 1, … }]`
