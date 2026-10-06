# Rendering

## Purpose

Covers the endpoints that turn a template and data into output (`/render/label`, `/batch`, `/print`, `/import/csv`): output formats, sheet pagination, all-or-nothing validation and the print summary, plus the HTTP surface they sit on.

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

`png` SHALL return `image/png` rasterized at the effective DPI; `pdf` SHALL return vector `application/pdf`. `bilevel` SHALL make every pixel opaque black (luminance below one half) or white, without dithering. An unknown template SHALL be `404 TemplateNotFound`, judged before the query; a `sheet` template SHALL be `422 UnsupportedFormat`. A failing label SHALL return its own error, not `BatchInvalid`.

#### Scenario: Out-of-range resolution with PDF

- **WHEN** a client posts with `?format=pdf&resolution=0`
- **THEN** the response is `400 InvalidRequest` with reason `resolution_invalid`

#### Scenario: Sheet template

- **WHEN** a client posts a `sheet` template
- **THEN** the response is `422 UnsupportedFormat`

### Requirement: Batch request

`POST /batch` SHALL render or print labels for one template. The body:

| Field | Type | Required | Rules |
|---|---|---|---|
| `template` | string | yes | Unknown is `404 TemplateNotFound`. |
| `mode` | `download`, `print` | yes | Other values are `400` `mode_unknown`. |
| `labels` | array of `{ "data": object }` | yes | Each entry holds exactly `data`. Over 500 is `413 BatchTooLarge`; none is `400` `batch_empty`. |
| `printer` | string | for `print` | Missing is `400` `printer_required`; unknown is `404 PrinterNotFound`. |
| `format` | `png`, `pdf` | no | `single` download only, default `png`, unknown is `400` `format_unknown`; ignored for `sheet` downloads; with `print` it is `400` `format_not_applicable`. |
| `start_slot` | integer ≥ 0 | no | Default `0`. Non-zero on `single` is `400` `start_slot_not_applicable`; at or past the sheet's slot count is `400` `start_slot_out_of_range`. |

Output by format and mode:

| | `download` | `print` |
|---|---|---|
| `single` | `<template id>.zip`, one file per label in `format` | one job per label |
| `sheet` | one paginated `<template id>.pdf` | that PDF as one job |

Downloads SHALL carry `Content-Disposition: attachment`. ZIP entries SHALL be named by 1-based label index zero-padded to the digit count of the label total, plus the extension.

#### Scenario: Single download

- **WHEN** a `single` batch of 10 labels is downloaded with `format: pdf`
- **THEN** the response is `<id>.zip` holding `01.pdf` to `10.pdf`

### Requirement: Sheet pagination

A `sheet` batch SHALL fill the sheet's `positions` in declared order with labels in request order, starting at `start_slot` on page 1 and at slot 0 on each later page, adding pages as needed. Each label SHALL be clipped to a box of the sheet's label size.

#### Scenario: Batch overflows the first page

- **WHEN** 5 labels go to a 4-slot sheet with `start_slot: 2`
- **THEN** the PDF has 2 pages: labels 0 and 1 in slots 2 and 3, labels 2 to 4 in slots 0 to 2 of page 2

### Requirement: All-or-nothing validation

`/batch`, `/print` and `/import/csv` SHALL judge every label before executing anything. A label is judged in this order, and its first failure is its only one: its `data` keys name declared parameters, its parameters resolve (both `parameters`), then it measures and renders. If any label fails, the request SHALL return `422 BatchInvalid` whose `details.failures` (shape: `errors`) holds one entry per failing label in ascending `index`, and SHALL produce no file, page or print job. Request-level refusals (unknown template or printer, label cap, empty batch, `mode`, `format`, `start_slot`, CSV file refusals) SHALL keep their own status and be decided before any label.

#### Scenario: Two labels failing different ways

- **WHEN** label 0 carries an undeclared key and label 1 omits a required parameter an active item reads
- **THEN** the response is `422 BatchInvalid` with index 0 reason `data_key_unknown` and index 1 code `MissingField`, and no ZIP

