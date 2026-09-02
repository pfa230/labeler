## ADDED Requirements

### Requirement: A container's box is painted with a geometry

A `container` SHALL accept `shape`, naming the geometry its paint takes within its resolved box. The
accepted values SHALL be `rect`, `ellipse` and `circle`, and the default when `shape` is omitted SHALL
be `rect`, so a template written before this key renders exactly as it did.

A container remains a positioning and grouping construct at every geometry: it holds `items`, it
establishes a padded inner frame, it accepts `rotate`, and its children are laid out and placed in its
box identically at every geometry. A geometry SHALL change what is drawn and what survives the clip,
never where a child sits or how large it is.

- **`rect`** SHALL paint the resolved box itself, and SHALL be the only geometry accepting `rounded`.
- **`ellipse`** SHALL paint the ellipse inscribed in the resolved box, touching all four sides, so a
  square box paints a circle.
- **`circle`** SHALL paint the same ellipse and SHALL be refused unless the resolved box is square.

**A container SHALL clip its children to the boundary of the element that holds them.** One rule, and
the geometries differ only in which element that is:

- At **`rect`** the painted element *is* the child-holding element: one box carries the fill, the
  stroke, the radius, the clip and the children. Its boundary therefore clips, and it clips at the
  **inner edge of its stroke**, corner radius included. A child whose ink reaches the edge of a
  stroked container SHALL be cut half that stroke's thickness in from the box, and a child reaching
  into a corner rounded by `rounded` SHALL be cut on a rounded curve there. With no stroke that curve
  is the painted boundary itself; with one it is the stroke's inner corner curve, and the radius
  requirement of this capability states the distinction. This is `border-radius` with
  `overflow: hidden`, which likewise rounds the clip and likewise clips inside the border, at the
  padding edge.
- At **`ellipse`** and **`circle`** the painted element is *not* the child-holding element, because
  the emitter has no round box: its only clip region is a rounded rectangle
  (`clip_rect(size, radius, stroke, outset)`, `typst-layout-0.15.1/src/shapes.rs:617`) and its round
  geometries ignore a radius entirely (`shapes.rs:576-591`). The painted curve is placed separately
  and the children are held by a plain, unstroked rectangle, so the clip is that whole rectangle. A
  child crossing the painted curve SHALL be drawn in full, and a child reaching the box edge SHALL
  draw over the stroke rather than be cut by it.

Both differences follow from that single cause and are stated here rather than left to be inferred. A
container's clip SHALL bound its children alone and SHALL NOT cut the container's own paint, at either
geometry; the paint-coverage requirement of this capability states what does clip that paint.

`circle` names an intent the service enforces rather than a geometry `ellipse` cannot express: it is
what makes a later edit to `size` that would turn the circle into an oval fail loudly instead of
silently changing the drawing.

Two extents are **square** when they differ by no more than **0.0001** in the template `unit`.
Exact equality is not the rule and SHALL NOT be used: a resolved extent is a 32-bit float and a `to`
extent is a subtraction, so two extents an author wrote as equal need not compare equal, and refusing
them would refuse a drawing the service renders as a circle.

The bound is this service's own bounds tolerance, the one every other comparison on a resolved extent
already uses, so squareness agrees with those comparisons on the edge cases instead of drawing a
second line beside them. It is a **deliberate tolerance, not a proof of identical output.** Lengths
reach the rendering engine formatted to four decimal places, and the two dimensions are formatted
independently, so a pair within the tolerance may still be emitted one quantum apart: `1.00004` and
`1.00006` differ by `0.00002` and format to `1.0000` and `1.0001`. The rule accepts that. What it buys
is one stated line that load and render draw in the same place; what it costs is that a circle may be
emitted with sides differing by one quantum, which is below the resolution of any target device.

