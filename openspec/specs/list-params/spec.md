# list-params Specification

## Purpose
Defines the `list` parameter type: how a template declares a parameter holding more than one value, which
attributes such a parameter refuses, what a request may send for it, and where in a template a list may
not be used. What a label prints from one is the `interpolation-tokens` capability's `join` reader.

## Requirements

### Requirement: A `list` parameter holds an ordered list of strings

A `params:` entry MAY declare `type: list`. Its value is an **ordered list of strings**. Order is the
author's and the caller's, and the service SHALL NOT sort, deduplicate or trim it.

A `list` parameter accepts exactly two other attributes:

- `default`: a YAML **sequence**, resolved by the same rules as every other type (`param-resolution`).
- `description`: string, as on every other parameter type.

`min`, `max`, `multiline`, `values`, `format` and `time` SHALL be refused at load on a `list` parameter,
with a validation message naming both the parameter and the offending attribute. The refusal SHALL turn
on the key being **written**, whatever it carries, an explicit YAML null included, exactly as
`datetime-params` refuses its own forbidden set. `enum:` is not an attribute of any parameter type and is
refused before this list is reached, likewise unchanged.

There is no element-type key and no `values`. The only consumer of a list is a join into text, so a typed
element would buy input validation and nothing at render; an author wanting numbers writes
`["1", "2"]`, which joins identically. Element typing and a per-element value set are a later question,
not a foreclosed one.

**A `default:` is a sequence of strings.** A `default:` that is a non-null YAML scalar or a mapping SHALL
be refused at load, naming the parameter. A YAML **null** is excluded from that rule and is not a
sequence either: it means the key was written and left empty, which is an absent default on every type,
under the paragraph below. Each element SHALL be a YAML **string** scalar. An element that is
a number, a boolean, a null, a sequence or a mapping SHALL be refused at load, naming the parameter and
the element's position.

The service SHALL NOT coerce an element. A list holds strings, so `default: [1, 2]` names a value the
type does not have, and stringifying it would make the declaration mean something the author did not
write. Quoting is the whole of what an author does about it: `default: ["1", "2"]` loads, and joins
exactly as `[1, 2]` would have. The same rule governs a request value below, which is what
`param-resolution` requires of every type: a default may not carry a value the request could not have
carried, and here neither may carry a non-string.

A list `default:` is not a string, so it carries no interpolation token and is used as written
(`interpolation-tokens`). `{vars.x}` written as an element is that literal text, and no brace-syntax
check applies to it.

`default:` written with an explicit YAML null is an **absent** default, exactly as it is on every other
type, and is not the empty list.

**`[]` is present and empty, not absent.** A list of zero elements is a list. The service distinguishes
presence from value everywhere, and folding `[]` into omission would make an empty tag set a `422` and
collapse it with a key nobody sent.

A `list` parameter named `p` claims the interpolation token `{p:join('<sep>')}` and no other
(`interpolation-tokens`). Parameter naming is governed by that capability, which owns the rule; this
requirement adds nothing to it.

A rejected declaration quarantines the template file under the existing rules of the `template-registry`
capability and SHALL NOT abort startup, and the same content arriving through a template write SHALL be
refused with `422 TemplateInvalid` and `details.reason` `template_parse_failed`.

Every refusal this requirement defines is decided in the raw-to-domain conversion (`src/convert.rs`),
which runs inside `parse_template` before `validate()` is reached, so `template_parse_failed` is the
reason the service reports. That is also what the corpus already publishes for this stage:
`conditional-visibility` and `template-groups` both name it for conversion-stage refusals. It sits
awkwardly with `param-resolution`'s definition of `template_parse_failed` as "The YAML did not parse",
which a `values:` key on a `list` parameter is not; that mismatch predates this change, spans every
conversion-stage refusal rather than these, and is #289's to settle.

#### Scenario: A list parameter declares a default and a description

- **WHEN** a template declares `tags: { type: list, default: [CONSUMABLE, KIDS], description: "Asset tags" }`
- **THEN** the template loads, and `tags` appears in the template's `params` on `GET /templates` and
  `GET /templates/{id}` with `type: "list"` and `default: ["CONSUMABLE", "KIDS"]`

#### Scenario: A forbidden attribute is refused

- **WHEN** a template declares `tags: { type: list, values: [a, b] }`
- **THEN** the template fails validation with a message naming `tags` and `values`, and the file is
  quarantined while the server still starts

#### Scenario: An explicitly null forbidden attribute is refused too

- **WHEN** a template declares `tags` as `type: list` with `multiline:` written and left empty, so it
  parses as an explicit null
- **THEN** the template fails validation with a message naming `tags` and `multiline`

#### Scenario: A scalar default is refused

