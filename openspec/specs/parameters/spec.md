# Parameters

## Purpose

Covers a template's `params:` declarations and types, declared defaults and how they resolve, the request `data` a label may carry and how each value is coerced, when a parameter is required, and the input lists and default reports the service publishes for clients.

## Requirements

### Requirement: Parameters are declared as a sequence

A template SHALL declare its parameters as a YAML sequence under the top-level `params:` key, each element carrying a required `name` plus the attributes its `type` permits. An omitted `params:` SHALL mean no parameters. `params: null` and a mapping-shaped `params:` SHALL be refused naming the file and `params`. A `name` SHALL be non-empty and match `^[a-zA-Z0-9_-]+$`, and two entries SHALL NOT share a `name`.

Every response carrying a template's `params` (the template list, the template detail, and create, replace and move responses) SHALL publish them as a JSON array in declaration order, `[]` when there are none. Each element carries `name`, `type`, the declared attributes, and `default` exactly as declared (not resolved). `multiline` appears only when true; `time` always appears on a `datetime`.

Where more than one declaration fails at load, or more than one supplied value fails at render, the first error reported SHALL be for the declaration-order first parameter.

Every refusal in this and the next two requirements quarantines the file at load while the server still starts, and refuses a template write with `422 TemplateInvalid`. The `details.reason` is `template_parse_failed` for the `params` shape, duplicate names, `format`, `time` and the attributes `datetime` and `list` forbid, a sequence default on a non-list, and a malformed list default. It is `template_validation_failed` for every other refusal.

#### Scenario: Declaration order is the wire order

- **WHEN** a template declares `params:` as `title`, `subtitle`, `code` in that order
- **THEN** both `GET /api/templates` and `GET /api/templates/{id}` publish `params` as `[title, subtitle, code]`

#### Scenario: A mapping-shaped params is refused

- **WHEN** a template file carries `params: { title: { type: string } }`
- **THEN** the file is quarantined with an error naming the file and `params`, and the same content sent to `PUT /api/templates/{id}` is `422 TemplateInvalid` with reason `template_parse_failed`

#### Scenario: A duplicate name is refused

- **WHEN** two entries both declare `name: title`
- **THEN** the template is refused naming the file and `title`

#### Scenario: The first error follows declaration order

- **WHEN** `params:` declares `zebra` then `alpha`, both with a `format:` attribute
- **THEN** the error names `zebra`

### Requirement: Parameter types and attributes

A parameter's `type` SHALL be one of the types below, and SHALL accept the attributes its row lists:

| Type | Attributes | Input control |
| --- | --- | --- |
| `string` | `default`, `multiline` (bool, default false), `description` | `text`, or `textarea` when `multiline: true` |
| `length` | `default`, `min`, `max`, `description` | `number`, with the template `unit` |
| `integer` | `default`, `min`, `max`, `description` | `integer` |
| `number` | `default`, `min`, `max`, `description` | `number` |
| `boolean` | `default`, `description` | `checkbox` |
| `enum` | `values` (required), `default`, `description` | `select` |
| `datetime` | `default`, `time` (bool, default false), `description` | `date`, or `datetime` when `time: true` |
| `list` | `default`, `description` | `list` |

`format` SHALL be refused on every type. On a `datetime`, `min`, `max`, `multiline` and `values` SHALL be refused. On a `list`, `min`, `max`, `multiline`, `values` and `time` SHALL be refused. `time` SHALL be refused on every type but `datetime`. Each refusal names the parameter and the attribute, and it applies when the key is written at all, explicit YAML null included. On the other six types, an attribute outside the type's row is accepted and has no effect.

An `enum`'s `values` SHALL be non-empty and SHALL contain no blank value. For `length`, `integer` and `number`, `min` SHALL NOT exceed `max`. `min` and `max` describe the input control and are not enforced on supplied values. `time` selects the input control only and does not change parsing or printing.

A `list` value is an ordered list of strings, which the service SHALL NOT sort, deduplicate or trim. `[]` is a present, empty list and not an omission.

#### Scenario: A format attribute is refused

