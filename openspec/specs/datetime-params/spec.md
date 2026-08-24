# datetime-params Specification

## Purpose
Defines the `datetime` parameter type: how a template declares an instant it wants to print, how the
interpolation token (not the parameter) chooses the format, how that instant defaults to the render
instant of the request, how a caller overrides it, and what control the print form and the row grids
give an operator for it.

## Requirements

### Requirement: A template declares a datetime parameter as an instant, not a rendering

*This requirement supersedes `docs/SPEC.md` §3.0 ("Parameters (`params:`)") and restates its complete
post-change contract. All other frozen sections remain authoritative.*

A `params:` entry MAY declare `type: datetime`. Such a parameter names one point in time. It carries
no format of its own: what a label prints is decided by the interpolation token that reads it.

A `datetime` parameter accepts exactly two other attributes:

- `time`: boolean, default `false`. It selects the form control only (see the UI requirement below).
  It SHALL NOT change how a value is parsed, stored, or printed.
- `description`: string, as on every other parameter type.

`format`, `default`, `min`, `max`, `multiline`, `values` and `enum` SHALL be rejected on a
`datetime` parameter, with a validation message naming both the parameter and the offending
attribute. `format` is rejected because the format belongs to the token; `default` is rejected
because the default is defined to be the render instant.

`time:` SHALL be rejected on a parameter of any other type, with a validation message naming the
parameter.

These rules turn on whether the key is **written**, not on what it holds: a forbidden attribute
present with an explicit YAML null (`default:` with no value) SHALL be rejected exactly as one
carrying a value, and `time:` written with an explicit null SHALL be rejected rather than silently
taken as `false`.

A `datetime` parameter SHALL NOT be usable where a template expects a numeric or dimension value
(a `format` width or height, `font_weight`, or any other `${param}` reference resolved to a number).
Such a reference SHALL fail validation with a message naming the parameter and the context.

A rejected declaration quarantines the template file under the existing rules of the
`template-registry` capability; it SHALL NOT abort startup.

The post-change set of parameter types is:

| Type | YAML attributes | Request value | Behavior when omitted from the request | UI form control |
| --- | --- | --- | --- | --- |
| `string` | `default`, `multiline` (bool), `description` | String scalar | If `default` set: uses `default`. If no `default`: `422 MissingField` when rendered in active layout. | Text input (`multiline: false`) or textarea (`multiline: true`) |
| `length` | `default`, `min`, `max`, `description` | Number or dimension string (`80`, `"80mm"`) | If `default` set: uses `default`. If no `default`: `422 MissingField` when active. | Number input with unit suffix, or slider (if `min`/`max` provided) |
| `integer` | `default`, `min`, `max`, `enum` (list), `description` | Integer (`400`) | If `default` set: uses `default`. If no `default`: `422 MissingField` when active. | Number input / stepper / dropdown (if `enum` provided) |
| `number` | `default`, `min`, `max`, `description` | Float / number (`1.5`) | If `default` set: uses `default`. If no `default`: `422 MissingField` when active. | Number input with step |
| `boolean` | `default`, `description` | `true` / `false` | If `default` set: uses `default`. If no `default`: `false`. | Toggle switch / checkbox |
| `enum` | `values` (required list), `default`, `description` | String matching `values` | If `default` set: uses `default`. If no `default`: first value in `values`. | Dropdown / segmented button group |
| `datetime` | `time` (bool, default `false`), `description` | `YYYY-MM-DD`, `YYYY-MM-DDTHH:MM[:SS]`, or an RFC 3339 timestamp | The render instant of the request. Never `MissingField`. | Date picker (`time: false`) or date-and-time picker (`time: true`) |

Namespace rules and reserved names, post-change:

- Parameter names SHALL match `^[a-zA-Z0-9_-]+$`.
- The names `datetime` and `vars`, the prefixes `datetime.` and `vars.`, and any name containing a
  dot SHALL be rejected at template load (`422 TemplateInvalid`).
- A `datetime` parameter named `p` additionally claims the interpolation names `p` and `p.<format>`
  for every `<format>`. Because parameter names are unique and cannot contain a dot, no two
  parameters can claim the same name.

#### Scenario: A datetime parameter declares only a time flag and a description

