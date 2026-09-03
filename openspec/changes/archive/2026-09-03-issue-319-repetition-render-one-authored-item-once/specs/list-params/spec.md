## MODIFIED Requirements

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

A `list` parameter named `p` claims the interpolation token `{p:join('<sep>')}`, and a bare `{p}` in
every part of the template except a subtree repeating it, where the name is bound to one element and the
bare token is the only spelling (`repetition`, `interpolation-tokens`). Parameter naming and both
readings are governed by those capabilities, which own the rules; this requirement adds nothing to
either.

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

**Every refusal above holds inside a repeat scope too**, and this is where the binding a `repeat:`
creates stops. A `container` carrying `repeat: tags` binds `tags` to one element for what the subtree
reads **as text**, which is an interpolation token and a `when:` key, and for nothing else
(`repetition`). Each attribute this requirement names reads a parameter as a **typed value** instead, so
inside such a subtree the name still denotes the declared `list` and is still refused, naming the
parameter and the context.

The reason is a failure the wider rule would cause: a template's geometry and colours are validated when
it loads, against one instantiated value per parameter, and a repeated name has no single value at load.
A per-instance extent or colour would therefore be one no load could check, and the load-time refusal of
a packed child too large for its parent's padded inner box could not see it. Admitting one of these
slots later is additive, and withdrawing it would not be.

The other two places a list may not appear are owned elsewhere and are not restated here: reading one
through a bare token or any reader other than a join is `interpolation-tokens`, and naming one in a
`when:` predicate is `conditional-visibility`. Each of those two now carries the one exception a repeat
scope makes to it, which is why the boundary is stated here rather than assumed. What an *undeclared*
name carrying an array does at an `image` `name:` binding is `interpolation-tokens`' rule too, with the
rest of the render-time refusal.

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

#### Scenario: A repeat scope does not open any of these slots

- **WHEN** an item inside a container carrying `repeat: tags` declares `size: ["{tags}", 4]`, or
  `color: "{tags}"`, or is an `image` carrying `name: "tags"`
- **THEN** each fails validation naming `tags` and the context, exactly as the same spelling outside
  every repeat does
- **AND** a `text` inside that same container reading `{tags}` loads, because a token reads the bound
  element