- **WHEN** a template declares `printed_on: { type: datetime, format: long_date }`
- **THEN** the template is refused with a message naming `printed_on` and `format`

#### Scenario: An explicitly null forbidden attribute is refused

- **WHEN** a `list` parameter `tags` writes `multiline:` with no value
- **THEN** the template is refused naming `tags` and `multiline`

#### Scenario: An explicitly null time flag is refused

- **WHEN** a `datetime` parameter writes `time:` with no value
- **THEN** the template is refused naming the parameter and `time`, rather than loading with `time` false

#### Scenario: The time flag is refused on another type

- **WHEN** a template declares `title: { type: string, time: true }`
- **THEN** the template is refused naming `title` and `time`

### Requirement: Declared defaults

A `default:` written with an explicit YAML null SHALL mean no default, on every type. The shape of a declared default SHALL be checked at load:

| Type | Admissible `default:` |
| --- | --- |
| `list` | A YAML sequence whose every element is a YAML string. Any other value is refused naming the parameter, and a non-string element is refused naming its position. |
| `datetime` | A string. |
| `enum` | A string. |
| any other | Anything but a sequence, which is refused naming the parameter. |

A string default SHALL be interpolated (`interpolation`), restricted to namespaced tokens: a bare `{name}` token in a default is refused at load, and so is malformed brace syntax. A non-string default carries no token and SHALL be used as written, so a token written as a `list` element is literal text.

A literal `enum` default, meaning one containing neither `{` nor `}`, SHALL be in `values`, checked at load. A default containing either brace SHALL NOT have its value checked at load. It is checked when it resolves.

#### Scenario: A literal enum default outside values is refused

- **WHEN** a template declares `size: { type: enum, values: [small, large], default: medium }`
- **THEN** the template is refused naming `size` and `medium`

#### Scenario: A tokened enum default is checked when it resolves

- **WHEN** the default is `"{vars.size}"` and the store holds `size = medium`
- **THEN** the template loads, and a render omitting `size` fails with `param_default_unresolvable`

#### Scenario: A sequence default on a non-list is refused

- **WHEN** a template declares `title: { type: string, default: [A, B] }`
- **THEN** the template is refused naming `title`

#### Scenario: A non-string list element is refused

- **WHEN** a template declares `codes: { type: list, default: [1, true] }`
- **THEN** the template is refused naming `codes` and position 0, while `default: ["1", "true"]` loads

#### Scenario: A token in a list element is literal

- **WHEN** a template declares `tags: { type: list, default: ["{vars.brand}"] }`
- **THEN** a label omitting `tags` prints `{vars.brand}`

### Requirement: A declared default resolves against one request snapshot

Each request SHALL capture one instant and read the variables store and the effective `datetime_formats` once. Every default it resolves SHALL be resolved against that snapshot, so no two labels of one batch, sheet, ZIP or print job see a default resolve differently. A resolved default SHALL then be coerced by the rule that coerces a supplied value of its type. A YAML integer written as the default of a non-`integer` type is held as a number, so `default: 1` on a `boolean` fails to resolve, and `default: true` is the boolean spelling.

When a render omits a parameter, its declared default SHALL be resolved whether or not any active item reads that parameter. Resolution fails when a token names nothing or when the resolved value fails coercion. Either failure SHALL be `422 TemplateInvalid` with `details.reason` `param_default_unresolvable`, and SHALL NOT be `MissingField`. Its `details` carries `param`, plus `token` when a token failed or `value` when a resolved value was refused, but never both. The message names the parameter and that token or value. In a batch or print job, every label reaching the failure SHALL be a per-label failure with that code and reason, and nothing is produced. A label that supplies the parameter never reaches its default.

A read-only path reports the same failure as `{ reason, message, token?, value? }`, the same strings without `param` (see the template detail and inputs requirements).

#### Scenario: A default naming an absent variable

- **WHEN** a template declares `url: { type: string, default: "{vars.base}" }`, the store holds no `base`, and a render omits `url`
- **THEN** the response is `422 TemplateInvalid` with reason `param_default_unresolvable`, `details.param` `url` and `details.token` `vars.base`

#### Scenario: A literal default a request could not send

