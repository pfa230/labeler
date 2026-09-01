## Purpose

Defines what a colour is, wherever one is written in a template: the forms it may take, the names it
may use and the value each denotes, how a parameter reference to one is checked and resolved, how a
colour is reported when a template is read back, and what an unreadable one does to its template. One
vocabulary serves every field that takes a colour, so a name cannot mean one thing on a text item and
another on the shape behind it.

## ADDED Requirements

### Requirement: A colour is a name, a hex string, or a parameter reference

A **colour** SHALL be exactly one of three forms, and nothing else.

**A colour name**, one of these sixteen, matched **case-insensitively** (`red`, `Red` and `RED` are
one name), each denoting exactly the value given:

| Name | Value | Name | Value |
| --- | --- | --- | --- |
| `black` | `#000000` | `silver` | `#c0c0c0` |
| `white` | `#ffffff` | `gray` | `#808080` |
| `red` | `#ff0000` | `maroon` | `#800000` |
| `yellow` | `#ffff00` | `olive` | `#808000` |
| `lime` | `#00ff00` | `green` | `#008000` |
| `aqua` | `#00ffff` | `teal` | `#008080` |
| `blue` | `#0000ff` | `navy` | `#000080` |
| `fuchsia` | `#ff00ff` | `purple` | `#800080` |

These are the CSS Level 1 names and their CSS values. The table above is the contract: a name SHALL
denote the value stated here and SHALL NOT be resolved by asking the rendering engine, whose own
constants of the same names carry different values. The set is closed, so the accepted vocabulary
cannot move underneath a template when that engine changes.

**A hex string**, written with a leading `#` and 3, 4, 6 or 8 hexadecimal digits, case-insensitive in
its digits: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`. The 3- and 4-digit forms expand by digit
doubling (`#f0c` is `#ff00cc`). The 4- and 8-digit forms carry an alpha channel, and a partially
transparent colour SHALL composite over what lies behind it in both PNG and PDF output. The `#` is
required, which is what keeps a hex string and a colour name unambiguous; in YAML this means a hex
colour must be quoted.

**A parameter reference**, a string of the form `"{name}"`, governed by the parameter requirement
below.

Anything else SHALL be refused: a name outside the sixteen, a hex string without its `#`, a hex
string of any other digit count, a hex string containing a non-hex character, a non-string YAML
value, and the empty string alike. A refused colour SHALL NOT be silently substituted, defaulted or
dropped.

This vocabulary judges a colour that is **present**. Whether a key written with no value at all
(`color: null`, `background: null`) is an absence or a refusal is a rule of the field, not of the
vocabulary, and is stated by the capability that owns the field: `text-ink` for `text.color`, which
reads it as an absence, and `shape-paint` for `background` and `stroke.color`, which refuse it. This
capability neither creates nor removes either behaviour.

The names Typst documents beyond these sixteen are not colour names here. `eastern` and `orange` in
particular SHALL be refused, as any other unrecognised name is.

A colour SHALL NOT be refused for being illegible. `white` on the white page, or any colour
indistinguishable from what lies behind it, loads and renders as written. Legibility is the author's,
exactly as placing text outside the printable area already is. The vocabulary is likewise not
constrained to monochrome: a colour a monochrome device cannot reproduce SHALL be accepted and
rendered, and converting it for such a device belongs to the print path (ADR-0033).

#### Scenario: A named colour carries its stated value

- **WHEN** a template writes `red` as a colour on any field that takes one
- **THEN** it denotes `#ff0000`, and not any other colour of that name

#### Scenario: A name matches regardless of case

- **WHEN** a template writes `Red`, `RED` or `rEd` as a colour
- **THEN** each is accepted and denotes `#ff0000`

#### Scenario: Each accepted hex form

- **WHEN** a template writes `"#f0f"`, `"#F0F8"`, `"#ff00ff"` or `"#FF00FF80"` as a colour
- **THEN** each is accepted and denotes the colour it names, the 3-digit form expanding to `#ff00ff`
  and the 4- and 8-digit forms carrying alpha

#### Scenario: An alpha channel composites rather than being dropped

- **WHEN** a template paints `"#00000080"` over the white page
- **THEN** the result is the composite of that colour over white, not opaque black

#### Scenario: A malformed hex string is refused

