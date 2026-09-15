## Purpose

Defines what the preview pane beside a batch grid shows on the Connect and Import pages, by template
format: for a `sheet` template the artifact the grid would submit, for a `single` template one row the
operator selects; when the sheet preview refuses and what it says; and how the preview follows the
grid as it is edited. Frozen `docs/SPEC.md` describes this preview in one paragraph of §2.0, and this
capability supersedes that paragraph.

## ADDED Requirements

### Requirement: The batch preview shows what the grid would submit

This requirement supersedes the frozen `docs/SPEC.md` §2.0 paragraph "The CSV Import and Homebox
Connect pages render an on-demand, selected-row preview using the same endpoints…", which described
one selected row for both formats. Every other §2.0 statement stays authoritative.

The Connect page and the Import page each carry a preview pane beside the batch grid. What that pane
previews SHALL follow from the template's format, because the format decides what the artifact is.

**For a `sheet` template the artifact is the page, and the preview SHALL be the page.** The pane SHALL
render the batch that Print and Download would submit at that moment: every row of the grid in grid
order, each expanded to `copies` adjacent labels, placed from `start slot`. It SHALL be rendered
through `POST /api/batch` in `download` mode, and the request body SHALL be the body the Download
control would send, less `printer`: the same `template`, the same `labels` in the same order, each
holding the same resolved `data`, and the same `start_slot`. The pane SHALL embed the returned PDF.
No row is chosen, so nothing offers a choice: the grid SHALL render no per-row preview selector for a
`sheet` template.

**For a `single` template the artifact is one label, and the preview SHALL be one label.** The grid
SHALL carry a per-row radio selector; the pane SHALL render the selected row through
`POST /api/render/label` with that row's resolved `data`, and SHALL embed the returned image. A row
the operator has selected SHALL be previewed whatever its validation state: selection is the
operator's choice, and an invalid row is the one they most need to see. Only when no row has been
selected, or the selected row has left the grid, SHALL the first valid row in grid order be previewed
instead, and when that fallback finds no valid row, nothing is previewed. A grid shown before any template is chosen has no
format and SHALL be treated as `single` for the selector's sake; it has no pane, because there is no
template to render with.

The `data` a preview submits for a row SHALL be resolved as the submit path resolves it: against the
entries the service reports for that row, with the same pruning. A preview MUST NOT be built by a
second path that could drift from the one Print and Download use.

A render the service refuses (any non-2xx response) SHALL surface in the pane as a failed preview
carrying the service's message, and SHALL NOT disable Print or Download, whatever the format.

#### Scenario: A sheet template previews the whole batch

- **WHEN** a `sheet` template's grid holds 3 valid rows, `copies` is 2 and `start slot` is 4
- **THEN** the preview request is `POST /api/batch` with `mode: "download"`, `start_slot: 4` and 6
  labels, in the order row 1, row 1, row 2, row 2, row 3, row 3, and the pane embeds the returned PDF

#### Scenario: The preview body is the Download body

- **WHEN** the operator activates Download on a `sheet` template's grid without editing it after the
  preview last rendered
- **THEN** the `labels` and `start_slot` the Download request carries are equal to the ones the
  preview request carried

#### Scenario: A sheet grid renders no row selector

- **WHEN** a `sheet` template's grid renders rows on the Connect page and on the Import page
- **THEN** no per-row preview radio is rendered on either page

#### Scenario: A single grid keeps the selector and previews the selected row

- **WHEN** a `single` template's grid holds two valid rows and the operator selects the second
- **THEN** each row carries a preview radio, and the preview request is `POST /api/render/label`
  with the second row's resolved data

#### Scenario: A single grid falls back to the first valid row

- **WHEN** a `single` template's grid holds an invalid first row and a valid second row and the
  operator has selected nothing
- **THEN** the second row is previewed

#### Scenario: An explicitly selected invalid row is still previewed

- **WHEN** the operator selects row 2 of a `single` template's grid and then clears a required value
  in that row, and the service refuses the render with a non-2xx response carrying a message
- **THEN** row 2 stays selected and is the row requested from `POST /api/render/label`, the pane
  shows the preview as failed with the service's message, and Download is disabled by the invalid
  row and not by the preview

#### Scenario: No valid row and no selection previews nothing

- **WHEN** every row of a `single` template's grid is invalid and the operator has selected nothing
- **THEN** no render request is sent and the pane shows its idle copy

#### Scenario: A refused render never gates the run

- **WHEN** a preview request on either format returns a non-2xx response with a message
- **THEN** the pane shows the preview as failed with that message, and Print and Download keep the
  enabled state the grid's own validity gives them

### Requirement: A sheet preview refuses what the submit path refuses

A sheet preview that showed a subset of the grid would put a page on screen whose slot order is not
the one that prints. So for a `sheet` template the preview SHALL refuse, and SHALL send no render
request, whenever Print and Download refuse for what the grid holds:

- **An invalid row.** While any row fails the grid's validation, the pane SHALL show no PDF and SHALL
  instead state which rows block the preview, naming each by its 1-based position in grid order:
  `Fix row 2 to preview the sheet.` for one, `Fix rows 2, 5 to preview the sheet.` for several. The
  rows named SHALL be exactly the rows Print and Download refuse.
- **A batch over the cap.** While the grid's expanded label count exceeds the 500-label batch limit,
  the pane SHALL show no PDF and SHALL state
  `Over the 500-label limit; reduce the batch to preview the sheet.`

A refusal is not a failed render and SHALL NOT be presented as one: the pane SHALL NOT prefix it with
the failed-preview wording, and the run controls keep the state the grid gives them, which for these
conditions is already disabled.

The refusal SHALL lift the moment the grid no longer meets the condition: repairing the last invalid
row, or bringing the count under the cap, SHALL render the sheet without any further action.

While the service is still reporting the inputs for one or more rows, the resolved batch is not yet
known, so the preview SHALL send no request and the pane SHALL show that it is rendering; the request
follows once the inputs arrive. **This state takes precedence over both refusals.** Until a row's
inputs have arrived, its validity is judged against the template's default inputs, which can name a
required entry the row's own branch does not carry; a refusal built on that would name a row the
operator cannot fix and would lift by itself. So while any row's inputs are pending the pane SHALL
show rendering and SHALL NOT name any row, and a refusal SHALL be computed only from inputs the
service has reported for every row.

An empty grid is not a refusal. With no rows there is nothing to preview and nothing to refuse, and
the pane is not shown at all, as it is not today.

#### Scenario: An invalid row blocks the sheet preview and is named

- **WHEN** a `sheet` template's grid holds 3 rows and row 2 is missing a required value
- **THEN** no batch request is sent, the pane embeds no PDF, and the pane reads
  `Fix row 2 to preview the sheet.`

#### Scenario: Several invalid rows are all named, in order

- **WHEN** rows 2 and 5 of a `sheet` template's grid are invalid
- **THEN** the pane reads `Fix rows 2, 5 to preview the sheet.`

#### Scenario: Repairing the row renders the sheet

- **WHEN** the operator fills the missing value in the one invalid row of a `sheet` template's grid
- **THEN** the refusal is gone, a batch request holding every row is sent, and the pane embeds the PDF

#### Scenario: A batch over the cap is refused without a request

- **WHEN** a `sheet` template's grid holds 2 rows and the operator sets `copies` to 300
- **THEN** no batch request is sent and the pane reads
  `Over the 500-label limit; reduce the batch to preview the sheet.`

#### Scenario: Pending inputs show rendering, not a refusal

- **WHEN** the inputs for one row of a `sheet` template's grid are still being reported, and the
  template's default inputs mark that row as missing a required value that its reported inputs will
  not require
- **THEN** while they are pending the pane shows that it is rendering, names no row and no batch
  request is sent; once they arrive, the row is valid and one batch request holding every row is sent

#### Scenario: A refusal is not a failed render

- **WHEN** the sheet preview is refused for an invalid row
- **THEN** the pane's text does not begin with `Preview failed`

### Requirement: The sheet preview follows the grid

The sheet preview SHALL be live. It SHALL re-render whenever the resolved batch changes: a cell edit, a
row added, duplicated or removed, a change to `copies` or to `start slot`, or a change of template. No
control refreshes it and no size withholds it: the pane always shows the current grid, or a refusal
naming what blocks it.

The cost of a live preview SHALL be bounded by two mechanisms and nothing else:

- **Debounce.** A request SHALL be sent only once the resolved batch has been stable for a short
  settling interval of about 300 ms, keyed on the resolved batch (template, labels and `start_slot`).
  A sequence of edits inside one interval SHALL produce at most one request. A re-render that leaves
  the resolved batch equal SHALL produce no request.
- **Abort.** When the resolved batch changes while a request is in flight, that request SHALL be
  aborted, and its response, should one arrive, SHALL NOT be shown and SHALL NOT be reported as a
  failure. Only the newest batch's render reaches the pane.

While a request is debouncing or in flight the pane SHALL show that it is rendering.

#### Scenario: Edits inside the settling interval coalesce into one request

- **WHEN** the operator makes three edits to a `sheet` template's grid within the settling interval
- **THEN** exactly one batch request is sent, after the interval, carrying the batch as it stood after
  the third edit

#### Scenario: An in-flight render is aborted by a later edit

- **WHEN** a batch request is in flight for a `sheet` template's grid and the operator edits a cell
- **THEN** the in-flight request is aborted, a new request carries the edited batch, and the pane
  shows neither the aborted request's PDF nor a failure for it

#### Scenario: Copies and start slot re-render the sheet

- **WHEN** the operator changes `copies` from 1 to 2 and then `start slot` from 0 to 3
- **THEN** each settled change sends one batch request carrying the new `labels` count and
  `start_slot` respectively

#### Scenario: An unchanged batch sends nothing

- **WHEN** the page re-renders for a reason that leaves every row's resolved data, `copies` and
  `start slot` unchanged
- **THEN** no batch request is sent