- **WHEN** a template declares `bold: { type: boolean, default: "yes" }` and a render omits `bold`
- **THEN** the response is `422 TemplateInvalid` with reason `param_default_unresolvable`, `details.param` `bold` and `details.value` `yes`

#### Scenario: A broken default nothing reads still fails the render

- **WHEN** a declared parameter that no item, gate or attribute reads has a default naming an absent variable, and a render omits it
- **THEN** the render fails with `param_default_unresolvable` naming that parameter

#### Scenario: A batch names every label reaching the broken default

- **WHEN** a batch of three labels all omit such a parameter
- **THEN** the response is `422 BatchInvalid` with three failure entries carrying `TemplateInvalid` and `param_default_unresolvable`, and no artifact is produced

#### Scenario: One batch resolves one instant

- **WHEN** a batch of labels omits a parameter declaring `default: "{sys.now}"` and the run crosses midnight
- **THEN** every label prints the same date

#### Scenario: A datetime default of sys.now is the render date

- **WHEN** a template declares `printed_on: { type: datetime, default: "{sys.now}" }` and a render omits it
- **THEN** `{printed_on}` prints the request's date and `{printed_on:time}` prints `00:00`

### Requirement: Supplied values are coerced by type

A label MAY supply any declared parameter in its `data` map. The service SHALL coerce each supplied value by its type, and SHALL coerce every supplied value before it evaluates any `when:`, so an uncoercible value fails the label even when only an inactive branch reads it.

| Type | Accepted | Refusal |
| --- | --- | --- |
| `string` | A JSON string. Any other non-array value is stringified, with `null` as the empty string. | An array: `400 InvalidRequest`, `request_body_invalid` |
| `length`, `number` | A JSON number, or a numeric string, trimmed, with an optional `mm` or `in` suffix stripped. The suffix converts nothing. | `400 InvalidRequest`, `request_body_invalid` |
| `integer` | A JSON number, rounded to the nearest whole number, or an integer string, trimmed | `400 InvalidRequest`, `request_body_invalid` |
| `boolean` | `true`/`false`, the strings `true`/`false`/`1`/`0` (trimmed), or the numbers `1`/`0` | `400 InvalidRequest`, `request_body_invalid` |
| `enum` | A string, other scalars stringified, that is a member of `values` | `422 InvalidEnumValue` |
| `datetime` | A string, trimmed: `YYYY-MM-DD` (local midnight), `YYYY-MM-DDTHH:MM[:SS]` (server-local wall clock) or RFC 3339 with an offset or `Z` (converted to server-local time) | `400 InvalidRequest`, `datetime_param_invalid` |
| `list` | A JSON array of JSON strings | `400 InvalidRequest`, `request_body_invalid` |

Each `400` message names the parameter, and a `list` element refusal also names the element's position. `InvalidEnumValue` carries the message `Invalid option selection` and `details` of exactly `selection` (`{ name: supplied value }`) and `allowed` (`{ name: values in declared order }`), with no `reason`.

A `datetime` sent as `null`, as the empty string or as whitespace, and a `list` sent as `null`, SHALL be treated as omitted. On every other type, `null` is a value. A `datetime` local time made ambiguous by a daylight-saving change SHALL resolve to the earlier instant. A nonexistent local time SHALL be refused, except that a date-only value resolves to the first instant that exists on that date, and is refused only when the zone skips the whole date. A number SHALL NOT be read as a datetime.

A coerced `datetime` is held as its `%Y-%m-%d` rendering, which is what a bare `{p}` prints and what a `when:` compares. A coerced `length` such as `"80mm"` is held as `80`. In a batch or print job, each label's refusal SHALL be its own per-label failure carrying that code and reason (`errors`). A CSV data cell reaches coercion as its string, so an empty cell is `""` and not an omission.

#### Scenario: Datetime forms

- **WHEN** a render sends `printed_on: "2026-08-19"` for `{printed_on:long_date}`, and another sends `"2026-08-19T14:30"` for `{printed_on:time}`
- **THEN** the labels read `August 19, 2026` and `14:30`

#### Scenario: A date-only value on a day with no local midnight

