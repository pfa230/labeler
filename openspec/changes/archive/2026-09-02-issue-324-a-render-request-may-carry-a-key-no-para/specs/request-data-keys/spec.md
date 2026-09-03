## Purpose

Defines what a request's `data` map, and a CSV file's data columns, may name: every key SHALL name a
parameter the template declares, and a key naming none is refused rather than carried into the render
and read by nothing. It owns the two refusals that enforce this, the endpoints they apply to, and the
one endpoint that is deliberately outside them.

## ADDED Requirements

### Requirement: Every key a request sends names a parameter the template declares

A label's `data` map SHALL carry only keys that name a parameter the named template declares under
`params:`. A key naming none is **unrecognized**, and an unrecognized key SHALL be refused. The service
SHALL NOT render, print or produce anything from a label carrying one, and SHALL NOT carry it into the
render data to be read by nothing.

**What may be sent is the declarations, not the input list.** Any parameter the template declares is a
legal key, including one that no active layout item reads and that no input list for this label
reports. `when:` narrows what an operator must supply; it narrows nothing about what a caller may
carry, and a label whose `data` covers every declared parameter SHALL be accepted whichever branch it
selects. A key that is legal for one label of a request is legal for every label of it.

**Where the rule applies.** These four paths, and no others:

- `POST /api/render/label`
- `POST /api/batch`
- `POST /api/print`
- `POST /api/import/csv`, in the column form the requirement below states

`POST /api/templates/{id}/inputs` is deliberately outside it and SHALL keep ignoring an unrecognized
key (`template-inputs`). That is a boundary and not an exception: it renders and prints nothing, and it
is the endpoint a client asks which of the columns it holds the template can read.

**Every unrecognized key is named, in ascending order.** A `data` map is unordered, so a label carrying
several unrecognized keys has no single "offending key" and any iteration order would differ between
two runs of the same request. One failure SHALL be reported for such a label, and its `message` SHALL
name **every** unrecognized key that label carries, sorted ascending by Unicode code point, together
with the template id the caller sent. The `message` is prose and is not part of the contract; that it
names all of them, in that order, is.

The template **id** is what the message names, because it is what the caller sent. The display `name:`
SHALL NOT be used for this: no caller chose it and two templates may share it.

**`data_key_unknown` is a new entry in the error contract, and this requirement is its published
home.** The frozen `docs/SPEC.md` §10.1 is not edited and every row already there remains
authoritative; this requirement extends that registry by exactly one row, under `InvalidRequest`, a
code that already carries reasons.

| Code | Status | Reason | When |
| --- | --- | --- | --- |
| `InvalidRequest` | `400` | `data_key_unknown` | A label's `data` map carries one or more keys naming no parameter the template declares. |

How it reaches the caller follows the envelope each path already has, and this requirement adds no
third shape:

- On `POST /api/render/label` the response SHALL be `400` carrying it at the top level, as
  `error.code` `InvalidRequest` with `error.details.reason` `data_key_unknown`.
- On `POST /api/batch` and `POST /api/print` the response SHALL be `422 BatchInvalid` at the top level,
  and the failure SHALL appear as an entry of `details.failures` carrying that label's `index`, the
  code `InvalidRequest` and the reason `data_key_unknown`, alongside an entry for every other label
  that fails for any reason (`batch-validation`). A batch entry carrying the `InvalidRequest` code is
  the shape an unparseable `datetime` value already uses (`datetime-params`).

**The check is per label and it is not a whole-request preflight.** Every label of a request SHALL be
checked, and every label carrying an unrecognized key SHALL appear in `details.failures`. A request
SHALL NOT stop at the first offending label, because doing so would report one failing label where the
frozen guarantee is that all of them are listed.

**A refusal that judges the request keeps what it reports.** Every check the service applies to the
request as a whole, before any label is validated, SHALL keep reporting what it reports today. In
particular, `POST /api/render/label` validates its `format`, `color_mode` and `resolution` query
parameters before it reaches the label, so a request carrying both an unknown `format` and an
unrecognized data key SHALL report `format_unknown`. The same holds for admission on the batch paths —
the label cap, an empty batch, an out-of-range `start_slot`, an unknown template id — and for the CSV
file's own refusals listed in the requirement below.

