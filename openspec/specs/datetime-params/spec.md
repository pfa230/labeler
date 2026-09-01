# datetime-params Specification

## Purpose
Defines the `datetime` parameter type: how a template declares an instant it wants to print, how the
interpolation token (not the parameter) chooses the format, how a template declares an explicit default
or requires the caller to supply a value, how a caller overrides it, and what control the print form and
the row grids give an operator for it.

## Requirements

### Requirement: A request may override a datetime parameter

A request MAY supply a `datetime` parameter in its `data` map, per label. The service SHALL accept:

- `YYYY-MM-DD`, which resolves to midnight local time on that date;
- `YYYY-MM-DDTHH:MM` or `YYYY-MM-DDTHH:MM:SS` with no offset, read as server-local wall-clock time
  (this is what an HTML date-and-time control submits);
- an RFC 3339 timestamp carrying an offset or `Z`, converted to the server-local timezone.

Surrounding whitespace SHALL be trimmed before parsing. A value that is not one of these forms SHALL be
rejected. A local time that is ambiguous because of a daylight-saving transition SHALL resolve to the
earlier of the two instants.

A value naming a local time that does not exist because of a daylight-saving transition SHALL be handled
by its form, and this is a change from the rule that rejected both forms alike: a **date-only** value
SHALL resolve to the first instant that exists on that local date, and a **date-and-time** value SHALL
still be rejected. A date names a day and a day normally exists; a time names an instant, which may not.
Where a zone skips an entire local date — `Pacific/Apia` had no instant on 2011-12-30 — there is no first
instant to resolve to, and a date-only value SHALL be rejected too, rather than shifted to a neighbouring
date it does not name.

The distinction is load-bearing rather than cosmetic. `{sys.now}` renders `%Y-%m-%d`, and this capability
tells an author to write `default: "{sys.now}"` for the render date, so under the old rule every template
carrying that default would fail for a whole day each year in any zone transitioning at `00:00` — of which
several are in current use — on a server whose clock and timezone are correct, blaming the template for a
migration this capability prescribed.

A `datetime` parameter sent as JSON `null` SHALL be treated exactly as if the request had omitted it,
which now means it is resolved by `param-resolution` rather than taken as the render instant. A value
that is neither a JSON string nor `null` (a number, a boolean, an array, or an object) SHALL be rejected
the same way an unparseable string is. A number in particular SHALL NOT be guessed at: this capability
defines no epoch or serial-date convention.

On the single-label render path, a rejected value SHALL be `400 InvalidRequest` whose message names
the parameter and whose `details.reason` is `datetime_param_invalid`. That slug is an addition to the
reason registry of `docs/SPEC.md` §10.1, which is frozen and therefore does not list it; this
requirement is its published home. It adds a row to the `InvalidRequest` set and changes no other row,
and it does not extend `reason` to a fifth code.

In a batch, validation SHALL be per label: every label carrying a rejected value SHALL appear in the
`details.failures` list of the `422 BatchInvalid` response, each entry naming its label index, the
`InvalidRequest` code and the `datetime_param_invalid` reason. The batch itself stays all-or-nothing,
as it is for every other per-label failure today: one rejected value SHALL fail the whole request, and
no PDF, no ZIP and no print job SHALL be produced or sent for any label in it.

An override SHALL affect only the parameter it names. `{sys.now}` and `{sys.now:<name>}` SHALL
continue to resolve the request's own instant.

#### Scenario: A date-only override on a day with no midnight

- **WHEN** a request sends a date whose local midnight does not exist because the zone transitions at
  `00:00`
- **THEN** the value resolves to the first instant that exists on that date, and a template declaring
  `default: "{sys.now}"` renders on that day like any other

#### Scenario: A date-and-time naming a nonexistent local time is still refused

- **WHEN** a request sends `printed_on: "2026-09-06T00:30"` in a zone where that local time does not exist
- **THEN** the response is `400 InvalidRequest` with `details.reason` `datetime_param_invalid`

#### Scenario: A date-only override

- **WHEN** a request sends `printed_on: "2026-08-19"` for a label printing `{printed_on:long_date}`
- **THEN** the label reads `August 19, 2026`

#### Scenario: A local date-and-time override

- **WHEN** a request sends `printed_on: "2026-08-19T14:30"` for a label printing `{printed_on:time}`
  with the default `time` format `%H:%M`
- **THEN** the label reads `14:30`

#### Scenario: An offset timestamp is converted to server-local time

- **WHEN** a request sends an RFC 3339 value carrying an offset different from the server's
- **THEN** the label prints the corresponding server-local wall-clock time

#### Scenario: An unparseable value is refused

- **WHEN** a request sends `printed_on: "yesterday"`
- **THEN** the response is `400 InvalidRequest`, the message names `printed_on`, and
  `details.reason` is `datetime_param_invalid`

#### Scenario: A null is the same as omitting it

- **WHEN** a request sends `printed_on: null`
- **THEN** the parameter resolves exactly as an omitted one does: its declared `default` if it has one,
  and `422 MissingField` naming `printed_on` if it does not and an active item reads it

