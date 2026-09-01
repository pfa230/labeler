## MODIFIED Requirements

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

**Surrounding whitespace SHALL NOT be significant in a colour.** Leading and trailing whitespace is
stripped before a colour is read, and the value that remains is read exactly as if it had been
written without the padding. The rule SHALL hold identically for all three forms and at both times a
colour is read: a literal at load (`color: " red "` denotes `#ff0000`, `background: " #F0F "` denotes
`#ff00ff`), a parameter reference at load (`" {brand} "` is the reference `brand`, which it already
is), and a parameter value resolved at render, under the parameter requirement below. Whitespace is
what the rest of the template vocabulary already treats as whitespace, so a colour carries no
definition of its own: a size written `" 80mm "` and a colour written `" navy "` are stripped by the
same rule.

Whitespace is stripped only from the ends. Whitespace *inside* a colour SHALL be refused as any other
unreadable colour is: `"re d"` is not a name and `"# f0f"` is not a hex string.

Anything else SHALL be refused: a name outside the sixteen, a hex string without its `#`, a hex
string of any other digit count, a hex string containing a non-hex character, a non-string YAML
value, and the empty string alike. A value that is entirely whitespace SHALL be refused exactly as the
empty string is, because stripping leaves nothing to read: `color: "   "` is refused on the terms
`color: ""` is refused. A refused colour SHALL NOT be silently substituted, defaulted or dropped.

Stripping governs what a colour **is**, not how it is reported. The string the template declared,
padding included, is still what a read-back reports, under the as-authored requirement below.

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

#### Scenario: Surrounding whitespace does not change a colour

- **WHEN** a template writes `color: " red "` on a `text` item, `background: " #F0F "` on a
  container, and `stroke: { thickness: 0.2, color: " navy " }` on its outline
- **THEN** the template loads, and each denotes the colour it names (`#ff0000`, `#ff00ff`,
  `#000080`), identically to the same template written without the padding

#### Scenario: A padded parameter reference is still a reference

- **WHEN** a template declares a `string` parameter `brand` and an item writes `color: " {brand} "`
- **THEN** the template loads, and the field holds a reference to `brand`, exactly as `"{brand}"`
  does

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

#### Scenario: A colour that is entirely whitespace is refused

- **WHEN** a template writes `color: "   "` as a colour
- **THEN** the template fails validation and is quarantined on the terms `color: ""` is refused on,
  naming the file, the item's layout path and the field

#### Scenario: Whitespace inside a colour is still refused

- **WHEN** a template writes `"re d"` or `"# f0f"` as a colour
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

Reading it as a colour includes stripping its surrounding whitespace, under the requirement above:
a resolved `" navy "` SHALL paint `#000080`, exactly as a literal `" navy "` does. The stripping
SHALL happen before every test applied to the resolved value, not only before it is parsed as a name
or a hex string. A resolved `" {other} "` is therefore a chained reference and SHALL be refused as
one, with the chained-reference failure below, and SHALL NOT be reported as an unrecognised colour.
A resolved value that is entirely whitespace SHALL fail the render as any other value that is not a
colour does.

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

#### Scenario: A padded resolved colour renders

- **WHEN** a render request supplies `brand: " navy "` for a parameter referenced as a colour
- **THEN** the render succeeds and paints `#000080`, identically to a request supplying `navy`

#### Scenario: A padded resolved reference is refused as a chained reference

- **WHEN** a render request supplies `brand: " {other} "` for a parameter referenced as a colour
- **THEN** the render is refused with `color_param_invalid` and the chained-reference message, and
  not with the message for an unrecognised colour


### Requirement: A colour is reported as authored wherever a template is read back

A template's layout is exposed through the API (`GET /templates/{id}`). Every colour in that response
SHALL be reported as the author wrote it: the name in the case it was written, or the hex string in
the digit count and case it was written. A parameter reference SHALL be reported in its written form,
`"{name}"`.

What "as the author wrote it" preserves, exactly: the **string content of the decoded YAML scalar**
the field was written with. That is what the service holds and therefore all it can report. YAML
quoting, escape sequences and scalar style are *not* preserved and SHALL NOT be expected from this
response: a field written with an escape decodes to the characters it denotes before any colour is
read, and reports those. A client that wants the template exactly as the file holds it reads
`GET /templates/{id}/source`, which is unchanged.

Within that decoded string, a **literal** colour SHALL be reported with its content preserved
exactly, and that includes surrounding whitespace: a template written `color: " red "` reports
`" red "`, padding and all. This does not conflict with whitespace being insignificant in a colour.
That rule is about a colour's **identity**: `" red "` and `"red"` are one colour, both load and both
paint `#ff0000`. What is reported here is the string the template declared, which already preserves
distinctions identity ignores, name case (`Red`) and hex digit count (`#F0F`). Padding is one more of
those, and normalizing it here would be the same act as reporting `#F0F` as `#ff00ff`, which this
requirement forbids.

A **parameter reference** carries no such declared string and SHALL be reported in the canonical
form `"{name}"`, so a field written `" {brand} "` reports `"{brand}"`. This is what the service does today
and this change does not move it: a reference is the parameter it names, and the padding around it is
not a spelling of anything. The asymmetry is stated rather than removed, so neither form's read-back
has to be guessed at.

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

#### Scenario: A padded literal is reported with its padding

- **WHEN** a template declaring `color: " red "` on a `text` item is read back
- **THEN** the response reports `" red "`, with the surrounding whitespace intact and the name
  neither resolved nor re-spelled

#### Scenario: A padded reference is reported canonically

- **WHEN** a template declaring `background: " {brand} "` is read back
- **THEN** the response reports `"{brand}"`

#### Scenario: The YAML spelling of the scalar is not preserved

- **WHEN** a template writes a colour whose YAML scalar decodes to `red` by some other spelling than
  the three characters — an escape sequence, or a different quoting or scalar style — and is read
  back
- **THEN** the response reports `"red"`, the decoded content, and not the spelling the file used
- **AND** `GET /templates/{id}/source` still returns the file as written