- **WHEN** a template writes `"#ff00f"` (five digits), `"ff00ff"` (no `#`), `"#gg0000"` (not
  hexadecimal) or `""` as a colour
- **THEN** the template fails validation and is quarantined, naming the offending value

#### Scenario: A name outside the sixteen is refused

- **WHEN** a template writes `chartreuse`, `eastern` or `orange` as a colour
- **THEN** the template fails validation and is quarantined, naming the unknown colour, and the
  failure is colour validation rather than an unrecognised field

#### Scenario: A non-string value is refused

- **WHEN** a template writes `16711680` or `true` as a colour
- **THEN** the template fails validation and is quarantined, naming the offending value

#### Scenario: An illegible colour is not refused

- **WHEN** a `text` item declares `color: white` on the white page
- **THEN** the template loads and is served, and the item renders white on white

### Requirement: A name denotes one colour on every field that takes one

The fields that take a colour are `text.color`, `stroke.color` and `background`. Every one of them
SHALL read a colour under the requirement above, from the one table stated there.

A colour name SHALL therefore denote the same value on every one of those fields. Two items in one
template that write the same name SHALL paint the same colour, whatever kind of item each is. No
field SHALL carry a vocabulary or a name table of its own, and no field SHALL read a name this table
does not hold.

A field MAY carry a **default**, the colour it paints when the key is omitted: `text.color` renders
black (`text-ink`) and `stroke.color` draws black (`shape-paint`). A default names a value from the
table above rather than adding one to it, so it cannot make a name mean two things, which is what
this requirement forbids.

This requirement supersedes the divergence ADR-0092 §6 recorded as intentional, under which a text
item's `red` was `#ff4136` and a shape's `red` was `#ff0000`.

#### Scenario: One name, one colour, across item kinds

- **WHEN** a template declares a `text` item with `color: red` inside a `container` with
  `background: red`
- **THEN** both paint `#ff0000`, and the paint emitted for the two items carries the same value

#### Scenario: The table is the CSS one, not the engine's

- **WHEN** a template declares `color: red`, `color: green`, `color: gray` or `color: yellow` on a
  `text` item
- **THEN** each denotes the CSS value in the table (`#ff0000`, `#008000`, `#808080`, `#ffff00`) and
  not the rendering engine's constant of the same name

### Requirement: Every field that takes a colour takes a parameter reference

`text.color`, `stroke.color` and `background` SHALL each accept a parameter reference, a string of
the form `"{name}"`, in place of a literal colour. This is the same reference form `font_size`,
`font_weight` and the size vocabulary already accept.

The template SHALL be refused at load unless `name` is a parameter the template declares, of type
`string` or `enum`. Those are the two parameter types whose values are strings; a reference to an
undeclared parameter, or to one of any other type, is refused with the same error the service already
gives for a bad parameter reference elsewhere in a layout, naming the item's layout path, the field
and the parameter.

At render time the referenced parameter's value SHALL be resolved under the existing rules of the
`param-resolution` capability, and the resolved value SHALL then be read as a colour under the
vocabulary above, accepting a name or a hex string.

A resolved value that is not a colour SHALL fail the render. It SHALL NOT fall back to black, to the
field's default, to any other colour, or to rendering nothing: a request that asks for a colour the
service cannot read is answered with an error, not with a plausible label.

A resolved value that is itself a `"{name}"` reference SHALL be refused the same way. References do
not chain.

The failure surfaces on each path exactly as every other per-label render failure already does, and
gains no path of its own:

- On the single-label render endpoint it SHALL be a `400` with error code `InvalidRequest` and
  `details.reason` of `color_param_invalid`, and the message SHALL name the parameter.
- On the batch and print endpoints it SHALL be a `422` with error code `BatchInvalid`, whose
  `details.failures` carries one entry for each offending label with that label's `index`, the `code`
  and `reason` above, and the same message. A batch in which only some labels carry a bad colour
  SHALL fail as a whole, under the existing all-or-nothing rule of the `request-error-envelope`
  capability, and no output SHALL be produced for the labels that would have rendered.

`color_param_invalid` replaces the reason `ink_param_invalid`, which named a field that no longer
exists and covered one of the three fields.

#### Scenario: A referenced colour renders on a shape

- **WHEN** a template declares a `string` parameter `brand` and a container with
  `background: "{brand}"`, and a render request supplies `brand: "#c0392b"`