Squareness is judged on the **resolved** box, so where each extent comes from decides where it can be
judged. An extent is **fixed by the template** when the number it resolves to is written there, and
that property SHALL be classified where every other property of an extent is classified: alongside the
source, by the one classifier `layout-sizing` describes, and never by a later rule re-reading the
spelling. Every spelling is covered:

| Extent | Source | Fixed by the template? |
| --- | --- | --- |
| a number | author | yes |
| a `to` whose corners are both non-negative or both sign-negative | author | yes |
| a `"{param}"` reference | author | no: a request's `data` outranks the declared `default:` (`param-resolution`) |
| a `to` with a sign-negative `at` and a non-negative `to` | author | no: it evaluates against the frame |
| `content` | content | no: nothing is measured until render |
| `fill`, or a `to` with a non-negative `at` and a sign-negative `to` | frame | no: the frame follows the label's own sizing, which a dynamic-width label decides per render |

The check SHALL run at two points, and the two are not alternatives:

- At **render**, every **active** `circle` that reaches measurement or rendering SHALL have its
  resolved box checked, whatever its extents' sources and whether or not load already passed it. A box
  that is not square SHALL be refused under the render-time error mapping below.
- At **load**, a `circle` **both** of whose extents are fixed by the template SHALL additionally be
  refused there when that box is not square. That refusal is ordinary structural validation and
  carries no new error mapping: the template fails validation and is quarantined exactly as every
  other refusal in this capability does, reported as `TemplateInvalid` with the reason template
  validation already carries (frozen `docs/SPEC.md` §10.1), never as `circle_box_not_square`.

**A gated-off circle is not checked at render.** An item whose `when:` predicate is false, and every
child of such a container, is excluded from the measurement pre-pass and from rendering (frozen
`docs/SPEC.md` §5). An inactive `circle` therefore has no resolved box, its request-dependent extents
are never resolved, and it SHALL NOT fail the request however its `size` would have resolved had it
been active. This is not an exception to the render rule; it is what "active" means, and it is the
same rule that already keeps a parameter read only by an inactive branch from being required
(`param-resolution`). Load-time structural validation is unaffected: it judges the template rather
than a request, so a `circle` whose extents are fixed by the template is refused at load whatever
`when:` it carries.

The render check is the guarantee, and among active circles it carries no exception. The load check is
the strict subset of it the service can answer early, and it answers identically, because an extent
fixed by the template resolves to the number written whatever the request, the content or the frame
supplies. Load SHALL NOT judge a `circle` on any other extent, and the reason is not that the answer
is merely unknown there: it is that load has a wrong answer to hand and would act on it. Load
substitutes the available extent for an intrinsic one, so a square `content`-sized circle inside a
non-square frame looks non-square to it; and it stands a declared `default:` in for a parameter it has
not been given, which a request then overrides. Judging on either would quarantine a template for a
drawing it never renders.

`circle_box_not_square` is a new entry in the error contract, it is **render-time only**, and this
requirement is its published home: the frozen `docs/SPEC.md` §10.1 is not edited, and every row
already there remains authoritative. This requirement extends that registry by exactly one row and
changes none of them. It adds that row under `UnsupportedLayoutItem`, a code that already carries a
reason, so it does not extend `reason` to a fifth code (ADR-0052). The complete mapping is:

| Code | Status | Reason | When |
| --- | --- | --- | --- |
| `UnsupportedLayoutItem` | `422` | `circle_box_not_square` | An **active** `container` declaring `shape: circle` resolves, during a render, a box whose width and height are not square by the rule above. |

How that reaches the caller follows the envelope each path already has, and this requirement adds no
third shape:

- On a **single-label render**, the response SHALL be `422` carrying it at the top level, as
  `error.code` `UnsupportedLayoutItem` with `error.details.reason` `circle_box_not_square`.
