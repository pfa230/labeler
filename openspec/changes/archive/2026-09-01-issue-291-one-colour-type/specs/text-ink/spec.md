## ADDED Requirements

### Requirement: A text item's foreground colour is named `color`

A `text` layout item SHALL accept an optional field `color`, the colour its glyphs are painted in,
read under the `colour-vocabulary` capability. A `text` item that declares no `color` SHALL render
black.

The field SHALL be spelled `color`, and `ink` SHALL NOT be accepted on any item. A `text` item
declaring `ink` SHALL be refused at load as it is refused on every other item: as an unknown field,
naming that item's layout path. There is no alias, no deprecation window and no warning path, so a
template written against the earlier spelling fails loudly rather than rendering something the author
did not ask for.

`color` SHALL NOT be accepted on any other layout item. `qr`, `image`, `line` and `container` reject
the key as they reject every other unknown field; a container's own paint is `background` and
`stroke.color`, owned by the `shape-paint` capability, and is never inherited by a `text` child.

`color` SHALL NOT affect layout. It changes no glyph metric, so text fitting, auto-shrink, wrapping,
truncation and every resolved extent are identical with and without it.

An explicit `color: null` SHALL be read as an absence, so the item renders black. This is what
`ink: null` does today and the rename does not change it. It is a rule of this field alone: the paint
keys `background` and `stroke.color` refuse an explicit null, under the `shape-paint` requirement that
owns them, and neither field's null rule follows from the shared colour vocabulary.

A template read back through the template API SHALL report the `color` an item declared, spelled as
the author wrote it under the `colour-vocabulary` capability, and SHALL omit the key for an item that
declared none. An uncoloured text item is therefore returned without a `color` key rather than with a
materialized `"black"`, which is what it is returned as today.

This requirement supersedes the frozen `docs/SPEC.md` §4.1 `text` field list insofar as that list is
exhaustive: a `text` item accepts `color` in addition to the fields named there. Every other clause
of that bullet — the semantics of `value`, placement, `font_size`, `font_weight`, `alignment`,
`overflow` and `when` — is unchanged and remains authoritative, as does the `wrap` requirement owned
by the `text-wrap-flag` capability.

#### Scenario: Absent colour renders black

- **WHEN** a `text` item declares no `color`
- **THEN** its glyphs render black, identically to the same template rendered before the field
  existed

#### Scenario: A colour does not move the text

- **WHEN** two otherwise identical `text` items differ only in that one declares a `color`
- **THEN** both resolve to the same box, the same fitted font size and the same line breaks

#### Scenario: A null colour is an absence on a text item

- **WHEN** a `text` item declares `color: null`
- **THEN** the template loads and the item renders black, identically to one declaring no `color`
- **AND** a container declaring `background: null` or `stroke: { thickness: 0.2, color: null }` is
  still refused, naming the field

#### Scenario: An omitted colour is omitted from the read-back

- **WHEN** a template whose `text` item declares no `color` is read back through
  `GET /templates/{id}`
- **THEN** that item carries no `color` key in the response
- **AND** an item declaring `color: red` reports `"red"`

#### Scenario: `ink` on a text item is refused

- **WHEN** a `text` item declares `ink: red`
- **THEN** the template fails validation and is quarantined with an unknown-field error naming
  `ink` and that item's layout path
- **AND** the server still starts and still serves every other template

#### Scenario: A colour on a non-text item is refused

- **WHEN** a template declares `color` on a `qr`, `image`, `line` or `container` item
- **THEN** the template is refused with an unknown-field error naming that item's layout path

## REMOVED Requirements

### Requirement: A text item's foreground colour is named `ink`

**Reason**: The field is renamed to `color` (#291), so that one word names the concept on every field
that takes one. The replacement requirement above carries the whole contract for the renamed field.

**Migration**: Rewrite `ink:` as `color:` on every `text` item. A template still writing `ink:` is
quarantined at load with an unknown-field error naming its layout path, and the server still starts.

### Requirement: An ink is a named colour, a hex colour, or a parameter reference

**Reason**: The vocabulary is now stated once, in the `colour-vocabulary` capability, for `text.color`,
`stroke.color` and `background` alike. Keeping a second statement here is what let `red` mean two
colours.

**Migration**: The accepted forms are unchanged in shape, but the name table is now the sixteen CSS
Level 1 names at their CSS values. A text item's `red` becomes `#ff0000` rather than `#ff4136`, and
every name except `black` and `white` likewise changes value; `eastern` and `orange` are refused.
A template wanting the previous value writes it as hex.

### Requirement: A bad ink quarantines its template and is refused at the write endpoint

**Reason**: Stated once in `colour-vocabulary`, for every field that takes a colour rather than for
this one.

**Migration**: None. The behaviour is unchanged: the template is quarantined, the server starts, and
the write endpoint refuses without writing a file.

### Requirement: A parameter-referenced ink is checked at load and resolved per render

**Reason**: Stated once in `colour-vocabulary`, which extends it to `stroke.color` and `background`.

**Migration**: The load-time check and the render-time resolution are unchanged. The failure reason
on the render endpoint is `color_param_invalid` rather than `ink_param_invalid`; a client matching on
that string must be updated.

### Requirement: A parameter-referenced ink is reported as an input

**Reason**: Stated once in `colour-vocabulary`, which extends it to the shape paint fields.

**Migration**: None for text. A parameter referenced by a shape's `background` or `stroke.color` now
appears in the input list as well.

### Requirement: An ink renders on every output path

**Reason**: Stated once in `colour-vocabulary`, which covers glyphs, outlines and fills under one
rule, including the bilevel threshold.

**Migration**: None. The behaviour is unchanged.
