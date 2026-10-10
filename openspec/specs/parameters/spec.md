# Parameters

## Purpose

Covers a template's `params:` declarations and types, declared defaults and how they resolve, the request `data` a label may carry and how each value is coerced, what an absent parameter reads as, the parameter list the service publishes for clients, and the placeholder values a thumbnail renders with.

## Requirements

### Requirement: Parameters are declared as a sequence

A template SHALL declare its parameters as a YAML sequence under the top-level `params:` key, each element carrying a required `name` plus the attributes its `type` permits. An omitted `params:` SHALL mean no parameters. A mapping-shaped `params:` SHALL be refused naming the file and `params`. A `name` SHALL be non-empty and match `^[a-zA-Z0-9_-]+$`, and two entries SHALL NOT share a `name`. Every refusal at load in this spec follows the invalid-template rule (`templates`): the file is quarantined while the server still starts, and a write of it is `422 TemplateInvalid` naming the key's path.

Every response carrying a template's `params` (the template list, the template detail, and the create and replace responses) SHALL publish them as one static JSON array in declaration order, `[]` when there are none, whatever any `when:` would select. Each element carries `name`, `type`, `control` (see the type table), the declared attributes, and `default`: a literal default's coerced value, or a tokened default resolved against the response's snapshot (see "Declared defaults"). When that template's snapshot does not resolve, its tokened defaults are omitted and its literal defaults are still published. Only `default` may differ between responses; the array's elements and order do not. `multiline` appears only when true; `time` always appears on a `datetime`.

#### Scenario: Declaration order is the wire order

- **WHEN** a template declares `params:` as `title`, `subtitle`, `code` in that order
- **THEN** both `GET /api/templates` and `GET /api/templates/{id}` publish `params` as `[title, subtitle, code]`

#### Scenario: A gated parameter is still published

- **WHEN** `{subtitle}` is read only under `when: { orientation: horizontal }`
- **THEN** the template detail publishes both `orientation` and `subtitle`

#### Scenario: A mapping-shaped params is refused

- **WHEN** a template file carries `params: { title: { type: string } }`
- **THEN** the file is quarantined with an error naming the file and `params`, and the same content sent to `PUT /api/templates/{id}` is `422 TemplateInvalid`

#### Scenario: A duplicate name is refused

- **WHEN** two entries both declare `name: title`
- **THEN** the template is refused naming the file and `title`

#### Scenario: Defaults are published by value

- **WHEN** a template declares `bold: { type: boolean, default: "false" }`, `copies: { type: integer, default: "3" }` and `url: { type: string, default: "{vars.base}" }`, and the store holds `base = https://ex.co/`
- **THEN** the detail publishes `bold` with `default: false`, `copies` with `default: 3` and `url` with `default: "https://ex.co/"`
- **AND** with no `base` in the store the detail is still `200`, `url` carries no `default`, and `copies` still carries `default: 3`

### Requirement: Parameter types and attributes

A parameter's `type` SHALL be one of the types below. Its entry's keys SHALL be `name`, `type` and the attributes its row lists; any other key is refused naming the parameter and the key.

| Type | Attributes | `control` |
| --- | --- | --- |
| `string` | `default`, `multiline` (bool, default false), `description` | `text`, or `textarea` when `multiline: true` |
| `integer` | `default`, `min`, `max`, `description` | `integer` |
| `number` | `default`, `min`, `max`, `description` | `number` |
| `boolean` | `default`, `description` | `checkbox` |
| `enum` | `values` (required), `default`, `description` | `select` |
| `datetime` | `default`, `time` (bool, default false), `description` | `date`, or `datetime` when `time: true` |
| `list` | `default`, `description` | `list` |

A `string` parameter whose token is the whole `src` of some `image` item (`src: "{photo}"`) SHALL have the control `image`.

An `enum`'s `values` SHALL be non-empty and SHALL contain no blank value. For `integer` and `number`, `min` SHALL NOT exceed `max`. `time` selects the input control only and does not change parsing or printing.

A `list` value is an ordered list of strings, which the service SHALL NOT sort, deduplicate or trim. `[]` is a present, empty list and not an omission.

#### Scenario: An attribute outside the type's row is refused

- **WHEN** a template declares `title: { type: string, time: true }`
- **THEN** the template is refused naming `title` and `time`

#### Scenario: An image source parameter gets the image control

- **WHEN** a template declares `photo: { type: string }` and an `image` item carries `src: "{photo}"`
- **THEN** `photo` is published with `control: image`

