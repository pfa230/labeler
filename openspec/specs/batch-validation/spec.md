# batch-validation Specification

## Purpose
Defines what a request rendering many labels checks before it produces or prints anything: that every
label is judged, that a request with any failing label produces nothing, and that the response lists
every label that failed rather than the first one.

## Requirements

### Requirement: Every label is judged before any label is executed

`POST /api/batch`, `POST /api/print` and `POST /api/import/csv` SHALL judge **every** label of the
request before the request executes. Only once every label passes does the endpoint execute:
`download` streams the blob, and `print` dispatches jobs and returns a `200` summary.

**Judging a label is not only rendering it.** A label fails if any of the following fails, and each is
a way a label can fail that this requirement covers:

- its `data` keys, which SHALL name declared parameters (`request-data-keys`);
- resolving its parameters, including a declared `default:` that cannot be resolved
  (`param-resolution`);
- measuring and rendering it.

Those are judged **in that order**, and the order is observable: the key check runs before any
parameter is resolved and before anything is measured or rendered, so a label carrying both an
unrecognized key and an omitted required parameter reports `data_key_unknown`. A label that fails
SHALL NOT be rendered.

**Atomicity is about output, not about work.** The service MAY render a label that passes before it
reaches a later label that fails, and a request that fails SHALL discard whatever it rendered. Nothing
observable distinguishes that from rendering no label at all: the failing request returns the same
`422`, emits no bytes, dispatches no job and writes nothing. A rule that no valid label may be
rasterized before a later one is found to fail would constrain the order of internal work while
changing no response, and this requirement does not make one. What it requires is that a label that
fails is not rendered and that a request with any failing label produces nothing.

**Exactly one entry per failing label.** A label that would fail several ways contributes one entry,
carrying the first failure by the order above. A label that passes contributes none.

**The response.** If any label fails, the request SHALL return `422` with `error.code` `BatchInvalid`
and `error.details.failures`, an array of `{ index, code, reason?, message }` holding an entry for
**every** failing label, in ascending `index` order. `index` is the zero-based label index. `code` is
the error code that label's failure carries. `reason` SHALL be present exactly when that `code` is one
that carries a reason (frozen `docs/SPEC.md` §10.1) and SHALL be omitted otherwise. The request SHALL
be **atomic** in both modes and both formats: nothing is produced and nothing is printed. No ZIP, no
PDF, no page of a sheet, and no print job.

**`POST /api/print` counts copies as labels.** `copies: N` expands the one submitted `data` map into N
labels at indices `0` through `N-1`, and each is a label this requirement judges. A `data` map that
fails therefore contributes **N** entries, one per index, carrying the same `code`, `reason` and
`message`, and not one entry. That is what the endpoint does today for every kind of label failure,
because `copies` counts label instances rather than annotating one; this requirement restates it and
changes nothing about it.

Listing every failing label is the guarantee, and it is what forbids stopping at the first one or
answering any single class of failure ahead of the rest of the labels. A check that refused the whole
request as soon as one label's keys were wrong would report that label and hide a second label's
missing parameter, which the caller would then discover only on the next attempt.

**Admission is separate and comes first.** A refusal that judges the request rather than a label keeps
its own status and code and is not reported as `BatchInvalid`: a batch exceeding the label cap is
`413 BatchTooLarge`, an empty batch is `400 InvalidRequest` with reason `batch_empty`, an out-of-range
`start_slot` is `400 InvalidRequest`, and an unknown template id is `404 TemplateNotFound`. On
`POST /api/import/csv` the file's own refusals — an unparsable header, an empty or duplicate column
name, an unparsable row, a file with no data rows, and an unrecognized column of either kind
(`request-data-keys`) — are likewise whole-request refusals raised before any label exists.

**Transport is after validation and is unchanged.** A print job that fails to send once the request has
executed is reported in the `BatchSummary`'s `failed[]` with a `200`, and is not fatal. Atomicity is a
guarantee about validation, not about delivery.

**Supersession.** This requirement supersedes three places in the frozen `docs/SPEC.md`, and restates
their complete post-change contract:

- §2.2's "Validate-then-execute" paragraph, whose "Every label is render-validated first" and "listing
  every failing label" this replaces, because a label can now fail before any render is attempted;
- §2.3's error-contract row `| 422 | BatchInvalid | A rendered label is invalid (same render path as
  /batch). |`;
- §10's code-table row `| BatchInvalid | 422 | One or more /batch labels failed render-validation;
  details.failures lists them. |`, whose post-change reading is that one or more labels failed
  validation, of which rendering is one part.

It supersedes nothing else. §2.2's dispatch matrix and its "Print summary (`BatchSummary`)" paragraph,
every other row of §2.3's error contract, and every other row of §10's code table remain authoritative
and are unchanged. `POST /api/import/csv` is governed on the same terms, which is what that endpoint
already does; no behaviour of it changes here beyond what `request-data-keys` adds.

#### Scenario: Two labels failing different ways are both reported

- **WHEN** `POST /api/batch` is sent two labels, label 0 carrying a `data` key the template does not
  declare and label 1 omitting a required parameter an active item reads
- **THEN** the response is `422 BatchInvalid` and `details.failures` holds two entries, index 0 with
  reason `data_key_unknown` and index 1 with code `MissingField`
- **AND** no ZIP is produced

#### Scenario: A label failing two ways contributes one entry

- **WHEN** a label both carries an unrecognized key and omits a required parameter
- **THEN** `details.failures` holds one entry for that label, carrying `data_key_unknown`, because the
  key check is judged first

#### Scenario: A sheet batch is atomic across its pages

- **WHEN** a `sheet` batch spanning two pages has one failing label on the second page
- **THEN** the response is `422 BatchInvalid` naming that label's index and no PDF is returned, not even
  the page whose labels all passed
- **AND** whether the first page's labels were rendered before the failure was found is not observable
  and is not constrained

#### Scenario: A failing print batch dispatches nothing, once per copy

- **WHEN** `POST /api/print` is sent `copies: 3` for a `data` map carrying an unrecognized key
- **THEN** the response is `422 BatchInvalid` and `details.failures` holds three entries, at indices
  `0`, `1` and `2`, each carrying the code `InvalidRequest` and the reason `data_key_unknown`
- **AND** no print job is dispatched for any copy

#### Scenario: A single-copy print reports one entry

- **WHEN** the same `data` map is sent with `copies` omitted
- **THEN** `details.failures` holds exactly one entry, at index `0`

#### Scenario: `reason` is omitted when the code carries none

- **WHEN** a label fails with a code that carries no reason
- **THEN** its entry carries `index`, `code` and `message`, and no `reason` key

#### Scenario: A batch whose every label passes executes

- **WHEN** every label of a `download` batch passes
- **THEN** the blob is streamed, exactly as today

#### Scenario: An oversized batch is not reported as `BatchInvalid`

- **WHEN** a `POST /api/batch` request exceeds the label cap and also carries a label with an
  unrecognized key
- **THEN** the response is `413 BatchTooLarge`, because admission is judged before any label is
  judged

#### Scenario: A CSV file's own refusal precedes admission

- **WHEN** a `POST /api/import/csv` file both exceeds the label cap and carries an unrecognized data
  column
- **THEN** the response is `400 InvalidRequest` with reason `csv_data_column_unknown`, because the
  file's columns are judged before any label exists to count