#### Scenario: A label failing two ways

- **WHEN** one label both carries an undeclared key and omits a required parameter
- **THEN** its one failure entry carries `data_key_unknown`

#### Scenario: Oversized batch with a bad label

- **WHEN** 501 labels include one with an undeclared key
- **THEN** the response is `413 BatchTooLarge`

### Requirement: Print summary

A print SHALL render with the printer's effective render profile after its media preflight (both `printing`), then send every job. A send failure SHALL NOT be fatal: the response SHALL be `200` with `{ "total", "succeeded", "failed": [{ "index", "error" }], "jobs" }`, where `total` counts labels, `jobs` counts jobs dispatched, `failed` lists every label index of each job that failed to send, and `succeeded` is `total` minus that count. Each job SHALL be recorded in the print-job log with its outcome.

#### Scenario: One tape job fails

- **WHEN** a 3-label `single` print has the job for label 1 fail to send
- **THEN** the response is `200` with `total` 3, `succeeded` 2, `jobs` 3 and `failed` `[{ "index": 1, … }]`

### Requirement: Print webhook

`POST /print` SHALL print one label `copies` times. The body SHALL hold exactly `template` (unknown: `404 TemplateNotFound`), `printer` (unknown: `404 PrinterNotFound`), `data` (object, required; `{}` is passed as an empty map) and `copies` (integer `1..100`, default `1`, else `400` `copies_invalid`). A body that does not deserialize, including a missing `data` or any other key, SHALL be `400` `json_malformed` (`errors`) before `copies` is checked; one over 64 KiB SHALL be `413 PayloadTooLarge`. `copies: N` SHALL become N identical labels at indices `0..N-1` printed as a `/batch` print with `start_slot` 0: N jobs for `single`, one N-slot paginated job for `sheet`. The response is the print summary. The OpenAPI `PrintRequest` SHALL require `data` and set `additionalProperties: false`.

#### Scenario: Copies of a failing map

- **WHEN** `copies: 3` is posted with a `data` map carrying an undeclared key
- **THEN** the response is `422 BatchInvalid` with failures at indices 0, 1 and 2, and no job

#### Scenario: Missing data with bad copies

- **WHEN** a body omits `data` and carries `"copies": 0`
- **THEN** the response is `400` with reason `json_malformed`, not `copies_invalid`

#### Scenario: Unknown key

- **WHEN** a body carries `fields` or any other unlisted key
- **THEN** the response is `400 InvalidRequest` `json_malformed` naming the key, and nothing prints

### Requirement: Print form body

The UI's print client SHALL type the `/print` body as exactly `template`, `printer`, `data` (required) and `copies`. The print screen SHALL send a `single` template's entered values as `data`, `{}` when nothing was entered, and SHALL print a `sheet` template through `POST /batch` with `mode: print`.

#### Scenario: Template needing no input

- **WHEN** an operator prints a `single` template that reports no inputs
- **THEN** one request goes to `POST /api/print` with `data: {}`

### Requirement: CSV import

`POST /import/csv?template=<id>&mode=&printer=&format=` SHALL render one label per CSV row through the `/batch` rules, `mode` defaulting to `download` and `start_slot` always 0. The body is raw CSV: a leading UTF-8 BOM is stripped, quoted fields work, headers and values are trimmed, the header names parameters (unknown columns: `csv_data_column_unknown`, see `parameters`) and each row is one label of string values. File refusals SHALL be `400 InvalidRequest`, in both modes, before any label is judged: `csv_header_invalid` (unparsable header, or an empty or duplicate column name), `csv_row_invalid` (unparsable row, including a field count differing from the header), `csv_empty` (no data rows).

#### Scenario: Download a tape CSV

- **WHEN** `message,code\nHello,QR-1\nWorld,QR-2\n` is posted for a `single` template
- **THEN** the response is a ZIP holding `1.png` and `2.png`

#### Scenario: File refusal precedes the label cap

- **WHEN** a CSV exceeds 500 rows and carries an undeclared column
- **THEN** the response is `400 InvalidRequest` with reason `csv_data_column_unknown`