- **WHEN** a render sends a date whose local midnight is skipped by a daylight-saving change at `00:00`
- **THEN** the value resolves to the first instant that exists on that date

#### Scenario: A nonexistent local date-and-time is refused

- **WHEN** a render sends `printed_on: "2026-09-06T00:30"` in a zone where that local time does not exist
- **THEN** the response is `400 InvalidRequest` with reason `datetime_param_invalid`

#### Scenario: An unparseable or numeric datetime is refused

- **WHEN** a render sends `printed_on: "yesterday"` or `printed_on: 20260819`
- **THEN** the response is `400 InvalidRequest` with reason `datetime_param_invalid`, naming `printed_on`

#### Scenario: A null datetime or list is an omission

- **WHEN** a render sends `tags: null` for a list declaring `default: [CONSUMABLE]`
- **THEN** the label prints `CONSUMABLE`, while `tags: []` prints nothing

#### Scenario: A list element that is not a string is refused

- **WHEN** a render sends `codes: [1, true]` for a declared `list`
- **THEN** the response is `400 InvalidRequest` with reason `request_body_invalid`, naming `codes` and position 0

#### Scenario: An array for a string is refused

- **WHEN** a render sends `title: ["A", "B"]` for a `string` parameter
- **THEN** the response is `400 InvalidRequest` with reason `request_body_invalid`, naming `title`

#### Scenario: An enum value outside values

- **WHEN** `orientation` declares `values: [horizontal, vertical]` and a render sends `orientation: "sideways"`
- **THEN** the response is `422 InvalidEnumValue` with message `Invalid option selection`, `details.selection` `{ "orientation": "sideways" }` and `details.allowed` `{ "orientation": ["horizontal", "vertical"] }`

#### Scenario: A blank CSV cell for an enum

- **WHEN** a CSV import leaves an `enum` column empty on one row
- **THEN** that row is a per-label `InvalidEnumValue` failure under `422 BatchInvalid`

### Requirement: Data keys name declared parameters

On `POST /api/render/label`, `POST /api/batch` and `POST /api/print`, every key of a label's `data` SHALL name a parameter the template declares. Any declared parameter is legal, whether or not an active item reads it. A label carrying one or more undeclared keys SHALL fail with `InvalidRequest` and reason `data_key_unknown`. One failure is raised per label, and its message names every undeclared key, sorted ascending by code point, together with the template id. On the single-label path this is a `400`. On the batch and print paths each such label is a per-label failure under `422 BatchInvalid`, every label is checked, and nothing is produced.

Within a label, this check SHALL come before resolution, so it replaces any coercion, omission, default or render failure the same label would report. Request-level refusals, such as query parameters and batch admission, are reported ahead of it. `POST /api/templates/{id}/inputs` ignores undeclared keys.

On `POST /api/import/csv`, a header column naming no declared parameter SHALL fail the whole request with `400 InvalidRequest` and reason `csv_data_column_unknown`, before any label is built and in both modes. The message names every such column, sorted ascending, and the template id. The file's own header, row and empty-file refusals (`rendering`) are reported ahead of it.

#### Scenario: A single render refuses an undeclared key

- **WHEN** `POST /api/render/label` sends `{"template": "shelf", "data": {"title": "Bolts", "sku_legacy": "X-1"}}` and `shelf` declares only `title`
- **THEN** the response is `400 InvalidRequest` with reason `data_key_unknown`, and the message names `sku_legacy` and `shelf`

#### Scenario: Several undeclared keys are named in order

- **WHEN** a label carries undeclared `zeta`, `alpha` and `mid`
- **THEN** one failure names `alpha`, `mid`, `zeta` in that order

#### Scenario: A batch reports every offending label

- **WHEN** a batch of three labels carries undeclared keys on labels 0 and 2
- **THEN** the response is `422 BatchInvalid` with entries for index 0 and 2, each carrying `InvalidRequest` and `data_key_unknown`

#### Scenario: The key check wins within a label

- **WHEN** a label carries undeclared `sku_legacy` and an uncoercible value for a declared `integer`
- **THEN** the label reports `data_key_unknown`

#### Scenario: A CSV column naming no parameter

