## MODIFIED Requirements

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


### Requirement: All-or-nothing validation

`/render` and `/print` SHALL judge every label before executing anything: its `data` keys name declared parameters, its parameters resolve (both `parameters`), and it measures and renders. If any label fails, the request SHALL return `422 BatchInvalid` whose `details.failures` (shape: `errors`) holds one entry per failing label, and SHALL produce no file, page or print job. Request-level refusals (unknown template or printer, label cap, empty batch, `format`, `start_slot`, unsupported continuation (`printing`), a template whose snapshot does not resolve: `interpolation`) SHALL keep their own status and are decided before any label (`errors`).

#### Scenario: Two labels failing different ways

- **WHEN** label 0 carries an undeclared key and label 1 sends `abc` for an `integer` parameter
- **THEN** the response is `422 BatchInvalid` with index 0 reason `data_key_unknown` and index 1 reason `param_value_invalid`, and no ZIP

#### Scenario: Unsupported continuation outranks a label fault

- **WHEN** a print request sets `chain_and_continue: true` without printer support and a label has invalid data
- **THEN** the request reports the continuation refusal defined by `printing` and dispatches no jobs
