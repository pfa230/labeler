## Purpose

Defines the `ink` field on a `text` layout item: the colour vocabulary it accepts, its parameter
reference form, when a template carrying one is refused, and what it renders to on each output path.
This capability owns the text item's foreground colour and nothing else: it says nothing about a
ground to reverse out of, about `line`, about a frame's stroke, or about any colour a container might
pass to its descendants.

## Requirements

### Requirement: A text item's foreground colour is named `ink`

A `text` layout item SHALL accept an optional field `ink`, the colour its glyphs are painted in. A
`text` item that declares no `ink` SHALL render black, which is what every text item renders today, so
no existing template changes appearance.

`ink` SHALL NOT be accepted on any other layout item. `qr`, `image`, `line` and `container` reject the
key as they reject every other unknown field.

`ink` SHALL NOT affect layout. It changes no glyph metric, so text fitting, auto-shrink, wrapping,
truncation and every resolved extent are identical with and without it.

A template read back through the template API SHALL report the `ink` an item declared, and SHALL omit
the key for an item that declared none.

This requirement supersedes the frozen `docs/SPEC.md` §4.1 `text` field list insofar as that list is
exhaustive: a `text` item accepts `ink` in addition to the fields named there. Every other clause of
that bullet — the semantics of `value`, placement, `font_size`, `font_weight`, `alignment`, `overflow`
and `when` — is unchanged and remains authoritative, as does the `wrap` requirement owned by the
`text-wrap-flag` capability.

#### Scenario: Absent ink renders black

- **WHEN** a `text` item declares no `ink`
- **THEN** its glyphs render black, identically to the same template rendered before `ink` existed

#### Scenario: Ink does not move the text

- **WHEN** two otherwise identical `text` items differ only in that one declares an `ink`
- **THEN** both resolve to the same box, the same fitted font size and the same line breaks

#### Scenario: Ink on a non-text item is refused

- **WHEN** a template declares `ink` on a `qr`, `image`, `line` or `container` item
- **THEN** the template is refused with an unknown-field error naming that item's layout path

### Requirement: An ink is a named colour, a hex colour, or a parameter reference

`ink` SHALL accept exactly three forms, and no others.

**A colour name**, one of: `black`, `gray`, `silver`, `white`, `navy`, `blue`, `aqua`, `teal`,
`eastern`, `purple`, `fuchsia`, `maroon`, `red`, `orange`, `yellow`, `olive`, `green`, `lime`. The set
is closed and is stated here rather than delegated, so the accepted vocabulary cannot move underneath
a template when the rendering engine changes. Names are matched exactly, in lower case.

**A hex colour**, written with a leading `#` and 3, 4, 6 or 8 hexadecimal digits — `#RGB`, `#RGBA`,
`#RRGGBB`, `#RRGGBBAA` — case-insensitive in its digits. The `#` is required, which is what keeps a
hex colour and a colour name unambiguous; in YAML this means a hex ink must be quoted. The 4- and
8-digit forms carry an alpha channel, and the ink composites over whatever is behind it.

**A parameter reference**, a string of the form `"{name}"`, resolved per render from the request's
parameters. This is the same reference form `font_size`, `font_weight` and the size vocabulary already
accept.

Anything else SHALL be refused: an unrecognised name, a hex string without its `#`, a hex string of
any other digit count, a non-string YAML value, and the empty string alike.

An `ink` SHALL NOT be refused for being illegible. A `white` ink on the white page, or any other
colour indistinguishable from its ground, loads and renders as written. Legibility is the author's,
exactly as placing text outside the printable area already is.

#### Scenario: A named colour is accepted

- **WHEN** a `text` item declares `ink: red`
- **THEN** the template loads and the item's glyphs render in that colour

#### Scenario: A hex colour is accepted in every permitted digit count

- **WHEN** a `text` item declares `ink` as `"#f00"`, `"#f008"`, `"#ff0000"` or `"#ff000088"`
- **THEN** the template loads in each case, and the two 8-bit-per-channel forms render identically to
  their short forms