**Within one label, this check wins, and it can replace a failure the label reports today.** The claim
above is about request-level checks and is not a claim that no reason ever changes. A label is judged
in a fixed order (`batch-validation`) and the key check is first, so a label carrying an unrecognized
key SHALL report `data_key_unknown` **instead of** whatever it would otherwise have reported: an
omitted required parameter, an uncoercible value for a declared parameter, an unresolvable declared
default, or a render failure. That is deliberate. A stale key and a bad value are two defects in one
label, and naming the one the caller can see in its own request is more use than naming a consequence
of it. A label that carries no unrecognized key is unaffected and reports exactly what it reports
today.

#### Scenario: A single render refuses an unrecognized key

- **WHEN** `POST /api/render/label` is sent `{"template": "shelf", "data": {"title": "Bolts",
  "sku_legacy": "X-1"}}` for a template declaring `title` and not `sku_legacy`
- **THEN** the response is `400` with `error.code` `InvalidRequest` and `error.details.reason`
  `data_key_unknown`
- **AND** the `message` names `sku_legacy` and the template id `shelf`
- **AND** no image is produced

#### Scenario: Several unrecognized keys are all named, in ascending order

- **WHEN** that label carries `zeta`, `alpha` and `mid`, none of them declared
- **THEN** one failure is reported, not three, and its `message` names `alpha`, `mid` and `zeta` in that
  order

#### Scenario: A declared parameter no active item reads is legal

- **WHEN** a template declares `title` and `subtitle`, `subtitle` is read only inside a container gated
  on `orientation: vertical`, and a label selecting `orientation: horizontal` carries both
- **THEN** the render succeeds, because `subtitle` is declared, even though no input list for that label
  reports it and nothing on the label reads it

#### Scenario: A batch reports every offending label

- **WHEN** `POST /api/batch` is sent three labels, of which label 0 and label 2 each carry an
  unrecognized key
- **THEN** the response is `422 BatchInvalid`, `details.failures` holds an entry for index 0 and one for
  index 2, each carrying the code `InvalidRequest` and the reason `data_key_unknown`
- **AND** no ZIP, PDF or print job is produced

#### Scenario: A sheet template refuses it on the same terms

- **WHEN** the same batch is sent for a `sheet` template
- **THEN** the response is `422 BatchInvalid` with the same entries, and no PDF page is emitted

#### Scenario: A print request dispatches nothing

- **WHEN** `POST /api/print` is sent a `data` map carrying an unrecognized key
- **THEN** the response is `422 BatchInvalid` carrying that failure and no print job is dispatched

#### Scenario: An unrecognized key replaces the reason a label reports today

- **WHEN** a label carries an unrecognized key `sku_legacy` and also `copies_shown: "abc"` for a
  declared `integer` parameter, which alone would fail during resolution
- **THEN** the response reports `data_key_unknown`, not the resolution failure, and the same label
  without `sku_legacy` still reports the resolution failure unchanged

#### Scenario: A query-parameter rejection still wins

- **WHEN** `POST /api/render/label?format=svg` is sent a label carrying an unrecognized key
- **THEN** the response reports `format_unknown`, unchanged from today, and not `data_key_unknown`

#### Scenario: The inputs endpoint is not affected

- **WHEN** the same label is posted to `POST /api/templates/{id}/inputs`
- **THEN** the response is `200` and the unrecognized key is ignored (`template-inputs`)

### Requirement: A CSV data column names a declared parameter

`POST /api/import/csv` SHALL refuse a CSV whose header carries a data column naming no parameter the
template declares. The refusal SHALL be a **whole-request** `400 InvalidRequest` with
`error.details.reason` `csv_data_column_unknown`, raised **before any label is built from the file and
before anything is rendered**, in both `mode=download` and `mode=print`. No label is rendered, no blob
is produced and no print job is dispatched.

