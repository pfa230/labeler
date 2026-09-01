## MODIFIED Requirements

### Requirement: The print form and the row grids carry a datetime parameter

The print form SHALL render a `datetime` parameter the service reports as an input for the current
selection as a date control when `time` is `false` and as a date-and-time control when `time` is
`true`. A `datetime` parameter the template reads only inside a branch the current selection
deactivates is not reported as an input and SHALL NOT be rendered, on the same rule that governs
every other control (`template-inputs`).

The form SHALL seed the control from the `default` the input list publishes for it, which is the value
the render path resolves for that parameter (`template-inputs`), and SHALL leave it empty when the list
publishes none — which is the case both for a parameter declaring no `default:` and for one whose
declared default failed to resolve. It SHALL NOT read a default out of the raw parameter declaration. It
SHALL NOT seed the operator's browser date: that was the client half of the render-instant fallback this
change removes, and it made the form print a value no template declared. The consequence the removed
rule recorded — that a browser and server straddling a date boundary print the browser's date — goes
with it.

A published `datetime` default is a bare `YYYY-MM-DD`, because that is the form the render path coerces a
datetime to. A date control holds it as published. A date-and-time control cannot, so a screen seeding
one SHALL widen the published value to `YYYY-MM-DDT00:00`, which names the instant the service resolved
and which that control holds. This is the only reshaping any screen performs on a published default, and
it is confined to the seeded control: what the screen shows as the entry's default, and what the
template detail's report carries, stay the published value.

An entry whose declared default failed to resolve SHALL be presented as one with no default: an empty
control, marked required, with the failure's message surfaced against it (`template-inputs`). The
operator SHALL still be able to supply an instant and print.

Clearing the control SHALL submit an omission. What that omission prints is `param-resolution`'s answer:
the declared default, or `422 MissingField` when there is none.

A blank `datetime` parameter SHALL be flagged as a missing required value, in the print form, the CSV
import grid and the connector grid alike, exactly when the input list the service reports marks it
`required` — on the same terms every other parameter type is flagged, and read from that field rather
than re-derived (`template-inputs`). That is the case when the parameter declares no `default:`, and
also when its declared default fails to resolve, since neither leaves the service a value to use. A
`datetime` parameter whose declared default resolves SHALL NOT be flagged, for the same reason.

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

#### Scenario: A tokened default seeds the control

- **WHEN** the print form loads a template declaring `printed_on: { type: datetime, default: "{sys.now}" }`
  with `time: false`
- **THEN** the date control holds the date the service resolved, rather than being empty

#### Scenario: A date-and-time control widens the published value

- **WHEN** the same parameter declares `time: true` and the list publishes `default: "2026-09-01"`
- **THEN** the date-and-time control holds `2026-09-01T00:00`, while the value shown as the entry's
  default remains `2026-09-01`

#### Scenario: A datetime default that cannot resolve leaves the control empty and required

- **WHEN** `printed_on` declares `default: "{vars.stamp}"` and the store holds no `stamp`
- **THEN** the control is empty, the entry is flagged as needing a value, and the failure's message is
  surfaced against it

#### Scenario: A cleared control still prints

- **WHEN** `printed_on` declares `default: "{sys.now}"` and an operator clears the control and submits
- **THEN** the request omits the parameter and the label prints the server's date

#### Scenario: A cleared control with no default blocks the submission

- **WHEN** `printed_on` declares no `default:` and an operator clears the control
- **THEN** the form flags it as a missing required value, as it would a blank `string` parameter

#### Scenario: A blank cell with no default blocks the run

- **WHEN** a CSV import row leaves a `datetime` column empty and the parameter declares no `default:`
- **THEN** the row is flagged and the run is blocked, rather than the label printing the server's
  instant

#### Scenario: A blank cell is not a missing value

- **WHEN** a CSV import row leaves a `datetime` column empty and the parameter declares
  `default: "{sys.now}"`
- **THEN** the row is valid, the run is not blocked, and the label prints the request's date

#### Scenario: A blank cell whose default cannot resolve blocks the run

- **WHEN** a CSV import row leaves a `datetime` column empty, the parameter declares
  `default: "{vars.stamp}"`, and the store holds no `stamp`
- **THEN** the row is flagged and the run is blocked, because the list reports that entry `required`,
  rather than the row being submitted for a label the service would refuse

#### Scenario: An unparseable cell blocks the run

- **WHEN** a CSV import row carries `printed_on` as `not a date`
- **THEN** that row is flagged, and the run is blocked until the cell is corrected or cleared

#### Scenario: A value only the server can reject lands on its row