#### Scenario: A number is refused rather than guessed at

- **WHEN** a request sends `printed_on: 20260819`
- **THEN** the response is `400 InvalidRequest` with `details.reason` `datetime_param_invalid`

#### Scenario: One bad label fails the whole batch and is named

- **WHEN** a batch of three labels sends an unparseable `printed_on` on the second
- **THEN** the response is `422 BatchInvalid`, no ZIP, PDF or print job is produced, and
  `details.failures` contains one entry for index 1 carrying the `InvalidRequest` code and the
  `datetime_param_invalid` reason

#### Scenario: An override does not move the bare datetime token

- **WHEN** a label prints both `{printed_on}` and `{sys.now}` and the request overrides
  `printed_on` with a past date
- **THEN** `{printed_on}` prints the past date and `{sys.now}` prints today

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

### Requirement: A datetime parameter names an instant, not a rendering

*This requirement supersedes `docs/SPEC.md` §3.0 ("Parameters (`params:`)") except its "Namespace rules
and reserved names" list, which the `interpolation-tokens` capability supersedes, and restates its
complete post-change contract. All other frozen sections remain authoritative. It replaces the
requirement "A template declares a datetime parameter as an instant, not a rendering", which this change
removes.*

A `params:` entry MAY declare `type: datetime`. Such a parameter names one point in time. It carries
no format of its own: what a label prints is decided by the interpolation token that reads it.

A `datetime` parameter accepts exactly three other attributes:

- `default`: as on every other parameter type, and interpolated by the same rules
  (`interpolation-tokens`). It SHALL resolve to one of the request forms this capability accepts below.
  `default: "{sys.now}"` is how a template declares that the parameter means the render **date**; see
  the resolution requirement below for why that is the date and not the wall-clock instant. A
  `default:` that is **not** a string SHALL be rejected at load, naming the parameter, for the reason
  this capability already refuses a numeric request value: it defines no epoch or serial-date
  convention, so `default: 20260819` can only ever fail, and failing it once at load is cheaper for the
  author than failing it on every request.
- `time`: boolean, default `false`. It selects the form control only (see the UI requirement below).
  It SHALL NOT change how a value is parsed, stored, or printed.
- `description`: string, as on every other parameter type.

`format`, `min`, `max`, `multiline` and `values` SHALL be rejected on a `datetime` parameter, with a
validation message naming both the parameter and the offending attribute. `format` is rejected because
the format belongs to the token. `default` is no longer among them: it was rejected while the default
was *defined* to be the render instant, and that definition is gone. `enum` is no longer among them
either, for a different reason: it is not an attribute of any parameter type, so it is refused before
this list is reached.

`enum:` SHALL NOT be an attribute of a `params:` entry of any type. A `params:` entry carrying it SHALL
be refused at load as an unknown key, with an error naming the file, the offending parameter and the key
`enum`, and the file quarantined under the `template-registry` rules while the server still starts. That
message SHALL be the service's generic unknown-key message and SHALL NOT be type-specific: the key is
part of no type's schema, and a pointed message would imply it is valid somewhere else. The refusal
SHALL turn on the key being written, whatever it carries, including an explicit YAML null.

`time:` SHALL be rejected on a parameter of any other type, with a validation message naming the
parameter.

These rules turn on whether the key is **written**, not on what it holds: a forbidden attribute
present with an explicit YAML null (`values:` with no value) SHALL be rejected exactly as one
carrying a value, and `time:` written with an explicit null SHALL be rejected rather than silently
taken as `false`. `default:` is not a forbidden attribute on any type, so `default:` written with an
explicit null SHALL be treated as an absent default here exactly as it is everywhere else.

A `datetime` parameter SHALL NOT be usable where a template expects a numeric or dimension value
(a `format` width or height, `font_weight`, or any other `${param}` reference resolved to a number).
Such a reference SHALL fail validation with a message naming the parameter and the context.

A `datetime` parameter used in a `when:` predicate SHALL compare against its bare ISO `%Y-%m-%d`
rendering.

A rejected declaration quarantines the template file under the existing rules of the
`template-registry` capability; it SHALL NOT abort startup.

The post-change set of parameter types is below. Every row's omission behavior is one rule, owned by the
`param-resolution` capability, and no row names a value the service picks:

