## MODIFIED Requirements

### Requirement: A datetime parameter defaults to the render instant of its request

The `interpolation-tokens` capability requires every render request to capture one instant and to read
the clock exactly once. That instant is what `{sys.now}` and `{sys.now:<name>}` resolve, and this
requirement extends it to declared parameters rather than restating it.

When a request omits a `datetime` parameter, or sends it as an empty string, that parameter SHALL
resolve to that same captured instant. It SHALL NOT be a `MissingField`.

Every label in one batch, sheet, or ZIP SHALL therefore share one instant for every un-overridden
`datetime` parameter and for every `{sys.now}` token, so a run spanning midnight cannot print two
different dates.

A `datetime` parameter SHALL NOT be reported as a request field the caller must supply: it SHALL be
absent from the field list a template advertises, whether the layout references it as `{<p>}` or as
`{<p>:<name>}`.

An input list (`template-inputs`) SHALL nevertheless hold an entry for a `datetime` parameter that an
active item reads, carrying control `date` or `datetime` and `required` false. That entry offers the
operator an override; it is not a field the caller must supply, and it does not appear in the field
list above.

A thumbnail or preview render, which substitutes placeholder values for request fields, SHALL render
a `datetime` parameter as the current instant rather than as a placeholder string.

#### Scenario: An omitted parameter prints today

- **WHEN** a request renders a label using `{printed_on:iso_date}` and sends no `printed_on`
- **THEN** the label prints the server's current date through the `iso_date` format

#### Scenario: A blank string is the same as omitting it

- **WHEN** a request sends `printed_on: ""`
- **THEN** the label prints the request's instant, with no error

#### Scenario: One sheet spanning midnight prints one date

- **WHEN** a sheet of labels renders `{printed_on}` on every slot and the render crosses midnight
- **THEN** every slot prints the same date, equal to what `{sys.now}` prints on that sheet

#### Scenario: The advertised field list omits the parameter

- **WHEN** a template's layout references `{printed_on}` and `{printed_on:long_date}` and nothing
  else that is data-bound
- **THEN** the field list the template advertises is empty, and neither `printed_on` nor
  `printed_on:long_date` appears in it

#### Scenario: The input list offers the override without requiring it

- **WHEN** a template's layout references `{printed_on.long_date}` unconditionally
- **THEN** the input list holds an entry for `printed_on` with control `date` and `required` false

#### Scenario: A thumbnail prints a real date

- **WHEN** a thumbnail is rendered for a template printing `{printed_on:short_date}`
- **THEN** the thumbnail shows the current date in that format, not the literal text
  `printed_on:short_date`

### Requirement: The print form and the row grids carry a datetime parameter

The print form SHALL render a `datetime` parameter the service reports as an input for the current
selection as a date control when `time` is `false` and as a date-and-time control when `time` is
`true`. A `datetime` parameter the template reads only inside a branch the current selection
deactivates is not reported as an input and SHALL NOT be rendered, on the same rule that governs
every other control (`template-inputs`).

The form SHALL seed the control with the operator's current browser date, and current time when
`time` is `true`, so that what will print is visible before the operator touches anything. The
seeded value is submitted like any other override; consequently, when the browser timezone and the
server timezone straddle a date boundary, the seeded value is the browser's date. Clearing the
control SHALL be valid and SHALL defer to the server's instant.

A blank `datetime` parameter SHALL NOT be flagged as a missing required value anywhere a value is
required today: the print form, the CSV import grid, and the connector grid.

The CSV import grid and the connector grid SHALL accept a `datetime` parameter as a text cell taking
the same three input forms as the API. A cell that cannot be parsed SHALL be flagged on its row,
alongside the existing per-row validation, and SHALL block the run until it is corrected or cleared.
Both grids SHALL apply the same rule and report the same message.

That client-side check covers the input's shape and calendar validity only. Whether a well-formed
local instant exists in the server's timezone is the server's to decide. A value the client accepts
but the server rejects SHALL be annotated on the row it came from, through the same path that already
carries a `422 BatchInvalid` failure back to its row in both grids.

#### Scenario: The control follows the time flag

- **WHEN** the print form renders a template declaring `printed_on` with `time: false` and
  `stamped_at` with `time: true`
- **THEN** `printed_on` is a date control and `stamped_at` is a date-and-time control

#### Scenario: A datetime parameter in an inactive branch has no control

- **WHEN** a template reads `stamped_at` only inside a container gated on `mode: full` and the
  operator selects `mode: brief`
- **THEN** the print form renders no control for `stamped_at`, and the label prints

#### Scenario: A cleared control still prints

- **WHEN** an operator clears the seeded date and submits
- **THEN** the request omits the parameter and the label prints the server's instant

#### Scenario: A blank cell is not a missing value

- **WHEN** a CSV import row leaves a `datetime` column empty
- **THEN** the row is valid, the run is not blocked, and the label prints the request's instant

#### Scenario: An unparseable cell blocks the run

- **WHEN** a CSV import row carries `printed_on` as `not a date`
- **THEN** that row is flagged, and the run is blocked until the cell is corrected or cleared

#### Scenario: A value only the server can reject lands on its row

- **WHEN** a grid row carries a well-formed `printed_on` that names a local time the server's
  timezone does not have, and the run is submitted
- **THEN** the run fails, and the server's message for that label is annotated on the row it came
  from rather than only at the form
