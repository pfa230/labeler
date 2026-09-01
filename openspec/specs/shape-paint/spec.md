## Purpose

Defines how a shape declares what it is drawn with: the outline that traces it, the colour that
fills it, and the corner radius both follow. One vocabulary serves every shape in the layout, so a
shape added later inherits the spelling rather than inventing one.

## Requirements

### Requirement: A shape is stroked; a shape with an interior is also filled

A **shape** is a layout item with a drawable boundary. `container` and `line` are the shapes.

A shape with an **interior** is a shape enclosing an area. `container` is the only one; a `line` has
no interior, and no future item becomes one by being a shape.

The paint keys are accepted by category, not uniformly:

| Key | Type | Accepted on | Meaning |
| --- | --- | --- | --- |
| `stroke` | block, see below | every shape | The outline tracing the shape. Omitted: no outline. |
| `background` | colour, per `colour-vocabulary` | a shape with an interior | The colour filling that interior. Omitted: nothing is filled, and whatever lies behind the shape shows through. |
| `rounded` | number | a shape with an interior | The corner radius. Omitted: square corners. |

A **colour** is what the `colour-vocabulary` capability defines, and this capability states no
vocabulary of its own: `background` accepts one of the sixteen names, a hex string, or a `"{param}"`
reference resolved per render, and the same name denotes the same colour here as on a `text` item's
`color`.

Where two keys are both accepted, neither SHALL imply the other. On a `container`, all four
combinations SHALL be accepted and SHALL render as declared: outline only, fill only, both, and
neither. A container with neither draws no boundary of its own and remains a positioning and grouping
construct, exactly as one with no paint does today.

`background` or `rounded` on a `line` SHALL be refused at load rather than ignored, because a line has
no interior to fill and no corners to round.

The requirements of this capability, together, supersede the `container` and `line` bullets of frozen
`docs/SPEC.md` §4.1 for everything those bullets say about `frame`, `thickness` and `rounded`. Every
other clause of those bullets (placement, `when`, `padding`, `items`, endpoint resolution and bounds
checking) stays authoritative.

#### Scenario: A fill with no outline

- **WHEN** a container declares `background: "#000000"` and no `stroke`
- **THEN** it renders as a solid black block with no outline drawn
- **AND** this holds in PNG output and in PDF output alike

#### Scenario: An outline with no fill

- **WHEN** a container declares `stroke: { thickness: 0.02 }` and no `background`
- **THEN** it renders as an outline only, and what lies behind it shows through the interior

#### Scenario: Both, on one container

- **WHEN** a container declares `stroke: { thickness: 0.3, color: red }` and `background: "#eee"`
- **THEN** the interior is filled `#eeeeee` and the outline is drawn `#ff0000` at 0.3 units

#### Scenario: Neither

- **WHEN** a container declares no `stroke` and no `background`
- **THEN** it draws nothing of itself, and only its children appear

#### Scenario: A line is stroked and nothing else

- **WHEN** a `line` declares `stroke: { thickness: 0.2, color: navy }`
- **THEN** it renders navy at 0.2 units

#### Scenario: A fill may be a parameter reference

- **WHEN** a container declares `background: "{brand}"` against a declared `string` parameter, and a
  render request supplies a colour for it
- **THEN** the interior is filled with that colour, under the `colour-vocabulary` capability

#### Scenario: A line has no interior to fill

- **WHEN** a `line` declares `background: black`, or `rounded: 1.0`
- **THEN** the template fails validation and is quarantined, naming the offending key on the line


### Requirement: A stroke is a thickness and a colour, and its thickness is finite and positive

`stroke` SHALL be a block with two fields:

| Field | Type | Required | Default |
| --- | --- | --- | --- |
| `thickness` | number | yes | none |
| `color` | colour, per `colour-vocabulary` | no | `black` |

`color` accepts everything a colour accepts, including a `"{param}"` reference resolved per render.