### Requirement: Declared defaults

A `boolean` that declares no `default:` SHALL default to `false`.

A default that is not a string containing `{` or `}` is literal. A literal default SHALL be judged at load by the rule for a supplied value of its type (including `values`, `min` and `max`); one that rule refuses, or would treat as an omission, SHALL be refused naming the parameter, and otherwise the coerced value is the default. A `list` default is therefore a YAML sequence of strings, and a token written in one of its elements is literal text.

A string default containing a brace is tokened: it is interpolated with the `vars` and `sys` namespaces only (`interpolation`). Every tokened default SHALL be resolved once per request against the request's snapshot (`interpolation`), whatever the labels carry and whether or not the render reads it, and its value SHALL pass the supplied-value rule for its type. A token that fails, or a value that rule refuses, SHALL fail the request with `422 TemplateInvalid` and reason `reference_unresolved` (`errors`).

#### Scenario: A literal enum default outside values is refused

- **WHEN** a template declares `size: { type: enum, values: [small, large], default: medium }`
- **THEN** the template is refused naming `size` and `medium`

#### Scenario: A literal default its type cannot take is refused

- **WHEN** a template declares `bold: { type: boolean, default: "yes" }`
- **THEN** the template is refused naming `bold`

#### Scenario: A sequence default on a non-list is refused

- **WHEN** a template declares `title: { type: string, default: [A, B] }`
- **THEN** the template is refused naming `title`

#### Scenario: A non-string list element is refused

- **WHEN** a template declares `codes: { type: list, default: [1, true] }`
- **THEN** the template is refused naming `codes` and position 0, while `default: ["1", "true"]` loads

#### Scenario: A token in a list element is literal

- **WHEN** a template declares `tags: { type: list, default: ["{vars.brand}"] }`
- **THEN** a label omitting `tags` prints `{vars.brand}`

#### Scenario: A tokened default naming an absent variable

- **WHEN** a template declares `url: { type: string, default: "{vars.base}" }`, the store holds no `base`, and a render omits `url` while an active item prints `{url}`
- **THEN** the response is `422 TemplateInvalid` with reason `reference_unresolved` and `details.field` `vars.base`

#### Scenario: A broken default fails even when nothing reads it

- **WHEN** the same `url` is read by no active item, or the label supplies `url`
- **THEN** the response is the same `422 TemplateInvalid` with reason `reference_unresolved`

#### Scenario: A tokened default resolving to a refused value

- **WHEN** `size: { type: enum, values: [small, large], default: "{vars.size}" }` is printed, the store holds `size = medium`, and a render omits `size`
- **THEN** the response is `422 TemplateInvalid` with reason `reference_unresolved` and `details.field` `size`

#### Scenario: A datetime default of sys.now is the render date

- **WHEN** a template declares `printed_on: { type: datetime, default: "{sys.now}" }` and a render omits it
- **THEN** `{printed_on}` prints the request's date and `{printed_on:time}` prints `00:00`

### Requirement: Supplied values are coerced by type

A label MAY supply any declared parameter in its `data` map. The service SHALL coerce each supplied value by its type, and SHALL coerce every supplied value before it evaluates any `when:`, so an uncoercible value fails the label even when only an inactive branch reads it.

For every type but `string`, `null` and a string that is empty or only whitespace SHALL be treated as omitted. For a `string`, `null` is omitted and `""` is empty text. Otherwise each type SHALL accept its JSON value or its string form, and nothing else:

| Type | Accepted |
| --- | --- |
| `string` | A JSON string |
| `number` | A JSON number, or a numeric string, trimmed |
| `integer` | A whole JSON number, or an integer string, trimmed |
| `boolean` | `true`/`false`, the strings `true`/`false`/`1`/`0` (trimmed), or the numbers `1`/`0` |
| `enum` | A JSON string that is a member of `values` |
| `datetime` | A string, trimmed: `YYYY-MM-DD` (local midnight), `YYYY-MM-DDTHH:MM[:SS]` (server-local wall clock) or RFC 3339 with an offset or `Z` (converted to server-local time) |
| `list` | A JSON array of JSON strings |

A `number` or `integer` outside its declared `min` or `max` SHALL be refused. Every refusal SHALL be `400 InvalidRequest` with reason `param_value_invalid`, `details.param` naming the parameter and, for a `list` element, `details.element` its position. In a batch each label's refusal is its own per-label failure (`errors`).