| Type | YAML attributes | Request value | Behavior when omitted from the request | UI form control |
| --- | --- | --- | --- | --- |
| `string` | `default`, `multiline` (bool), `description` | String scalar | If `default` set: uses `default`. If no `default`: `422 MissingField` when rendered in active layout. | Text input (`multiline: false`) or textarea (`multiline: true`) |
| `length` | `default`, `min`, `max`, `description` | Number or dimension string (`80`, `"80mm"`) | If `default` set: uses `default`. If no `default`: `422 MissingField` when active. | Number input with unit suffix, or slider (if `min`/`max` provided) |
| `integer` | `default`, `min`, `max`, `description` | Integer (`400`) | If `default` set: uses `default`. If no `default`: `422 MissingField` when active. | Number input / stepper |
| `number` | `default`, `min`, `max`, `description` | Float / number (`1.5`) | If `default` set: uses `default`. If no `default`: `422 MissingField` when active. | Number input with step |
| `boolean` | `default`, `description` | `true` / `false` | If `default` set: uses `default`. If no `default`: `422 MissingField` when active. | Toggle switch / checkbox |
| `enum` | `values` (required list), `default`, `description` | String matching `values` | If `default` set: uses `default`. If no `default`: `422 MissingField` when active. | Dropdown / segmented button group |
| `datetime` | `default`, `time` (bool, default `false`), `description` | `YYYY-MM-DD`, `YYYY-MM-DDTHH:MM[:SS]`, or an RFC 3339 timestamp | If `default` set: uses `default`. If no `default`: `422 MissingField` when active. | Date picker (`time: false`) or date-and-time picker (`time: true`) |

Parameter naming is governed by the `interpolation-tokens` capability, which owns that rule. This
requirement adds nothing to it and restates none of it.

A `datetime` parameter named `p` claims the interpolation token `{p}` and, for every format name
`<fmt>`, the token `{p:<fmt>}`. Because parameter names are unique and may contain neither a dot nor a
colon, no two parameters can claim the same token.

#### Scenario: A datetime parameter declares only a time flag and a description

- **WHEN** a template declares `printed_on: { type: datetime, time: false, description: "Print date" }`
- **THEN** the template loads, and `printed_on` appears in the template's `params` on
  `GET /templates` and `GET /templates/{id}` with `type: "datetime"` and `time: false`

#### Scenario: A format attribute on the parameter is refused

- **WHEN** a template declares `printed_on: { type: datetime, format: long_date }`
- **THEN** the template fails validation with a message naming `printed_on` and `format`, and the
  file is quarantined while the server still starts

#### Scenario: A literal default on a datetime parameter is accepted

- **WHEN** a template declares `printed_on: { type: datetime, default: "2026-01-01" }`
- **THEN** the template loads, and a request omitting `printed_on` prints `2026-01-01`

#### Scenario: A datetime parameter declares the render date

- **WHEN** a template declares `printed_on: { type: datetime, default: "{sys.now}" }` and a request
  omits `printed_on`
- **THEN** `{printed_on}` prints the request's own date, exactly as `{sys.now}` does
- **AND** `{printed_on:time}` prints `00:00`, because the default resolved to local midnight of that
  date, where `{sys.now:time}` prints the captured wall-clock time

#### Scenario: An explicitly null forbidden attribute is still refused

- **WHEN** a template declares `printed_on` as `type: datetime` with `values:` written and left
  empty, so it parses as an explicit null
- **THEN** the template fails validation with a message naming `printed_on` and `values`

#### Scenario: An enum key on an enum parameter is refused rather than emptied

- **WHEN** a template declares `size: { type: enum, enum: [small, large] }`
- **THEN** the file is quarantined with an error naming the file, `size` and the unknown key `enum`,
  and not with "enum values must not be empty"

#### Scenario: An enum key on an integer parameter is refused rather than ignored

- **WHEN** a template declares `weight: { type: integer, default: 400, enum: [100, 400, 700] }`
- **THEN** the file is quarantined with an error naming the file, `weight` and the unknown key `enum`,
  rather than loading with the key discarded

#### Scenario: An enum key on a datetime parameter gets the unknown-key message

- **WHEN** a template declares `printed_on: { type: datetime, enum: ["2026-01-01"] }`
- **THEN** the file is quarantined with the same unknown-key error naming `printed_on` and `enum`,
  and not with a message saying `enum` is unsupported on datetime parameters

#### Scenario: An explicitly null enum key is refused too

- **WHEN** a template declares `weight` as `type: integer` with `enum:` written and left empty
- **THEN** the file is quarantined with the unknown-key error naming `weight` and `enum`

#### Scenario: An explicitly null default is an absent default

- **WHEN** a template declares `printed_on` as `type: datetime` with `default:` written and left empty
- **THEN** the template loads with no default, and a request omitting `printed_on` fails with
  `422 MissingField` when an active item reads it

#### Scenario: An explicitly null time flag is refused, not defaulted

- **WHEN** a template declares `printed_on` as `type: datetime` with `time:` written and left empty
- **THEN** the template fails validation naming `printed_on` and `time`, rather than loading with
  `time` taken as `false`

#### Scenario: The time flag is refused on another parameter type

- **WHEN** a template declares `title: { type: string, time: true }`
- **THEN** the template fails validation with a message naming `title` and `time`

#### Scenario: A datetime parameter cannot drive a dimension

- **WHEN** a template declares `printed_on: { type: datetime }` and references it as a `format`
  width, a `font_weight`, or any other numeric parameter reference
- **THEN** the template fails validation with a message naming `printed_on` and the context

#### Scenario: A when predicate compares the bare ISO date

- **WHEN** a container declares `when: { printed_on: "2026-08-19" }` and the request sets
  `printed_on` to `2026-08-19T14:30`
- **THEN** the container is rendered, because the comparison uses the parameter's `%Y-%m-%d` rendering

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