- **WHEN** `POST /api/import/csv?template=shelf` receives the header `title,zeta,alpha` and `shelf` declares only `title`
- **THEN** the response is `400 InvalidRequest` with reason `csv_data_column_unknown`, naming `alpha` then `zeta` and `shelf`, and nothing is rendered or printed

### Requirement: A value comes from the request or the declared default

The value a token reads for a declared parameter SHALL come from the label's `data`, or failing that from its resolved declared default, and from nothing else. A parameter that neither supplies is absent: a `boolean` is not `false`, an `enum` is not its first value, and a `datetime` is not the render instant.

An absent parameter read by an active item's token, or by an active container's `repeat:`, SHALL be `422 MissingField` naming it. A parameter read only by inactive items SHALL NOT be required. An absent parameter in a `when:` makes that predicate false (`layout`). A dimension `ref` resolves its parameter under the `layout` rules, not this one.

#### Scenario: An omitted boolean with no default fails

- **WHEN** `bold: { type: boolean }` is read by an active `text` item and the render omits `bold`
- **THEN** the response is `422 MissingField` naming `bold`

#### Scenario: An omitted enum with no default gates off a branch

- **WHEN** `outline: { type: enum, values: [yes] }` gates a container with `when: { outline: yes }` and the render omits `outline`
- **THEN** the label renders without that container

#### Scenario: An omitted list a container repeats fails

- **WHEN** `tags: { type: list }` with no default is named by an active container's `repeat:` and the render omits `tags`
- **THEN** the response is `422 MissingField` naming `tags`

#### Scenario: A parameter only an inactive branch reads is not required

- **WHEN** only an inactive container's `text` reads `{caption}` and the render omits `caption`
- **THEN** the label renders

### Requirement: Parameter references from layout attributes are type-checked

A layout or `format` attribute that names a parameter directly SHALL name a declared parameter of an allowed type, checked at load. A refusal names the parameter and the context, and the rule holds inside a `repeat:` subtree as well:

| Attribute | Allowed types |
| --- | --- |
| `format` `width`/`height` (and their `min`/`max`), item `width`/`height` | `length`, `number`, `integer` |
| `font_weight` | `integer` |
| `line_spacing` | `number`, `integer` |
| `color`, stroke `color`, `background` | `string`, `enum` |
| `image` `name:` | `string` |

#### Scenario: A list cannot drive a dimension

- **WHEN** `tags: { type: list }` is referenced as a `format` width
- **THEN** the template is refused naming `tags` and the context

#### Scenario: A list cannot bind an image

- **WHEN** an `image` item carries `name: "tags"` for `tags: { type: list }`
- **THEN** the template is refused naming `tags` and `image name`, and a write of it is `422 TemplateInvalid`

### Requirement: An input entry describes one control

An input list SHALL be an array of entries, one per declared parameter that the label's render reads, ordered by declaration order. Each entry has these fields:

| Field | Meaning |
| --- | --- |
| `name` | The parameter. |
| `control` | From the type table, except `image` for a `string` that an active `image` item binds through `name:`. |
| `slider` | True when a `length`, `integer` or `number` declares both `min` and `max`. |
| `required` | False exactly when `default` is present. |
| `default` | The declared default as resolved for this request, after coercion (`"80mm"` gives `80`, and a `datetime` gives `YYYY-MM-DD`). Absent when none is declared or resolution failed. |
| `default_error` | `{ reason, message, token?, value? }` when the declared default failed to resolve. Absent otherwise. |
| `values` | On `select`, the declared values in order. |
| `min`, `max` | On `length`, `integer` and `number`, when declared. |
| `unit` | On `length`, the template's unit. |
| `description` | When declared. |
| `interpolated` | True when an active item reads the name as a value: a `text` or `qr` token, an `image` `name:`, a token in `image` `src`, or a `repeat:`. |
| `truncated_elsewhere` | True when any `wrap: false` `text` item anywhere in the template reads the name. |