`thickness` is in the template `unit` and SHALL be **finite and at least 0.0001**. A `stroke` block
without `thickness`, or whose `thickness` is negative, zero, NaN, infinite, or positive but below
0.0001, SHALL be refused at load. "No outline" is spelled by omitting `stroke`, so it has exactly one
spelling.

The lower bound is not decorative, and it is the emitter's quantum rather than a round number.
Lengths reach the rendering engine formatted to four decimal places, so the only lengths that can be
emitted at all are multiples of 0.0001. A positive value below that quantum is therefore never drawn
as declared: it is emitted as zero, drawing nothing, or rounded up to the quantum, drawing something
the author did not write. Both outcomes are the template validating and then rendering something
other than its contract, which is what the single-spelling rule exists to prevent. Requiring at least
one quantum makes every accepted value a value that renders at the thickness it declares, to the
precision the emitter has. At 0.0001 mm, and at 0.0001 in, one quantum lies far below the resolution
of any target device, so the bound refuses nothing an author could print.

Omitting `color` SHALL draw the outline black, which is what a thickness alone draws today.

A `stroke` block SHALL accept no field other than these two. An unrecognised field SHALL be refused
at load rather than ignored.

#### Scenario: A thickness alone draws black

- **WHEN** a container declares `stroke: { thickness: 0.02 }`
- **THEN** the outline renders `#000000` at 0.02 units

#### Scenario: A stroke colour may be a parameter reference

- **WHEN** a container declares `stroke: { thickness: 0.3, color: "{brand}" }` against a declared
  `string` parameter, and a render request supplies a colour for it
- **THEN** the outline renders in that colour, under the `colour-vocabulary` capability

#### Scenario: A zero or negative thickness is refused

- **WHEN** a shape declares `stroke: { thickness: 0 }` or `stroke: { thickness: -0.5 }`
- **THEN** the template fails validation and is quarantined

#### Scenario: A positive thickness too small to render is refused

- **WHEN** a shape declares `stroke: { thickness: 0.00001 }`
- **THEN** the template fails validation and is quarantined, rather than validating and rendering no
  outline
- **AND** a shape declaring `stroke: { thickness: 0.0001 }` is accepted and renders a visible outline

#### Scenario: A non-finite thickness is refused

- **WHEN** a shape declares `stroke: { thickness: .nan }` or `stroke: { thickness: .inf }`
- **THEN** the template fails validation and is quarantined, and no Typst source is generated from
  the value

#### Scenario: A stroke block without a thickness is refused

- **WHEN** a shape declares `stroke: { color: red }`
- **THEN** the template fails validation and is quarantined, naming the missing `thickness`

#### Scenario: An unknown key inside a stroke is refused

- **WHEN** a shape declares `stroke: { thickness: 0.2, dash: dotted }`
- **THEN** the template fails validation and is quarantined, naming `dash`


### Requirement: The corner radius is authored, not derived from the stroke

A shape with an interior SHALL accept `rounded: <number>`, the corner radius in the template `unit`,
applied to all four corners.

The radius SHALL be **finite and at least 0.0001**, by the same reasoning and the same bound as
`stroke.thickness` above: a smaller positive value is below the emitter's quantum, so it is drawn
either as a square corner or as one radius the author did not write, never as the radius declared. Square corners are spelled by omitting `rounded`, so they have exactly one spelling: a zero
radius SHALL be refused rather than accepted as a second spelling of square, and NaN, infinity, or a
positive value below 0.0001 SHALL be refused.

The radius SHALL be independent of whether the shape has a stroke, of that stroke's thickness, and of
whether the shape has a background. The same radius SHALL shape the outline and the fill, so a filled
shape with no outline rounds exactly as a stroked one does.

An authored radius exceeding half the shorter side of the shape's resolved box SHALL be reduced to
half that side before rendering. The clamp is stated here rather than refused at load because a
shape's extent may resolve from its content or from its frame (`layout-sizing`), so the side length is
not always known when the template is validated, and refusing only the cases that are known would
make the rule depend on where the extent came from.