- **WHEN** a template declares `printed_on: { type: datetime, time: false, description: "Print date" }`
- **THEN** the template loads, and `printed_on` appears in the template's `params` on
  `GET /templates` and `GET /templates/{id}` with `type: "datetime"` and `time: false`

#### Scenario: A format attribute on the parameter is refused

- **WHEN** a template declares `printed_on: { type: datetime, format: long_date }`
- **THEN** the template fails validation with a message naming `printed_on` and `format`, and the
  file is quarantined while the server still starts

#### Scenario: A default on a datetime parameter is refused

- **WHEN** a template declares `printed_on: { type: datetime, default: "2026-01-01" }`
- **THEN** the template fails validation with a message naming `printed_on` and `default`

#### Scenario: An explicitly null forbidden attribute is still refused

- **WHEN** a template declares `printed_on` as `type: datetime` with `default:` written and left
  empty, so it parses as an explicit null
- **THEN** the template fails validation with a message naming `printed_on` and `default`

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

### Requirement: A datetime parameter owns an interpolation namespace shaped like `{datetime}`

*This requirement supersedes the "Token types and precedence" list in `docs/SPEC.md` §8 ("Data
binding") and restates its complete post-change contract. The rest of §8 remains authoritative.*

Interpolation stays substitution-only (ADR-0010, ADR-0055). Tokens resolve in this precedence order,
highest first, after which `{{` and `}}` emit literal braces:

1. **`{datetime}`** (bare) resolves to the request's instant formatted as ISO `%Y-%m-%d`. Always
   succeeds; no configuration required.
2. **`{datetime.<name>}`** resolves a named strftime format from the `datetime_formats` app setting,
   applied to the request's instant. An unknown `<name>` is `422 MissingField`.
3. **`{<p>}` and `{<p>.<name>}`**, where `<p>` is a parameter of `type: datetime` declared by the
   template, resolve that parameter's instant: bare `{<p>}` as ISO `%Y-%m-%d`, and `{<p>.<name>}`
   through the `datetime_formats` entry `<name>`. An unknown `<name>` SHALL be `422 MissingField`
   reporting the field as `<p>.<name>`. Because `datetime_formats` is runtime state, a `<name>` that
   does not resolve SHALL NOT be an error at template load.
4. **`{vars.<key>}`** resolves from the variables store.
5. **`{param_name}`** resolves from the request `data` map, falling back to declared `default`s in
   `params:`.

A token whose head names a declared `datetime` parameter SHALL NOT fall through to the request `data`
map, whatever the request carries under that key.

A missing key or unresolved token is `422 MissingField`. JSON scalars are stringified: strings as-is,
numbers and booleans via their textual form, `null` as empty, other values via JSON.

A `datetime` parameter used in a `when:` predicate SHALL compare against its bare ISO `%Y-%m-%d`
rendering.

#### Scenario: The bare parameter token prints an ISO date

- **WHEN** a template declaring `printed_on: { type: datetime }` renders `"Printed {printed_on}"`
  with `printed_on` set to `2026-08-23`
- **THEN** the label reads `Printed 2026-08-23`

#### Scenario: The dotted parameter token prints a named format

- **WHEN** the same template renders `"Printed {printed_on.long_date}"` with the default
  `long_date` format `%B %-d, %Y`
- **THEN** the label reads `Printed August 23, 2026`

#### Scenario: An unknown format name fails at render, not at load

- **WHEN** a template renders `{printed_on.no_such_format}`
- **THEN** the template loads successfully, and the render returns `422 MissingField` naming
  `printed_on.no_such_format`

#### Scenario: A request data key cannot shadow the parameter namespace

- **WHEN** a request sends `data: { "printed_on.long_date": "whatever" }` for a template declaring
  `printed_on` as a `datetime` parameter
- **THEN** `{printed_on.long_date}` still renders the parameter's instant through the `long_date`
  format, and the request key is ignored

### Requirement: A datetime parameter defaults to the render instant of its request

Every render request SHALL capture one instant, in the server-local timezone (controlled by `TZ`).
That instant is what `{datetime}` and `{datetime.<name>}` already use.

When a request omits a `datetime` parameter, or sends it as an empty string, that parameter SHALL
resolve to the same captured instant. It SHALL NOT be a `MissingField`, and the service SHALL NOT
read the clock a second time while resolving parameters.

Every label in one batch, sheet, or ZIP SHALL therefore share one instant for every un-overridden
`datetime` parameter and for every `{datetime}` token, so a run spanning midnight cannot print two
different dates.

A `datetime` parameter SHALL NOT be reported as a request field the caller must supply: it SHALL be
absent from the field list a template advertises, whether the layout references it as `{<p>}` or as
`{<p>.<name>}`.

A thumbnail or preview render, which substitutes placeholder values for request fields, SHALL render
a `datetime` parameter as the current instant rather than as a placeholder string.

#### Scenario: An omitted parameter prints today

- **WHEN** a request renders a label using `{printed_on.iso_date}` and sends no `printed_on`
- **THEN** the label prints the server's current date through the `iso_date` format

#### Scenario: A blank string is the same as omitting it

- **WHEN** a request sends `printed_on: ""`
- **THEN** the label prints the request's instant, with no error

#### Scenario: One sheet spanning midnight prints one date

- **WHEN** a sheet of labels renders `{printed_on}` on every slot and the render crosses midnight
- **THEN** every slot prints the same date, equal to what `{datetime}` prints on that sheet

#### Scenario: The advertised field list omits the parameter

- **WHEN** a template's layout references `{printed_on}` and `{printed_on.long_date}` and nothing
  else that is data-bound
- **THEN** the field list the template advertises is empty, and neither `printed_on` nor
  `printed_on.long_date` appears in it

#### Scenario: A thumbnail prints a real date

- **WHEN** a thumbnail is rendered for a template printing `{printed_on.short_date}`
- **THEN** the thumbnail shows the current date in that format, not the literal text
  `printed_on.short_date`

### Requirement: A request may override a datetime parameter

A request MAY supply a `datetime` parameter in its `data` map, per label. The service SHALL accept:

- `YYYY-MM-DD`, which resolves to midnight local time on that date;
- `YYYY-MM-DDTHH:MM` or `YYYY-MM-DDTHH:MM:SS` with no offset, read as server-local wall-clock time
  (this is what an HTML date-and-time control submits);
- an RFC 3339 timestamp carrying an offset or `Z`, converted to the server-local timezone.

Surrounding whitespace SHALL be trimmed before parsing. A value that is not one of these forms, or
that names a local time that does not exist because of a daylight-saving transition, SHALL be
rejected. A local time that is ambiguous because of a daylight-saving transition SHALL resolve to the
earlier of the two instants.

A `datetime` parameter sent as JSON `null` SHALL be treated exactly as if the request had omitted it.
A value that is neither a JSON string nor `null` (a number, a boolean, an array, or an object) SHALL
be rejected the same way an unparseable string is. A number in particular SHALL NOT be guessed at:
this capability defines no epoch or serial-date convention.

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

An override SHALL affect only the parameter it names. `{datetime}` and `{datetime.<name>}` SHALL
continue to resolve the request's own instant.

#### Scenario: A date-only override

- **WHEN** a request sends `printed_on: "2026-08-19"` for a label printing `{printed_on.long_date}`
- **THEN** the label reads `August 19, 2026`

#### Scenario: A local date-and-time override

- **WHEN** a request sends `printed_on: "2026-08-19T14:30"` for a label printing `{printed_on.time}`
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
- **THEN** the label prints the request's instant, with no error

#### Scenario: A number is refused rather than guessed at

- **WHEN** a request sends `printed_on: 20260819`
- **THEN** the response is `400 InvalidRequest` with `details.reason` `datetime_param_invalid`

#### Scenario: One bad label fails the whole batch and is named

- **WHEN** a batch of three labels sends an unparseable `printed_on` on the second
- **THEN** the response is `422 BatchInvalid`, no ZIP, PDF or print job is produced, and
  `details.failures` contains one entry for index 1 carrying the `InvalidRequest` code and the
  `datetime_param_invalid` reason

#### Scenario: An override does not move the bare datetime token

- **WHEN** a label prints both `{printed_on}` and `{datetime}` and the request overrides
  `printed_on` with a past date
- **THEN** `{printed_on}` prints the past date and `{datetime}` prints today

### Requirement: The print form and the row grids carry a datetime parameter

The print form SHALL render a `datetime` parameter as a date control when `time` is `false` and as a
date-and-time control when `time` is `true`.

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
