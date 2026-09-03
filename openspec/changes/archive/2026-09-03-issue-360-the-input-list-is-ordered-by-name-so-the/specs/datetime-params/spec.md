# Delta: datetime-params — params container repartitioned to sequence

## MODIFIED Requirements

### Requirement: A datetime parameter names an instant, not a rendering

*This requirement supersedes `docs/SPEC.md` §3.0 ("Parameters (`params:`)") except its opening
declaration/container example (now governed by `template-inputs: Template params are declared as a
sequence and published as an array`) and its "Namespace rules and reserved names" list (governed by
`interpolation-tokens: A bare name is a bare name, and no word is reserved`), and restates the
per-entry type/attribute contract for that section. All other frozen sections remain authoritative. It
replaces the requirement "A template declares a datetime parameter as an instant, not a rendering",
which this change removes.*

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

The `list` row is added by the change that introduces that type. Its own rules, meaning which attributes
it refuses, what its `default:` may hold, what a request may send, and where a list may not be used, are
the `list-params` capability's and are not restated here; this table stays the single place the complete
set of types is published.

| Type | YAML attributes | Request value | Behavior when omitted from the request | UI form control |
| --- | --- | --- | --- | --- |
| `string` | `default`, `multiline` (bool), `description` | String scalar | If `default` set: uses `default`. If no `default`: `422 MissingField` when rendered in active layout. | Text input (`multiline: false`) or textarea (`multiline: true`) |
| `length` | `default`, `min`, `max`, `description` | Number or dimension string (`80`, `"80mm"`) | If `default` set: uses `default`. If no `default`: `422 MissingField` when active. | Number input with unit suffix, or slider (if `min`/`max` provided) |
| `integer` | `default`, `min`, `max`, `description` | Integer (`400`) | If `default` set: uses `default`. If no `default`: `422 MissingField` when active. | Number input / stepper |
| `number` | `default`, `min`, `max`, `description` | Float / number (`1.5`) | If `default` set: uses `default`. If no `default`: `422 MissingField` when active. | Number input with step |
| `boolean` | `default`, `description` | `true` / `false` | If `default` set: uses `default`. If no `default`: `422 MissingField` when active. | Toggle switch / checkbox |
| `enum` | `values` (required list), `default`, `description` | String matching `values` | If `default` set: uses `default`. If no `default`: `422 MissingField` when active. | Dropdown / segmented button group |
| `datetime` | `default`, `time` (bool, default `false`), `description` | `YYYY-MM-DD`, `YYYY-MM-DDTHH:MM[:SS]`, or an RFC 3339 timestamp | If `default` set: uses `default`. If no `default`: `422 MissingField` when active. | Date picker (`time: false`) or date-and-time picker (`time: true`) |
| `list` | `default` (a YAML sequence of strings), `description` | JSON array of strings | If `default` set: uses `default`. If no `default`: `422 MissingField` when active. | `list` control (#318 builds the editor; until it lands a screen reports the entry and draws nothing) |

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

#### Scenario: The published set of types carries the list row

- **WHEN** a client reads the set of parameter types this table publishes
- **THEN** it carries `list` alongside `string`, `length`, `integer`, `number`, `boolean`, `enum` and
  `datetime`, each row naming the form control its type is reported with