#### Scenario: An alpha channel composites rather than being dropped

- **WHEN** a `text` item declares `ink: "#00000080"` over the white page
- **THEN** its glyphs render as the composite of that colour over white, not as opaque black

#### Scenario: An unparseable literal is refused

- **WHEN** a `text` item declares `ink: chartreuse`, `ink: "ff0000"`, `ink: "#ff00"`, `ink: 16711680`
  or `ink: ""`
- **THEN** the template is refused in each case, with an error naming the item's layout path and the
  `ink` field

#### Scenario: An illegible ink is not refused

- **WHEN** a `text` item declares `ink: white` and no template can yet paint a ground behind it
- **THEN** the template loads and is served, and the item renders white on white

### Requirement: A bad ink quarantines its template and is refused at the write endpoint

A template whose `text` item declares an ink the vocabulary does not accept SHALL fail to load: it is
excluded from the served set and reported as broken through the same channel as every other content
fault, under the existing rules of the `template-registry` capability, and SHALL NOT abort startup.
The reported error SHALL name the file, the layout path of the offending item, and the `ink` field.

The same refusal SHALL apply to a template submitted through the template write endpoint, which
validates before writing, and no file SHALL be written.

#### Scenario: One broken template does not take the others down

- **WHEN** the templates tree holds one template whose `text` item declares an unparseable `ink` and
  one valid template
- **THEN** the service starts and the valid template is served
- **AND** the broken template is not served, and is reported as broken with an error naming its file,
  the item's layout path and the `ink` field

#### Scenario: Writing a template with a bad ink is refused

- **WHEN** a template whose `text` item declares an unparseable `ink` is submitted to the template
  write endpoint
- **THEN** the write is refused with a validation error naming the item's layout path and the `ink`
  field, and no file is written

### Requirement: A parameter-referenced ink is checked at load and resolved per render

When `ink` is a `"{name}"` reference, the template SHALL be refused at load unless `name` is a
parameter the template declares, of type `string` or `enum`. Those are the two parameter types whose
values are strings; a reference to an undeclared parameter, or to one of any other type, is refused
with the same error the service already gives for a bad parameter reference elsewhere in a layout.

At render time the referenced parameter's value SHALL be resolved under the existing rules of the
`param-resolution` capability, and the resolved value SHALL then be read as an ink under the
vocabulary above, accepting a colour name or a hex colour.

A resolved value that is not a colour SHALL fail the render. It SHALL NOT fall back to black, to any
other colour, or to rendering nothing: a request that asks for a colour the service cannot read is
answered with an error, not with a plausible label.

A resolved value that is itself a `"{name}"` reference SHALL be refused the same way. References do
not chain.

The failure surfaces on each path exactly as every other per-label render failure already does, and
gains no path of its own:

- On the single-label render endpoint it SHALL be a `400` with error code `InvalidRequest` and
  `details.reason` of `ink_param_invalid`, and the message SHALL name the parameter.
- On the batch and print endpoints it SHALL be a `422` with error code `BatchInvalid`, whose
  `details.failures` carries one entry for each offending label with that label's `index`, the
  `code` and `reason` above, and the same message. A batch in which only some labels carry a bad ink
  SHALL fail as a whole, under the existing all-or-nothing rule of the `request-error-envelope`
  capability, and no output SHALL be produced for the labels that would have rendered.

#### Scenario: A referenced ink renders the requested colour

- **WHEN** a template declares a `string` parameter `brand` and a `text` item with `ink: "{brand}"`,
  and a render request supplies `brand: "#c0392b"`
- **THEN** the item's glyphs render in that colour

#### Scenario: An enum parameter drives the ink

- **WHEN** a template declares an `enum` parameter whose values are colour names and a `text` item
  references it as its `ink`
- **THEN** the template loads, and each enum value renders its own colour

#### Scenario: A reference to an undeclared parameter is refused at load

- **WHEN** a `text` item declares `ink: "{missing}"` and the template declares no parameter `missing`
- **THEN** the template is refused at load, naming the item's layout path and the undeclared parameter