- **THEN** the container's interior renders in that colour

#### Scenario: A referenced colour renders on a stroke

- **WHEN** a template declares a `string` parameter `brand` and a container with
  `stroke: { thickness: 0.3, color: "{brand}" }`, and a render request supplies `brand: navy`
- **THEN** the outline renders `#000080`

#### Scenario: A referenced colour renders on text

- **WHEN** a template declares a `string` parameter `brand` and a `text` item with
  `color: "{brand}"`, and a render request supplies `brand: "#c0392b"`
- **THEN** the item's glyphs render in that colour

#### Scenario: An enum parameter drives the colour

- **WHEN** a template declares an `enum` parameter whose values are colour names and a container
  references it as its `background`
- **THEN** the template loads, and each enum value renders its own colour

#### Scenario: A reference to an undeclared parameter is refused at load

- **WHEN** a container declares `background: "{missing}"`, or
  `stroke: { thickness: 0.2, color: "{missing}" }`, and the template declares no parameter `missing`
- **THEN** the template is refused at load in each case, naming the item's layout path, the field and
  the undeclared parameter

#### Scenario: A reference to a parameter of the wrong type is refused at load

- **WHEN** a container declares `background: "{width}"` and `width` is declared as a `length`,
  `number`, `integer`, `boolean` or `datetime` parameter
- **THEN** the template is refused at load, naming the item's layout path, the field, the parameter
  and its type

#### Scenario: A resolved value that is not a colour fails a single render loudly

- **WHEN** a `POST /render/label` request supplies `brand: "octarine"` for a parameter referenced as
  a `background`
- **THEN** the response is `400` with error code `InvalidRequest` and `details.reason`
  `color_param_invalid`, and the message names `brand`
- **AND** no label is produced, and the container is not painted some other colour instead

#### Scenario: A bad colour in one batch label fails the batch

- **WHEN** a `POST /batch` request carries two labels and only the second supplies
  `brand: "octarine"` for a parameter referenced as a colour
- **THEN** the response is `422` with error code `BatchInvalid`, and `details.failures` holds one
  entry with `index` 1, `code` `InvalidRequest`, `reason` `color_param_invalid` and a message naming
  `brand`
- **AND** no PDF or ZIP is produced, including for the first label

#### Scenario: A resolved value cannot be another reference

- **WHEN** a render request supplies `brand: "{other}"` for a parameter referenced as a colour
- **THEN** the render is refused with the same `color_param_invalid` failure, on whichever endpoint
  carried it

### Requirement: A parameter referenced by a colour is reported as an input

A parameter that an active item references as a colour, on any of the three fields, SHALL appear in
the input list the service derives for the template, so a client rendering a form for that template
asks for it. It SHALL be reported as a layout attribute rather than as an interpolated value: it is
not substituted into any string the label prints, exactly as a parameter referenced by `font_weight`
or by a size is not.

An item that is gated off by its `when` SHALL NOT contribute its colour reference, under the existing
rules of the `template-inputs` capability for an inactive item. The parameters named by the `when`
itself are reported as they already are.

Where the same parameter is referenced both as a colour and interpolated into some item's `value`, it
SHALL be reported once, as interpolated.

#### Scenario: An active shape's colour parameter is asked for

- **WHEN** a template declares a `string` parameter `brand` and an ungated container with
  `background: "{brand}"`
- **THEN** the derived input list includes `brand`, marked as not interpolated

#### Scenario: An active text item's colour parameter is asked for

- **WHEN** a template declares a `string` parameter `brand` and an ungated `text` item with
  `color: "{brand}"`
- **THEN** the derived input list includes `brand`, marked as not interpolated

#### Scenario: A gated-off item does not ask for its colour parameter

- **WHEN** a container with `background: "{brand}"` is gated by a `when` that the supplied data does
  not satisfy
- **THEN** the derived input list does not include `brand`
- **AND** it still includes the parameters the `when` names

#### Scenario: A parameter used both ways is reported once

- **WHEN** `brand` is referenced as one item's `background` and interpolated into another item's
  `value`
- **THEN** the derived input list holds one entry for `brand`, marked as interpolated

### Requirement: A colour is reported as authored wherever a template is read back