- **WHEN** a grid row carries a well-formed `printed_on` that names a local time the server's
  timezone does not have, and the run is submitted
- **THEN** the run fails, and the server's message for that label is annotated on the row it came
  from rather than only at the form

### Requirement: A datetime parameter is resolved like every other parameter

A `datetime` parameter carries no resolution rule of its own. When a request omits it, or sends it as
an empty string or JSON `null`, it SHALL be resolved by the `param-resolution` capability: its declared
`default` if it has one, and `422 MissingField` if it does not and an active layout item reads it. The
empty string keeps meaning "omitted" so that a cleared form control reaches the same rule an absent key
does. That rule is about a *request* value: a declared `default: ""` on a `datetime` is not an omission
but a default resolving to text the parser rejects, and is `param_default_unresolvable` naming the
parameter.

A `datetime` parameter's `default` SHALL be interpolated and then parsed by the same parser that reads a
supplied value, so a default resolving to text that parser rejects is a resolution failure under
`param-resolution`, naming the parameter.

`{sys.now}` renders as `%Y-%m-%d`, so `default: "{sys.now}"` resolves to **the first instant of the render
date**, not to the wall-clock instant — local midnight on all but the one day a year a zone transitioning
at `00:00` does not have one, per the override requirement below. A template that needs the time of day attaches a format whose
output this capability's parser accepts, which means a `datetime_formats` entry producing
`YYYY-MM-DDTHH:MM[:SS]`. None of the shipped formats does, and this change adds none: the shipped set is
an application setting an operator may already have replaced wholesale, so a new built-in would not
reach the installs that need it, and nothing in this capability requires second precision. A deployment
that wants it configures one entry.

Because `interpolation-tokens` requires one clock read per request, every label in one batch, sheet or
ZIP that resolves `{sys.now}` — through a token or through a default — SHALL share one instant, so a run
spanning midnight cannot print two different dates.

A `datetime` parameter SHALL appear in the request-field list a template advertises on exactly the terms
every other declared parameter does. It was kept off that list because it could never be missing; one
declaring no `default:` now can be, so it becomes `required` and the list carries it, by the same rule
that carries every other required name.

A thumbnail or preview render SHALL supply the current instant as the placeholder for a `datetime`
parameter it invents for, rather than a placeholder string, because a placeholder string is not a legal
instant. Which parameters it invents for is `template-inputs`' rule and not this capability's: a
`datetime` whose declared `default:` **resolves** is not one of them, because the service has a value for
it; one declaring none, and one whose declared default fails to resolve, both are, because in neither
case does the service have a value. This is placeholder substitution and not a default: it never reaches
a render a caller asked for.

#### Scenario: A datetime whose default cannot resolve is invented for in a preview

- **WHEN** a thumbnail is rendered for a template printing `{printed_on:short_date}` where `printed_on`
  declares `default: "{vars.stamp}"` and the store holds no `stamp`
- **THEN** the thumbnail shows the current date in that format, as it does for a `datetime` declaring no
  default, rather than failing with `param_default_unresolvable`

#### Scenario: An omitted parameter with no default fails

- **WHEN** a template declares `printed_on: { type: datetime }`, an active item renders
  `{printed_on:short_date}`, and the request omits `printed_on`
- **THEN** the response is `422 MissingField` naming `printed_on`

#### Scenario: A blank string is the same as omitting it

- **WHEN** a request sends `printed_on: ""` and the parameter declares `default: "{sys.now}"`
- **THEN** the label prints the request's date, with no error

#### Scenario: A blank string on an undefaulted parameter fails

- **WHEN** a request sends `printed_on: ""` and the parameter declares no `default`
- **THEN** the response is `422 MissingField` naming `printed_on`

#### Scenario: One sheet spanning midnight prints one date

- **WHEN** a sheet of labels renders `{printed_on}` on every slot, `printed_on` declares
  `default: "{sys.now}"`, and the render crosses midnight
- **THEN** every slot prints the same date, equal to what `{sys.now}` prints on that sheet

#### Scenario: The advertised field list treats it like any other parameter

- **WHEN** a template's layout references `{printed_on}` and `{printed_on:long_date}` and nothing
  else that is data-bound
- **THEN** `printed_on` appears once in the field list the template advertises, and the
  format-carrying spelling does not appear as a separate entry

#### Scenario: A thumbnail prints a real date

- **WHEN** a thumbnail is rendered for a template printing `{printed_on:short_date}` where
  `printed_on` declares no `default`
- **THEN** the thumbnail shows the current date in that format, not the literal text
  `printed_on:short_date` and not a `422`