#### Scenario: A filled shape with no outline still rounds

- **WHEN** a container declares `background: black` and `rounded: 1.5`, with no `stroke`
- **THEN** the filled block renders with 1.5-unit rounded corners

#### Scenario: One radius shapes both outline and fill

- **WHEN** a container declares `stroke: { thickness: 0.3 }`, `background: white` and `rounded: 2.0`
- **THEN** the outline and the fill follow the same 2.0-unit corner, with no gap or overhang between
  them

#### Scenario: The radius does not track the stroke

- **WHEN** two containers declare `rounded: 1.0`, one with `stroke: { thickness: 0.1 }` and one with
  `stroke: { thickness: 0.9 }`
- **THEN** both render the same 1.0-unit corner radius

#### Scenario: A zero, non-finite, or unrenderable radius is refused

- **WHEN** a container declares `rounded: 0`, `rounded: .nan`, `rounded: .inf` or `rounded: 0.00001`
- **THEN** the template fails validation and is quarantined
- **AND** a container declaring `rounded: 0.0001` is accepted

#### Scenario: An oversized radius is clamped

- **WHEN** a container whose box resolves to 4.0 by 2.0 units declares `rounded: 5.0`
- **THEN** the corner radius renders as 1.0, half the shorter side, on both the fill and the outline


### Requirement: A container's paint covers its whole box, unrotated, behind its children

A container's `background` and `stroke` SHALL be painted on the container's **outer** box: the
rectangle its resolved placement occupies in its parent's coordinate frame. The paint therefore
covers the `padding` band as well as the padded inner box, because padding insets the children and
not the shape.

Within a container, the paint SHALL be drawn in this order, back to front: the `background`, then the
`stroke`, then the container's `items`. A child therefore always draws on top of the ground it sits
on, and a filled container never hides its own contents.

The stroke SHALL be **centred on the boundary** of that box, so half its thickness lies outside the
box and half inside. That outer half SHALL NOT participate in size resolution: a stroke never grows a
shape, never insets its children, and contributes nothing to any extent (`layout-sizing` is unchanged
by this capability). Ink falling outside the box SHALL be clipped by whatever already clips the shape
itself: the enclosing container's box, or the label.

A container's paint SHALL NOT be rotated by the container's `rotate` (frozen `docs/SPEC.md` §4.2, and
`layout-sizing`), which rotates the inner content only. The painted rectangle stays axis-aligned in
the parent frame at every rotation.

A shape that does not render, because a `when` gate excludes it, SHALL paint nothing.

#### Scenario: The fill sits behind the children

- **WHEN** a container declares `background: black` and contains a `text` child
- **THEN** the text draws over the black ground rather than under it

#### Scenario: Padding is inside the paint

- **WHEN** a container declares `padding: 0.5` and `background: black`
- **THEN** the black covers the full outer box, and the 0.5-unit band inset from its edge is black
  with no child drawn in it

#### Scenario: A stroke does not change any size

- **WHEN** two sibling containers are packed adjacently, one carrying `stroke: { thickness: 1.0 }`
  and one carrying none
- **THEN** both occupy the same extents they would with no stroke at all, and the stroked one's
  outer half-thickness overlaps its neighbour rather than displacing it

#### Scenario: A stroke is clipped at the boundary that clips the shape

- **WHEN** a container flush against the label's left edge declares `stroke: { thickness: 1.0 }`
- **THEN** the outer half of that stroke falls outside the label and is clipped, and the label's own
  dimensions are unchanged

#### Scenario: Rotation does not rotate the paint

- **WHEN** a container declares `rotate: 90`, `background: black` and a text child
- **THEN** the black rectangle stays axis-aligned in the parent frame while the text renders rotated

#### Scenario: A gated-off shape paints nothing

- **WHEN** a container declares `when: { outline: yes }` with `stroke: { thickness: 0.02 }`, and the
  request resolves `outline` to any other value