A template's layout is exposed through the API (`GET /templates/{id}`). Every colour in that response
SHALL be reported as the author wrote it: the name in the case it was written, or the hex string in
the digit count and case it was written. A parameter reference SHALL be reported in its written form,
`"{name}"`.

Which keys appear in that response is unchanged by this capability. `stroke.color` SHALL be reported
even when the author omitted it, because a `stroke` block always carries a colour: `stroke:
{ thickness: 0.2 }` reports `"black"`, its default, spelled as a name. Every other colour key SHALL
be reported only when the author wrote one, so an omitted `text.color` or `background` stays absent
from the response, exactly as it is today. This requirement changes how a colour is spelled, and adds
no key that was not there before.

This requirement supersedes the canonical `#rrggbbaa` normalization of the `shape-paint` capability,
for the reason #291 gives for the surviving type: what the author wrote is recoverable from the
colour itself, on every field, rather than on one field and not the other. A client that needs one
comparable form per colour resolves the reported value through the table above; a client that wants
the template exactly as written reads `GET /templates/{id}/source`, which is unchanged.

#### Scenario: A named colour is reported by name

- **WHEN** a template declaring `background: red` and a `text` item with `color: red` is read back
- **THEN** the response reports `"red"` for both

#### Scenario: A hex colour is reported as written

- **WHEN** a template declaring `stroke: { thickness: 0.2, color: "#F0F" }` is read back
- **THEN** the response reports the stroke colour as `"#F0F"`, neither expanded nor lower-cased

#### Scenario: A defaulted stroke colour is reported, not omitted

- **WHEN** a template declaring `stroke: { thickness: 0.2 }`, with no `color`, is read back
- **THEN** the response reports the stroke colour as `"black"`
- **AND** it is not absent from the response

#### Scenario: A parameter reference is reported as a reference

- **WHEN** a template declaring `background: "{brand}"` is read back
- **THEN** the response reports `"{brand}"`, and does not resolve it

### Requirement: An unreadable colour quarantines its template and is refused at the write endpoint

A template carrying a colour the vocabulary does not accept, on any of the three fields, SHALL fail to
load: it is excluded from the served set and reported as broken through the same channel as every
other content fault, under the existing rules of the `template-registry` capability, and SHALL NOT
abort startup. The reported error SHALL name the file, the layout path of the offending item, and the
field.

The same refusal SHALL apply to a template submitted through the template write endpoint, which
validates before writing, and no file SHALL be written.

#### Scenario: One broken template does not take the others down

- **WHEN** the templates tree holds one template whose container declares `background: chartreuse`
  and one valid template
- **THEN** the service starts and the valid template is served
- **AND** the broken template is not served, and is reported as broken with an error naming its file,
  the item's layout path and the `background` field

#### Scenario: Writing a template with a bad colour is refused

- **WHEN** a template whose `text` item declares an unparseable `color` is submitted to the template
  write endpoint
- **THEN** the write is refused with a validation error naming the item's layout path and the `color`
  field, and no file is written

### Requirement: A colour renders on every output path

A resolved colour SHALL be carried into both the PNG and the PDF output, for a single label and for
every slot of a batched sheet alike, whether it paints glyphs, an outline or a fill.

On the bilevel path — `color_mode=bilevel` on the render endpoint, or a printer whose negotiated
`render.color_mode` is `bilevel` — a colour SHALL be converted by the same global luminance threshold
that path already applies to the whole raster, with no separate treatment and no separate treatment
per field. A light colour therefore becomes white and a dark one black, and a mid-tone resolves by
which side of that threshold it falls on. Nothing warns about this; it is the documented behaviour of
that path applied to every colour on the label.

#### Scenario: The colour survives both formats

- **WHEN** a template with a coloured `text` item inside a container with a `background` is rendered
  as PNG and as PDF
- **THEN** both carry both colours

#### Scenario: A batched sheet carries the colour in every slot

- **WHEN** a template with a coloured `text` item and a painted container is rendered as a
  multi-slot PDF sheet
- **THEN** every slot carries both colours

#### Scenario: Bilevel thresholds every colour alike

- **WHEN** a label whose `text` item declares a light colour inside a container with a dark
  `background` is rendered with `color_mode=bilevel`
- **THEN** the glyphs are white and the ground is black in the 1-bit output, by the same luminance
  threshold applied to the rest of the raster