A `datetime` local time made ambiguous by a daylight-saving change SHALL resolve to the earlier instant. A nonexistent local time SHALL be refused, except that a date-only value resolves to the first instant that exists on that date, and is refused only when the zone skips the whole date. A number SHALL NOT be read as a datetime. A coerced `datetime` is held as its `%Y-%m-%d` rendering, which is what a bare `{p}` prints and what a `when:` compares.

#### Scenario: Datetime forms

- **WHEN** a render sends `printed_on: "2026-08-19"` for `{printed_on:long_date}`, and another sends `"2026-08-19T14:30"` for `{printed_on:time}`
- **THEN** the labels read `August 19, 2026` and `14:30`

#### Scenario: A date-only value on a day with no local midnight

- **WHEN** a render sends a date whose local midnight is skipped by a daylight-saving change at `00:00`
- **THEN** the value resolves to the first instant that exists on that date

#### Scenario: A nonexistent local date-and-time is refused

- **WHEN** a render sends `printed_on: "2026-09-06T00:30"` in a zone where that local time does not exist
- **THEN** the response is `400 InvalidRequest` with reason `param_value_invalid` naming `printed_on`

#### Scenario: An unparseable or numeric datetime is refused

- **WHEN** a render sends `printed_on: "yesterday"` or `printed_on: 20260819`
- **THEN** the response is `400 InvalidRequest` with reason `param_value_invalid` naming `printed_on`

#### Scenario: Null and blank are omissions except for a string

- **WHEN** a render sends `tags: null` for a list declaring `default: [CONSUMABLE]`, `copies: ""` for an integer declaring `default: 1`, and `title: ""` for a string declaring `default: Untitled`
- **THEN** the label prints `CONSUMABLE`, `1` and an empty title

#### Scenario: A fractional integer is refused, not rounded

- **WHEN** a render sends `copies: 2.7` for an `integer`
- **THEN** the response is `400 InvalidRequest` with reason `param_value_invalid` naming `copies`

#### Scenario: A value outside min and max is refused

- **WHEN** `count: { type: integer, min: 1, max: 10 }` and a render sends `count: 11`
- **THEN** the response is `400 InvalidRequest` with reason `param_value_invalid` naming `count`

#### Scenario: A list element that is not a string is refused

- **WHEN** a render sends `codes: [1, true]` for a declared `list`
- **THEN** the response is `400 InvalidRequest` with reason `param_value_invalid`, `details.param` `codes` and `details.element` 0

#### Scenario: A non-string for a string is refused

- **WHEN** a render sends `title: ["A", "B"]` or `title: 5` for a `string` parameter
- **THEN** the response is `400 InvalidRequest` with reason `param_value_invalid` naming `title`

#### Scenario: An enum value outside values

- **WHEN** `orientation` declares `values: [horizontal, vertical]` and a render sends `orientation: "sideways"`
- **THEN** the response is `400 InvalidRequest` with reason `param_value_invalid` naming `orientation`

### Requirement: Data keys name declared parameters

On `POST /api/render/label`, `POST /api/render` and `POST /api/print`, every key of a label's `data` SHALL name a parameter the template declares. Any declared parameter is legal, whether or not an active item reads it. A label carrying one or more undeclared keys SHALL fail with `InvalidRequest` and reason `data_key_unknown`. One failure is raised per label, and its message names every undeclared key, sorted ascending by code point, together with the template id. On the single-label path this is a `400`. On the batch paths each such label is a per-label failure under `422 BatchInvalid`, every label is checked, and nothing is produced.

#### Scenario: A single render refuses an undeclared key

- **WHEN** `POST /api/render/label` sends `{"template": "shelf", "data": {"title": "Bolts", "sku_legacy": "X-1"}}` and `shelf` declares only `title`
- **THEN** the response is `400 InvalidRequest` with reason `data_key_unknown`, and the message names `sku_legacy` and `shelf`

#### Scenario: Several undeclared keys are named in order

- **WHEN** a label carries undeclared `zeta`, `alpha` and `mid`
- **THEN** one failure names `alpha`, `mid`, `zeta` in that order

#### Scenario: A batch reports every offending label

- **WHEN** a `POST /api/render` of three labels carries undeclared keys on labels 0 and 2
- **THEN** the response is `422 BatchInvalid` with entries for index 0 and 2, each carrying `InvalidRequest` and `data_key_unknown`

### Requirement: A value comes from the request or the declared default