It is a whole-request failure rather than a per-label one because a column is a property of the file
and not of one row: every row of the file carries it, so reporting it once per row would report the
same defect as many times as the sheet is long.

**Every unrecognized column is named, in ascending order**, on the same terms the requirement above
states for a label's keys: one failure, naming all of them sorted ascending by Unicode code point,
together with the template id the request named.

A column named `option.<name>` is not a data column and is not judged by this rule. It is judged by the
`csv_option_column_unknown` rule the service already applies (frozen `docs/SPEC.md` §10.1), which is
unchanged.

**The order of the file's own refusals is fixed, and this rule joins the end of it.** In order:

1. an unparsable header, or one with an empty or duplicate column name — `csv_header_invalid`;
2. an unparsable data row — `csv_row_invalid`;
3. a header with no data rows under it — `csv_empty`;
4. an `option.<name>` column naming no parameter the template declares — `csv_option_column_unknown`;
5. a data column naming no parameter the template declares — `csv_data_column_unknown`.

Steps 1 to 4 are unchanged and every file that fails one of them today SHALL keep reporting what it
reports today. A file breaking both column rules therefore reports `csv_option_column_unknown`, and a
file with an unparsable row and an unrecognized column reports `csv_row_invalid`.

**The whole file is parsed before any column is judged, and that is deliberate.** This requirement does
not claim the refusal precedes parsing: a column set read from a header the parser has not accepted is
not a column set anyone can act on, and hoisting step 5 above steps 1 to 3 would re-label files that
fail today. What it claims is what a caller can observe — that no **label** is built from the file, no
render is attempted, and the whole request fails with one status. A `ParsedCsvRow` is an intermediate
of parsing the file, not a label: no label exists until the option columns have been folded in and the
rows have been turned into label inputs, and that happens after step 5.

**`csv_data_column_unknown` is a new entry in the error contract, and this requirement is its published
home.** The frozen `docs/SPEC.md` §10.1 is not edited and every row already there remains authoritative;
this requirement extends that registry by exactly one row, under `InvalidRequest`.

| Code | Status | Reason | When |
| --- | --- | --- | --- |
| `InvalidRequest` | `400` | `csv_data_column_unknown` | A CSV data column names no parameter the template declares. |

#### Scenario: An unrecognized data column is refused

- **WHEN** `POST /api/import/csv?template=shelf` is sent a sheet whose header is `title,sku_legacy` for a
  template declaring `title` and not `sku_legacy`
- **THEN** the response is `400` with `error.code` `InvalidRequest` and `error.details.reason`
  `csv_data_column_unknown`
- **AND** the `message` names `sku_legacy` and the template id `shelf`
- **AND** no ZIP or PDF is produced

#### Scenario: Print mode refuses it too

- **WHEN** the same file is sent with `mode=print` and a valid printer
- **THEN** the response is the same `400` and no print job is dispatched

#### Scenario: Every unrecognized column is named once

- **WHEN** the header is `title,zeta,alpha` and neither `zeta` nor `alpha` is declared
- **THEN** one failure is reported whose `message` names `alpha` then `zeta`, regardless of how many
  data rows the file holds

#### Scenario: An unparsable row is reported ahead of an unrecognized column

- **WHEN** a file carries an unrecognized data column and also a row the CSV parser rejects
- **THEN** the response reports `csv_row_invalid`, unchanged from today

#### Scenario: A header with no rows is reported ahead of an unrecognized column

- **WHEN** a file carries an unrecognized data column and no data rows
- **THEN** the response reports `csv_empty`, unchanged from today

#### Scenario: An option column is judged by its own rule

- **WHEN** the header is `title,sku_legacy,option.size` where neither `sku_legacy` nor `size` is declared
- **THEN** the response reports `csv_option_column_unknown`, unchanged from today

#### Scenario: A file naming only declared parameters imports

- **WHEN** the header names only declared parameters, alongside `option.` columns naming declared ones
- **THEN** the file imports exactly as it does today