- **WHEN** a template declares `tags: { type: list, default: "CONSUMABLE" }`
- **THEN** the template fails validation naming `tags`, and the file is quarantined

#### Scenario: A non-string element is refused rather than coerced

- **WHEN** a template declares `codes: { type: list, default: [1, true] }`
- **THEN** the template fails validation naming `codes` and the first offending element's position, and
  the file is quarantined

#### Scenario: Quoting is what an author does about it

- **WHEN** the same template declares `codes: { type: list, default: ["1", "true"] }`
- **THEN** it loads, and `{codes:join(', ')}` prints `1, true`

#### Scenario: A nested element is refused

- **WHEN** a template declares `tags: { type: list, default: [[a, b]] }`
- **THEN** the template fails validation naming `tags` and the offending element's position

#### Scenario: An empty default is a list, not an omission

- **WHEN** a template declares `tags: { type: list, default: [] }`, an active item renders
  `{tags:join(', ')}`, and a request omits `tags`
- **THEN** the label renders with that text empty, and the response is not `422 MissingField`

#### Scenario: An explicitly null default is an absent default

- **WHEN** a template declares `tags` as `type: list` with `default:` written and left empty, and an
  active item renders `{tags:join(', ')}`
- **THEN** the template loads with no default, and a request omitting `tags` fails with
  `422 MissingField` naming `tags`

#### Scenario: A token in an element is literal text

- **WHEN** a template declares `tags: { type: list, default: ["{vars.brand}"] }` and the store holds
  `brand = acme`
- **THEN** the label prints `{vars.brand}` rather than `acme`, because a non-string default carries no
  token

### Requirement: A request supplies a list as a JSON array

A request MAY supply a `list` parameter in its `data` map, per label, as a **JSON array of strings**.
Every element SHALL be a JSON string, and the service SHALL NOT coerce one.

A value that is not a JSON array SHALL be refused with `400 InvalidRequest` and `details.reason`
`request_body_invalid`, in a message naming the parameter, on the terms every other declared type refuses
a value it cannot coerce. An array carrying an element that is not a JSON string (a number, a boolean, a
null, an array or an object) SHALL be refused the same way, in a message naming the parameter and the
first offending element's position. A caller wanting numbers sends `["1", "2"]`, exactly as an author
declaring a default writes them.

A `list` parameter sent as JSON `null` SHALL be treated exactly as if the request had omitted it, as a
`datetime` is, so a caller has a spelling for "use the declared default". `[]` is **not** that spelling:
it is a value, and it resolves to the empty list.

A `list` parameter carries no resolution rule of its own. When a request omits it, it SHALL be resolved by
`param-resolution`: its declared `default` if it has one, and `422 MissingField` naming the parameter if
it does not and an active layout item reads it. A `list` only an inactive branch reads is not required.

In a batch, validation SHALL be per label: every label carrying a refused value SHALL appear in the
`details.failures` list of the `422 BatchInvalid` response, each entry naming its label index, the
`InvalidRequest` code and the `request_body_invalid` reason. The batch stays all-or-nothing: no PDF, no
ZIP and no print job SHALL be produced for any label in it.

**No declared parameter of any other type accepts a JSON array.** Each refuses one by its own existing
coercion contract, and only `string` changes here: it stringified an array into its JSON text and SHALL
now be refused with `400 InvalidRequest` and `details.reason` `request_body_invalid` naming the
parameter. `boolean`, `integer`, `number` and `length` already refuse one on those exact terms, `enum`
already refuses one as a value outside its `values`, and `datetime` already refuses one as
`datetime_param_invalid`. This requirement changes none of those four answers, because widening a shared
coercion path to make one new case tidier would alter behaviour for inputs #213 never mentions.

The declaration side of that rule is `param-resolution`'s and is not restated here: a `default:` that is
a YAML sequence is refused at load on every type but `list`, because refusing the request value while
accepting the declared one would breach that capability's rule that a default may not carry a value the
request could not have carried. It is written there because it qualifies that capability's own sentence
that a non-string default is used as written.

#### Scenario: A request supplies a list

- **WHEN** a request sends `tags: ["A", "B"]` for a template printing `{tags:join(', ')}`
- **THEN** the label reads `A, B`

#### Scenario: A non-string element is refused rather than coerced

- **WHEN** a request sends `codes: [1, true]` for a declared `list` parameter
- **THEN** the response is `400 InvalidRequest` with `details.reason` `request_body_invalid`, naming
  `codes` and the first offending element's position, rather than the label printing `1, true`

#### Scenario: The quoted form is accepted

- **WHEN** a request sends `codes: ["1", "true"]` for a template printing `{codes:join('|')}`
- **THEN** the label reads `1|true`