The value a token reads for a declared parameter SHALL come from the label's `data`, or failing that from its default, and from nothing else. A parameter that neither supplies is absent: an `enum` is not its first value, and a `datetime` is not the render instant. A `boolean` is never absent, because it always has a default.

An absent parameter SHALL read as empty: a token naming it prints nothing, with or without a reader (`interpolation`), and an active container's `repeat:` over it draws no instance (`layout`). An absent parameter in a `when:` makes that predicate false (`layout`). A parameter reference from a layout attribute is never absent, because its parameter declares a default (see "Parameter references from layout attributes").

#### Scenario: An omitted boolean is false

- **WHEN** `bold: { type: boolean }` declares no default, a container carries `when: { bold: false }`, and the render omits `bold`
- **THEN** the container renders

#### Scenario: An omitted enum with no default gates off a branch

- **WHEN** `outline: { type: enum, values: [yes] }` gates a container with `when: { outline: yes }` and the render omits `outline`
- **THEN** the label renders without that container

#### Scenario: An omitted list draws no instance

- **WHEN** `tags: { type: list }` with no default is named by an active container's `repeat:` and the render omits `tags`
- **THEN** the label renders without any instance of that container

#### Scenario: An omitted string prints nothing

- **WHEN** `caption: { type: string }` declares no default, an active `text` prints `"[{caption}]"`, and the render omits `caption`
- **THEN** the text reads `[]`

### Requirement: Parameter references from layout attributes

A layout or `format` attribute that takes a parameter reference SHALL write it as exactly `"{name}"`: the whole string is one bare token of the interpolation grammar, with no reader and no whitespace inside the braces. The reference SHALL name a declared parameter of an allowed type that declares a `default`, checked at load. A refusal names the parameter and the context, and the rule holds inside a `repeat:` subtree as well:

| Attribute | Allowed types |
| --- | --- |
| `format` `width` (and its `min`/`max`) and `height`, item `size` | `number`, `integer` |
| `font_weight` | `integer` |

A value a label supplies for a parameter that a `font_weight` references SHALL be a weight `text` accepts as a literal, else it is refused as `400 InvalidRequest` with reason `param_value_invalid` naming the parameter.

#### Scenario: A list cannot drive a dimension

- **WHEN** `tags: { type: list }` is referenced as a `format` width
- **THEN** the template is refused naming `tags` and the context

#### Scenario: A referenced parameter needs a default

- **WHEN** an item carries `size: ["{w}", 10]` and `w: { type: number }` declares no default
- **THEN** the template is refused naming `w` and the item's `size`

#### Scenario: A spaced reference is not a reference

- **WHEN** an item carries `size: ["{ w }", 10]` for a declared `number` `w`
- **THEN** the template is refused naming the item's `size`

#### Scenario: A supplied weight must be a literal weight

- **WHEN** a `text` item carries `font_weight: "{weight}"` and a render sends `weight: 450`
- **THEN** the response is `400 InvalidRequest` with reason `param_value_invalid` naming `weight`

### Requirement: Placeholder values

A thumbnail SHALL render with no `data`, every declared parameter taking its default as a render resolves it, and every parameter without a default taking a placeholder by type:

| Type | Placeholder |
| --- | --- |
| `string` | Its name; a sample PNG for the `image` control |
| `list` | `[<name>]` |
| `number`, `integer` | `42`, clamped into `[min, max]`; for an `integer`, into the whole numbers that range admits |
| `enum` | The first of `values` |
| `datetime` | The request's captured instant |

Gates are evaluated against these values. A default that fails SHALL fail the thumbnail as it fails a render.

#### Scenario: An undefaulted datetime prints the current date

- **WHEN** a template prints `{printed_on:short_date}` and `printed_on` declares no default
- **THEN** the thumbnail shows the current date in that format

#### Scenario: A declared default is shown over a placeholder

- **WHEN** a template prints `{orientation}`, which declares `values: [horizontal, vertical]` and `default: vertical`
- **THEN** the thumbnail prints `vertical`, and prints `horizontal` when no default is declared

#### Scenario: A number placeholder respects its bounds

- **WHEN** a template prints `{qty}` for `qty: { type: integer, min: 5, max: 10 }` with no default
- **THEN** the thumbnail prints `10`

#### Scenario: A broken default fails the thumbnail

- **WHEN** `title: { type: string, default: "{vars.base}" }` is printed and the store holds no `base`
- **THEN** the thumbnail fails with `422 TemplateInvalid` reason `reference_unresolved` and `details.field` `vars.base`