#### Scenario: A reference to a parameter of the wrong type is refused at load

- **WHEN** a `text` item declares `ink: "{width}"` and `width` is declared as a `length`, `number`,
  `integer`, `boolean` or `datetime` parameter
- **THEN** the template is refused at load, naming the item's layout path, the parameter and its type

#### Scenario: A resolved value that is not a colour fails a single render loudly

- **WHEN** a `POST /render/label` request supplies `brand: "octarine"` for a parameter referenced as
  an `ink`
- **THEN** the response is `400` with error code `InvalidRequest` and `details.reason`
  `ink_param_invalid`, and the message names `brand`
- **AND** no label is produced, and the text is not rendered black instead

#### Scenario: A bad ink in one batch label fails the batch

- **WHEN** a `POST /batch` request carries two labels and only the second supplies `brand: "octarine"`
  for a parameter referenced as an `ink`
- **THEN** the response is `422` with error code `BatchInvalid`, and `details.failures` holds one
  entry with `index` 1, `code` `InvalidRequest`, `reason` `ink_param_invalid` and a message naming
  `brand`
- **AND** no PDF or ZIP is produced, including for the first label

#### Scenario: A resolved value cannot be another reference

- **WHEN** a render request supplies `brand: "{other}"` for a parameter referenced as an `ink`
- **THEN** the render is refused with the same `ink_param_invalid` failure, on whichever endpoint
  carried it

### Requirement: A parameter-referenced ink is reported as an input

A parameter an active `text` item references as its `ink` SHALL appear in the input list the service
derives for the template, so a client rendering a form for that template asks for it. It SHALL be
reported as a layout attribute rather than as an interpolated value: it is not substituted into any
string the label prints, exactly as a parameter referenced by `font_weight` or by a size is not.

An item that is gated off by its `when` SHALL NOT contribute its ink reference, under the existing
rules of the `template-inputs` capability for an inactive item. The parameters named by the `when`
itself are reported as they already are.

Where the same parameter is referenced both as an ink and interpolated into some other item's
`value`, it SHALL be reported once, as interpolated.

#### Scenario: An active item's ink parameter is asked for

- **WHEN** a template declares a `string` parameter `brand` and an ungated `text` item with
  `ink: "{brand}"`
- **THEN** the derived input list includes `brand`, marked as not interpolated

#### Scenario: A gated-off item does not ask for its ink parameter

- **WHEN** a `text` item with `ink: "{brand}"` is gated by a `when` that the supplied data does not
  satisfy
- **THEN** the derived input list does not include `brand`
- **AND** it still includes the parameters the `when` names

#### Scenario: A parameter used both ways is reported once

- **WHEN** `brand` is referenced as one item's `ink` and interpolated into another item's `value`
- **THEN** the derived input list holds one entry for `brand`, marked as interpolated

### Requirement: An ink renders on every output path

A resolved `ink` SHALL be carried into both the PNG and the PDF output, for a single label and for
every slot of a batched sheet alike.

On the bilevel path — `color_mode=bilevel` on the render endpoint, or a printer whose negotiated
`render.color_mode` is `bilevel` — an ink SHALL be converted by the same global luminance threshold
that path already applies to the whole raster, with no separate treatment. A light ink therefore
becomes white and a dark one black, and a mid-tone resolves by which side of that threshold it falls
on. Nothing warns about this; it is the documented behaviour of that path applied to a new field.

#### Scenario: The colour survives both formats

- **WHEN** a template with a coloured `text` item is rendered as PNG and as PDF
- **THEN** both carry the colour

#### Scenario: A batched sheet carries the colour in every slot

- **WHEN** a template with a coloured `text` item is rendered as a multi-slot PDF sheet
- **THEN** every slot carries the colour

#### Scenario: Bilevel thresholds a coloured ink like everything else

- **WHEN** a label whose `text` item declares a light ink is rendered with `color_mode=bilevel`
- **THEN** those glyphs are white in the 1-bit output, by the same luminance threshold applied to the
  rest of the raster