#### Scenario: A non-array value is refused

- **WHEN** a request sends `tags: "A,B"` for a declared `list` parameter
- **THEN** the response is `400 InvalidRequest` with `details.reason` `request_body_invalid`, and the
  message names `tags`

#### Scenario: A nested element is refused

- **WHEN** a request sends `tags: [["A"]]`
- **THEN** the response is `400 InvalidRequest` with `details.reason` `request_body_invalid` naming
  `tags` and the offending element's position, rather than the label printing `["A"]`

#### Scenario: A null is the same as omitting it

- **WHEN** a request sends `tags: null` and the parameter declares `default: [CONSUMABLE]`
- **THEN** the label prints `CONSUMABLE`, with no error

#### Scenario: An empty array is not an omission

- **WHEN** a request sends `tags: []` and the parameter declares `default: [CONSUMABLE]`
- **THEN** the joined text is empty, rather than the declared default being used

#### Scenario: An omitted list with no default fails

- **WHEN** a template declares `tags: { type: list }`, an active item renders `{tags:join(', ')}`, and
  the request omits `tags`
- **THEN** the response is `422 MissingField` naming `tags`

#### Scenario: One bad label fails the whole batch and is named

- **WHEN** a batch of three labels sends `tags: "A,B"` on the second
- **THEN** the response is `422 BatchInvalid`, no ZIP, PDF or print job is produced, and
  `details.failures` contains one entry for index 1 carrying the `InvalidRequest` code and the
  `request_body_invalid` reason

#### Scenario: A string parameter no longer stringifies an array

- **WHEN** a request sends `title: ["A", "B"]` for a parameter declared `type: string`
- **THEN** the response is `400 InvalidRequest` with `details.reason` `request_body_invalid` naming
  `title`, rather than the label printing `["A","B"]`

#### Scenario: Every other type keeps the answer it already gives

- **WHEN** a request sends an array for a parameter declared `enum`, `boolean`, `integer`, `number`,
  `length` or `datetime`
- **THEN** each is refused with exactly the code, reason and message it is refused with today

### Requirement: A list cannot resolve a layout attribute or bind an image

A `list` parameter SHALL NOT be usable where a template expects a numeric or dimension value: a
`format` width or height, an item's `width` or `height` `ref:`, `font_weight`, or any other `${param}`
reference resolved to a number. Such a reference SHALL fail validation at load with a message naming the
parameter and the context, exactly as the same reference to a `datetime` parameter does.

A `list` parameter SHALL likewise not be usable where a template expects a colour: a `text` item's
`color`, a `line` or `container` stroke colour, or a container `background`. Those attributes accept a
reference to a `string` or `enum` parameter, and a list is neither.

An `image` item's `name:` SHALL NOT name a `list` parameter. That key binds a `data` field directly
rather than through a token, and the value it binds is a data URI, which is one string and never a
sequence of them. The refusal SHALL name the parameter and the offending item's layout path, and it
belongs here rather than with the token rules precisely because `name:` is not a token: leaving it out
would have left the one scalar slot a template can write without a `{token}` accepting a list that
nothing could render.

Naming a layout path is **new** for a message raised by this stage of validation, which today reports a
parameter name and nothing about where it was read. This requirement asks for it on its own refusals
only. It does not ask for it on any message that exists today, and adding it to those is a change to
diagnostics no part of this capability governs.

The refusal is decidable from the template's own text, because `params:` is part of the file. A rejected
reference quarantines the file under the `template-registry` rules while the server still starts, and the
same content arriving through a template write SHALL be refused with `422 TemplateInvalid`.

The other two places a list may not appear are owned elsewhere and are not restated here: reading one
through a bare token or any reader other than a join is `interpolation-tokens`, and naming one in a
`when:` predicate is `conditional-visibility`. What an *undeclared* name carrying an array does at an
`image` `name:` binding is `interpolation-tokens`' rule too, with the rest of the render-time refusal.

#### Scenario: A list cannot drive a dimension

- **WHEN** a template declares `tags: { type: list }` and references it as a `format` width, a `text`
  item's `height`, or a `font_weight`
- **THEN** the template fails validation with a message naming `tags` and the context, and the file is
  quarantined

#### Scenario: A list cannot drive a colour

- **WHEN** a template declares `tags: { type: list }` and references it as a `text` item's `color`
- **THEN** the template fails validation with a message naming `tags` and `color`

#### Scenario: A list cannot bind an image

- **WHEN** a template declares `tags: { type: list }` and an `image` item carries `name: "tags"`
- **THEN** the template fails validation with a message naming `tags` and that item's layout path, the
  file is quarantined, and the same content arriving through a `PUT` is refused with
  `422 TemplateInvalid`