- In a **batch**, the response SHALL be `422 BatchInvalid` at the top level, and the failure SHALL
  appear as an entry of `details.failures` carrying `index`, `code` `UnsupportedLayoutItem` and
  `reason` `circle_box_not_square` (frozen `docs/SPEC.md` §2.1). Validate-then-execute is unchanged:
  the batch stays all-or-nothing, and a batch with any such failure SHALL produce no artifact and
  dispatch no print job.

In both, the `message` SHALL name the JSON path of the container. The `message` is prose and is not
part of the contract; the slug is.

An unknown `shape` value SHALL be refused at load, naming the value and the accepted set. `shape` on
any item other than a `container` SHALL be refused, as any other field those items do not accept
already is.

#### Scenario: The default geometry is the rectangle

- **WHEN** a container declares `background: black` and no `shape`
- **THEN** it paints the rectangle of its resolved box, exactly as it did before this key existed

#### Scenario: An ellipse fills its box and holds children

- **WHEN** a container declares `shape: ellipse`, `size: [12, 8]`, `background: black`, `padding: 1`
  and a `text` child
- **THEN** an ellipse touching all four sides of the 12-by-8 box is painted, and the text draws over
  it, inset by the padding
- **AND** this holds in PNG output and in PDF output alike

#### Scenario: A square box makes the ellipse a circle

- **WHEN** a container declares `shape: ellipse` and `size: [12, 12]`
- **THEN** the painted ellipse is a circle of 12 units across

#### Scenario: A circle is refused at load on a box that is not square

- **WHEN** a container declares `shape: circle` and `size: [14, 12]`
- **THEN** the template fails validation and is quarantined, naming the container's `size`

#### Scenario: A circle sized from its content is judged at render, not at load

- **WHEN** a container declares `shape: circle` and `size: [content, content]`, inside a frame that is
  not square
- **THEN** the template loads and is not quarantined, because neither extent is fixed by the template
- **AND** the box that resolves at render decides it: square renders the circle, non-square is refused
  `422 UnsupportedLayoutItem` with a `details.reason` of `circle_box_not_square`, naming the JSON path
  of the container
- **AND** in a batch the same failure is reported as `422 BatchInvalid` with an entry in
  `details.failures` carrying that label's `index`, the code `UnsupportedLayoutItem` and that reason,
  and the batch produces no artifact and dispatches no print job

#### Scenario: A circle sized by a parameter is judged per request

- **WHEN** a container declares `shape: circle` and `size: ["{w}", 12]`, against a declared parameter
  `w` whose `default:` is `12`
- **THEN** the template loads and is not quarantined, and a request supplying no `w` renders the
  circle at 12 by 12
- **AND** a request supplying `w: 14` is refused `422 UnsupportedLayoutItem` with a `details.reason`
  of `circle_box_not_square`, because the request's `data` outranks the default and the box it
  resolves is not square

#### Scenario: A gated-off circle is not judged at render

- **WHEN** a container declares `shape: circle`, `size: ["{w}", 12]` and `when: { badge: yes }`, and a
  request supplies `w: 14` with a `badge` value that makes the predicate false
- **THEN** the request succeeds, the container is neither measured nor rendered, and no
  `circle_box_not_square` failure is raised, because its box is never resolved
- **AND** a request whose `badge` value makes the predicate true is refused
  `422 UnsupportedLayoutItem` with that reason

#### Scenario: Squareness is judged to the service's bounds tolerance, not to exact equality

- **WHEN** a container declares `shape: circle`, `at: [0.2, 0.0]` and `to: [0.3, 0.1]`, so its width
  is a subtraction that need not compare equal to its literal height
- **THEN** it renders the circle, because the two extents differ by far less than the tolerance
- **AND** a container whose extents differ by 0.001 is refused, at load or at render as its extents'
  sources decide

#### Scenario: A rounded rectangle cuts a child at the corner

- **WHEN** a container declares `shape: rect`, `rounded: 3.0` and a child reaching into one of those
  rounded corners
- **THEN** the part of the child outside the rounded boundary is cut, exactly as the paint stops there