A parameter is read by a token, an `image` `name:` or `src`, a `repeat:`, a `when:` key, a `format` dimension `ref`, or an item attribute `ref`. A `when:` key is reported for every item the walk reaches, whether or not its condition holds. Items inside an inactive container are not reached. A `repeat:` subtree is walked once per element of the label's list, so a repeat over an absent or empty list contributes only its own name. `{vars.*}` and `{sys.*}` are never entries, and a parameter nothing reads has none.

#### Scenario: A gated field is absent and its gate is present

- **WHEN** `{subtitle}` is read only under `when: { orientation: horizontal }` and the label selects `vertical`
- **THEN** the list holds `orientation`, with control `select` and `interpolated` false, and no `subtitle`

#### Scenario: A tokened default is published resolved

- **WHEN** `url: { type: string, default: "{vars.base}" }` and the store holds `base = https://example.test`
- **THEN** the `url` entry carries `default: "https://example.test"` and `required: false`

#### Scenario: A broken default publishes its diagnostic

- **WHEN** the store holds no `base`
- **THEN** the `url` entry carries no `default`, `required: true`, and `default_error` with reason `param_default_unresolvable` and `token` `vars.base`

#### Scenario: An undefaulted boolean is required

- **WHEN** `bold: { type: boolean }` is read by an active item
- **THEN** its entry carries `control: checkbox`, `required: true` and no `default`

#### Scenario: A repeat is an interpolated read

- **WHEN** `tags: { type: list }` is named by a container's `repeat:` whose only child prints fixed text
- **THEN** the list holds `tags` with control `list`, `required: true` and `interpolated: true`

### Requirement: The inputs endpoint computes a list per label

`POST /api/templates/{id}/inputs` SHALL accept `{ "labels": [ { "data": { ... } } ] }` and return `200` with `{ "inputs": [ [entry, ...], ... ] }`, one list per label in order. A label carrying any key besides `data` SHALL be `400 InvalidRequest` with reason `json_malformed`. An empty `labels` array returns `{ "inputs": [] }`. More than 500 labels are refused as `POST /api/batch` refuses them. An unknown id is `404 TemplateNotFound`.

Entries are decided by the render's rule, so an item whose `when:` fails reads nothing. Resolution is lenient here, and SHALL differ from a render in three ways only. A supplied value that fails coercion is treated as omitted. A key naming no declared parameter is ignored. A declared default that fails to resolve leaves the parameter absent, so its gates are false, while its entry reports `default_error`. The endpoint SHALL NOT refuse a label for its content. `required` does not depend on the label's values.

#### Scenario: Two labels, two branches

- **WHEN** one label selects `orientation: horizontal` and another `vertical`
- **THEN** two lists come back in that order, each holding the names its branch reads

#### Scenario: An uncoercible value falls back to the default

- **WHEN** a label carries `copies: "abc"` for an `integer` declaring `default: 1`
- **THEN** the response is `200`, `copies` carries `default: 1`, and gates on `copies` see `1`, while the same label sent to a render is `400 InvalidRequest`

#### Scenario: A broken default answers rather than fails

- **WHEN** `mode: { type: string, default: "{vars.mode}" }` gates a container on `mode: full` and the store holds no `mode`
- **THEN** the response is `200`, the container's names are absent, and `mode` carries `required: true` and `default_error`, while a render omitting `mode` is `422 TemplateInvalid`

#### Scenario: An undeclared key is ignored

- **WHEN** a label carries `{ "title": "Bolts", "sku_legacy": "X-1" }` and only `title` is declared
- **THEN** the response is `200` and the list equals the one for the label without `sku_legacy`

#### Scenario: A label envelope key is refused

- **WHEN** a label carries `{ "data": {}, "option": { "x": "1" } }`
- **THEN** the response is `400 InvalidRequest` with reason `json_malformed`, and `details.error` names `option`

### Requirement: The template detail reports inputs and resolved defaults

Every response carrying a template-detail body (`GET /api/templates/{id}` and the create, replace and move responses) SHALL include:

- `inputs.default`: the input list for a label carrying no `data`;
- `inputs.all`: the union over every branch, ignoring every `when:` and walking each `repeat:` subtree once, with one entry per name and the same fields, including `image` when any branch binds the name;
- `variables`: the `{vars.<key>}` keys the layout reads, without the prefix, ascending;
- `param_defaults`: one key per parameter that declares a `default:`, and none for any other, each holding either `{ "resolved": <value> }` or `{ "error": { reason, message, token?, value? } }`.

