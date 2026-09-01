## MODIFIED Requirements

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

## REMOVED Requirements

### Requirement: A colour is a hex string or one of the named colours

**Reason**: One colour type now serves `text.color`, `stroke.color` and `background` (#291), so the
vocabulary is stated once, in the `colour-vocabulary` capability. A second statement here is what
allowed `red` to mean `#ff0000` on a shape and `#ff4136` on the text in front of it.

**Migration**: None for shape paint. The sixteen names, their values, the case-insensitive matching
and the four hex forms carry over verbatim to `colour-vocabulary`, which additionally accepts a
`"{param}"` reference on `stroke.color` and `background`.

### Requirement: A colour is reported canonically wherever a template is read back

**Reason**: The surviving colour type keeps the spelling the author wrote (#291), and reports it on
every field rather than normalizing one field and preserving the other. `colour-vocabulary` states
the replacement rule.

**Migration**: `GET /templates/{id}` reports `background: red` as `"red"` and
`stroke: { color: "#F0F" }` as `"#F0F"`, where each was previously `"#ff0000ff"` and `"#ff00ffff"`;
an omitted `stroke.color` reports `"black"` rather than `"#000000ff"`. A client that compared
canonical strings must resolve the reported value through the name table in `colour-vocabulary`, or
read `GET /templates/{id}/source`, which returns the authored YAML verbatim and is unchanged.