#### Scenario: A stroked rectangle cuts child ink at the inner edge of its stroke

- **WHEN** a container declares `shape: rect`, `stroke: { thickness: 1.0 }` and a child whose ink
  reaches the edge of the container's box
- **THEN** the child is laid out and placed exactly as it would be with no stroke, and its ink is cut
  0.5 units in from the edge, at the stroke's inner edge, so no child ink covers the stroke

#### Scenario: A child is not clipped to a round geometry

- **WHEN** a container declares `shape: ellipse`, `stroke: { thickness: 1.0 }` and a `text` child
  wide enough to cross the painted curve and reach the box edge
- **THEN** the text renders in full, clipped only by the container's rectangular box, and its ink
  draws over the stroke rather than being cut by it

#### Scenario: An unknown geometry is refused

- **WHEN** a container declares `shape: polygon` or `shape: Rect`
- **THEN** the template fails validation and is quarantined, naming the value and the accepted set

#### Scenario: A geometry on a non-container is refused

- **WHEN** a `text`, `qr`, `image` or `line` item declares `shape`
- **THEN** the template fails validation and is quarantined, naming the field and the item

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
| `rounded` | number | a shape with an interior whose geometry has corners | The corner radius. Omitted: square corners. |
| `shape` | geometry | `container` | The geometry the paint takes within the resolved box. Omitted: `rect`. |

A **colour** is what the `colour-vocabulary` capability defines, and this capability states no
vocabulary of its own: `background` accepts one of the sixteen names, a hex string, or a `"{param}"`
reference resolved per render, and the same name denotes the same colour here as on a `text` item's
`color`.

Where two keys are both accepted, neither SHALL imply the other. On a `container`, all four
combinations of `stroke` and `background` SHALL be accepted and SHALL render as declared: outline
only, fill only, both, and neither. A container with neither draws no boundary of its own and
remains a positioning and grouping construct, exactly as one with no paint does today, at every
geometry.

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

### Requirement: The corner radius is authored, not derived from the stroke

A shape with an interior **whose geometry has corners** SHALL accept `rounded: <number>`, the corner
radius in the template `unit`, applied to all four corners. `shape: rect` is that geometry; `rounded`
on `shape: ellipse` or `shape: circle` SHALL be refused at load rather than ignored, because a round
geometry has no corners to round, exactly as a `line` has none.

The radius SHALL be **finite and at least 0.0001**, by the same reasoning and the same bound as
`stroke.thickness` above: a smaller positive value is below the emitter's quantum, so it is drawn
either as a square corner or as one radius the author did not write, never as the radius declared. Square corners are spelled by omitting `rounded`, so they have exactly one spelling: a zero
radius SHALL be refused rather than accepted as a second spelling of square, and NaN, infinity, or a
positive value below 0.0001 SHALL be refused.

The radius SHALL be independent of whether the shape has a stroke, of that stroke's thickness, and of
whether the shape has a background. The same radius SHALL shape the outline and the fill, so a filled
shape with no outline rounds exactly as a stroked one does.

The same radius SHALL also govern the container's clip region, so a child reaching into a rounded
corner is cut by a rounded curve rather than drawn over the corner. The radius is not a decoration
laid over a square boundary; the geometry requirement of this capability states that rule and the
round geometries' rectangular-clip counterpart.

The clip curve and the painted curve SHALL coincide only where there is no stroke. A centred stroke
lies half outside and half inside the painted boundary, and the clip follows its **inner** edge: along
a side, half the thickness inside the box; at a corner, the stroke's inner corner curve, which lies
inside the painted curve and is tighter than it, not the same curve. `rounded` names one authored
radius, and that radius is what the paint takes; what the clip takes is the inner corner the stroke of
that thickness leaves inside it. The authored value is unchanged either way, and neither the stroke's
presence nor its thickness alters it, exactly as the paragraph above requires.

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
- **AND** a child reaching into that corner is cut on the stroke's inner corner curve, which lies
  inside the painted 2.0-unit curve, rather than on that curve itself