- **THEN** neither an outline nor a fill is drawn


### Requirement: Paint belongs to shapes alone and is never inherited

`stroke`, `background` and `rounded` SHALL be accepted only where the first requirement of this
capability places them, and nowhere else.

A `text`, `qr` or `image` item declaring any of the three SHALL be refused at load, as any other
field those items do not accept already is.

Paint SHALL NOT be inherited. A container's `background` sets what lies behind its children and
SHALL NOT set a colour that any child draws with, at any depth.

#### Scenario: Paint on a non-shape is refused

- **WHEN** a `text`, `qr` or `image` item declares `stroke`, `background` or `rounded`
- **THEN** the template fails validation and is quarantined, naming the field and the item

#### Scenario: A background is not inherited as a child's colour

- **WHEN** a container declares `background: black` and contains a `text` child and a nested
  container of its own
- **THEN** the text renders in the colour it would render in with no background declared, and the
  nested container renders with no background of its own


### Requirement: An explicit null is not a spelling of absence

`stroke`, `background`, `rounded` and `stroke.color` each carry a rule that an omitted key means
something definite: no outline, no fill, square corners, black. An explicit YAML `null` SHALL NOT be
accepted as a second way to say any of those. Writing `stroke: null`, `background: null`,
`rounded: null` or `color: null` SHALL be refused at load, naming the field.

`stroke.thickness` is required rather than optional, and an explicit null on it SHALL be refused
naming `thickness` as a null, not reported as a missing field. The distinction matters for the same
reason as above: "you wrote nothing here" and "you wrote a key with no value" are different mistakes,
and collapsing them tells the author to add a field they can see they already wrote.

Absence and null are therefore distinguishable, and only absence carries meaning. This is what keeps
"exactly one spelling" true in practice rather than only on paper: a key present with no value is an
authoring mistake, and reporting it is more useful than quietly treating it as though the author had
deleted the line.

#### Scenario: A null paint key is refused

- **WHEN** a container declares `stroke: null`, `background: null` or `rounded: null`
- **THEN** the template fails validation and is quarantined, naming the field
- **AND** the same template with the key omitted entirely is accepted

#### Scenario: A null thickness is refused as a null, not as an absence

- **WHEN** a shape declares `stroke: { thickness: null }`
- **THEN** the template fails validation and is quarantined, reporting `thickness` as null
- **AND** the message is distinguishable from the one a `stroke` block with no `thickness` key produces

#### Scenario: A null colour inside a stroke is refused

- **WHEN** a shape declares `stroke: { thickness: 0.2, color: null }`
- **THEN** the template fails validation and is quarantined, naming `color`
- **AND** it is not treated as an omitted `color` defaulting to black


### Requirement: The superseded spellings no longer parse

The spellings this capability replaces SHALL be refused at load, so that no template can silently
keep the old meaning:

| Removed spelling | Replacement |
| --- | --- |
| `container.frame: { thickness, rounded }` | `stroke: { thickness }` and `rounded: <number>` on the container |
| `line.thickness: <number>` | `stroke: { thickness: <number> }` on the line |
| `rounded: true` | `rounded: <number>`, an explicit radius |
| `rounded: false` | omit `rounded` |

A template using any of them SHALL fail validation with an error naming the offending field, and
SHALL be quarantined at load. A quarantined template SHALL NOT prevent the server from starting or
from serving any other template.

#### Scenario: A frame block is refused

- **WHEN** a container declares `frame: { thickness: 0.02, rounded: false }`
- **THEN** the template fails validation and is quarantined, naming `frame`
- **AND** the server still starts and still serves every other template

#### Scenario: A bare line thickness is refused

- **WHEN** a `line` declares `thickness: 0.2` outside a `stroke` block
- **THEN** the template fails validation and is quarantined, naming `thickness`

#### Scenario: A boolean radius is refused

- **WHEN** a container declares `rounded: true` or `rounded: false`
- **THEN** the template fails validation and is quarantined, naming `rounded`