The response SHALL be `200` whether or not every default resolves. Within one response, `param_defaults` and both input lists SHALL report one resolution, and the detail and the inputs endpoint SHALL agree for one snapshot. A failure to read the variables store or the `datetime_formats` is `500 Internal`. A write path reads them before it changes any file.

#### Scenario: The union holds every branch

- **WHEN** `{subtitle}` is read only under `orientation: horizontal` and `{tracking_url}` only under `vertical`, and `orientation` declares no default
- **THEN** `inputs.all` holds both, and `inputs.default` holds neither

#### Scenario: The union holds a repeated subtree

- **WHEN** a container with `repeat: tags` prints `{tags}` and `{price}`, and `tags` declares no default
- **THEN** `inputs.all` holds `tags` and `price`, and `inputs.default` holds `tags` alone

#### Scenario: Every declared default is reported

- **WHEN** `title` declares a default, `subtitle` none, and `mode` a broken default that nothing reads
- **THEN** `param_defaults` holds `title` with `resolved` and `mode` with `error`, and has no `subtitle` key

#### Scenario: The report is post-coercion

- **WHEN** `width: { type: length, default: "80mm" }` and `bold: { type: boolean, default: "yes" }`
- **THEN** `param_defaults.width.resolved` is `80`, and `param_defaults.bold.error.value` is `yes`

#### Scenario: A store failure refuses a write

- **WHEN** the variables store cannot be read during a `POST` or `PUT` of a template
- **THEN** the response is `500 Internal`, and no file has been written, moved or replaced

### Requirement: Thumbnail placeholder values

A thumbnail SHALL render from placeholder data built from `inputs.all`. It invents a value only for an entry that is both `interpolated` and `required`, and for a `select` only when the parameter declares no `default:`. The invented value depends on `control`:

| Control | Placeholder |
| --- | --- |
| `text`, `textarea` | The entry's name. This includes a name read through `image` `src`. |
| `image` | A 1×1 PNG data URI |
| `integer` | `min` truncated to a whole number, else `1` |
| `number` | `min`, else `1` |
| `checkbox` | `false` |
| `date`, `datetime` | The request's captured instant |
| `list` | `[<name>]` |
| `select` | The first of `values` |

Every other parameter SHALL resolve as a render resolves it. Gates are evaluated against the placeholder data. A broken default is therefore masked on every control but `select`, where the thumbnail fails with `param_default_unresolvable`.

#### Scenario: An undefaulted datetime prints the current date

- **WHEN** a template prints `{printed_on:short_date}` and `printed_on` declares no default
- **THEN** the thumbnail shows the current date in that format

#### Scenario: A declared enum default is shown

- **WHEN** a template prints `{orientation}`, which declares `values: [horizontal, vertical]` and `default: vertical`
- **THEN** the thumbnail prints `vertical`

#### Scenario: An undefaulted enum shows its first value

- **WHEN** the same `orientation` declares no default
- **THEN** the thumbnail prints `horizontal`

#### Scenario: A broken enum default fails the thumbnail

- **WHEN** `orientation` declares `default: "{vars.orient}"`, an active item prints it, and the store holds no `orient`
- **THEN** the thumbnail is `422 TemplateInvalid` with reason `param_default_unresolvable` naming `orientation`

#### Scenario: A broken string default is stood in for

- **WHEN** `title: { type: string, default: "{vars.base}" }` is printed and the store holds no `base`
- **THEN** the thumbnail prints `title`

#### Scenario: An enum only a gate names stays absent

- **WHEN** `outline: { type: enum, values: [yes] }` with no default only gates a container
- **THEN** the thumbnail renders without that container

#### Scenario: A placeholder that opens a gate fills its branch

- **WHEN** a required `integer` `copies` with no `min` is printed, and a container gated on `copies: 1` prints `{subtitle}`
- **THEN** the thumbnail fills `copies` with `1` and `subtitle` with `subtitle`, and the gated container renders