- **AND** the same container with no `stroke` cuts that child on the painted 2.0-unit curve, because
  with no stroke the two coincide

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

#### Scenario: A radius on a round geometry is refused

- **WHEN** a container declares `shape: ellipse` and `rounded: 1.0`, or `shape: circle` and
  `rounded: 1.0`
- **THEN** the template fails validation and is quarantined, naming `rounded` on that container

### Requirement: A container's paint covers its whole box, unrotated, behind its children

A container's `background` and `stroke` SHALL be painted on the geometry its `shape` names,
inscribed in the container's **outer** box: the rectangle its resolved placement occupies in its
parent's coordinate frame. The paint therefore extends over the `padding` band as well as the padded
inner box, because padding insets the children and not the shape. At `shape: rect` the painted
geometry is that outer box itself; at a round geometry it is the ellipse touching all four of its
sides, and the area of the box outside that curve is not painted.

Within a container, the paint SHALL be drawn in this order, back to front: the `background`, then the
`stroke`, then the container's `items`. A child therefore always draws on top of the ground it sits
on, and a filled container never hides its own contents.

The stroke SHALL be **centred on the boundary** of that geometry, so half its thickness lies outside
the geometry and half inside. That outer half SHALL NOT participate in size resolution: a stroke never grows a
shape, never insets its children **in layout**, and contributes nothing to any extent
(`layout-sizing` is unchanged by this capability). Ink falling outside the container's box SHALL be
clipped by whatever already clips the shape itself: the enclosing container's box, or the label.

Clipping is where a stroke does reach the children, and only there. At `shape: rect` a container's
clip is the boundary of the box that both paints and holds them, so the inner half of a stroke cuts
child ink, as the geometry requirement of this capability states. Every child SHALL still be laid out
and placed exactly as it would be with no stroke: the stroke changes what survives the clip, never a
coordinate, an extent or a padded inner frame.

A container's own clip region SHALL bound its children alone and SHALL NOT cut its own fill or
stroke. The outer half of a centred stroke therefore survives the container's own boundary at every
geometry, including a `rect` that clips its children to a rounded corner, and is cut only by an
ancestor or by the label.

A container's paint SHALL NOT be rotated by the container's `rotate` (frozen `docs/SPEC.md` §4.2, and
`layout-sizing`), which rotates the inner content only. The painted geometry stays axis-aligned in
the parent frame at every rotation, so a rotated container with `shape: ellipse` paints an ellipse on
the axes of the parent frame rather than one turned with its contents.

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
- **AND** the children of the stroked one are placed at the same coordinates and resolve to the same
  extents as the children of the unstroked one, whatever the clip then cuts

#### Scenario: A stroke is clipped at the boundary that clips the shape

- **WHEN** a container flush against the label's left edge declares `stroke: { thickness: 1.0 }`
- **THEN** the outer half of that stroke falls outside the label and is clipped, and the label's own
  dimensions are unchanged
- **AND** a container carrying children, and therefore clipping them, still draws that outer half
  outside its own boundary rather than cutting it there

#### Scenario: Rotation does not rotate the paint

- **WHEN** a container declares `rotate: 90`, `background: black` and a text child
- **THEN** the black rectangle stays axis-aligned in the parent frame while the text renders rotated

#### Scenario: A gated-off shape paints nothing

- **WHEN** a container declares `when: { outline: yes }` with `stroke: { thickness: 0.02 }`, and the
  request resolves `outline` to any other value
- **THEN** neither an outline nor a fill is drawn

#### Scenario: A round geometry leaves the corners of its box unpainted

- **WHEN** a container declares `shape: ellipse` and `background: black` over a non-black ground
- **THEN** the four corner regions of its box, outside the painted curve, show that ground rather
  than black
