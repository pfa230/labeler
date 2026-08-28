## Purpose

Defines the flow arrangement: how a container packs its children in order instead of placing each one
by its own coordinate. An arrangement decides **position and nothing else**; how big a box is stays
owned by `layout-sizing`. This capability covers what a `flow` block declares, what a packed child may
and may not carry, which children take up room along the packing axis, what the container assembles
from the result, and what happens when the packing runs past the padded inner box.

## ADDED Requirements

### Requirement: A `flow` block selects the flow arrangement

A `container` MAY carry a `flow` block. Its presence, and nothing else, selects the flow arrangement
for that container's children. A container without `flow` keeps the absolute arrangement and is
unchanged in every respect.

The block SHALL be:

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `direction` | `row` or `column` | required | The **primary** axis, the one children are packed along. |
| `gap` | number, at least 0 | `0` | The space between two adjacent children along the primary axis, in template units. |

`direction` names the primary axis the way reading direction is primary for the words in a line. The
other axis is the **secondary** one. `direction: row` SHALL make the horizontal axis primary and pack
along `+x` from the padded inner box's **left** edge. `direction: column` SHALL make the vertical axis
primary and pack along `−y` from its **top** edge: coordinates are bottom-left origin and y-up
(`docs/SPEC.md` §6), so a column advances by decreasing `y` and its first child is the topmost.

`direction` SHALL be required rather than defaulted, so `flow: {}` cannot select an arrangement by
accident. The service SHALL refuse the template at load, quarantining it and naming the JSON path of
the offending key, when `direction` is absent or is neither `row` nor `column`, or when `gap` is
negative or non-finite.

Every child SHALL be aligned to the padded inner box's leading edge on the **secondary** axis: its top
edge for a `row`, its left edge for a `column`. This capability offers no secondary-axis alignment
control.

A `container` carrying both `flow` and `rotate` SHALL pack in **author space**, as every other layout
decision beneath a rotation does (`layout-sizing`). `direction` therefore names an author axis, and
the assembled extent swaps with the rest of the author-space aggregate on the way out.

This requirement adds the `flow` key to the `container` field list of the frozen `docs/SPEC.md` §4.1
and supersedes that clause to that extent. Every other field it lists, and every other statement in
§4.1, remains authoritative.

#### Scenario: A container without a flow block is unaffected

- **WHEN** a template contains a `container` with no `flow` key
- **THEN** its children are placed by their own `at`/`to` exactly as before this change
- **AND** the emitted Typst source is byte-identical to the source before this change

#### Scenario: A flow block without a direction is refused

- **WHEN** a template declares `flow: { gap: 2 }` on a container
- **THEN** the template fails validation and is quarantined
- **AND** the message names the JSON path of that `flow` block

#### Scenario: A negative gap is refused

- **WHEN** a template declares `flow: { direction: row, gap: -1 }`
- **THEN** the template fails validation and is quarantined, naming the JSON path of `gap`
- **AND** `gap: 0` and an absent `gap` are accepted and mean the same thing

#### Scenario: A column packs downward from the top edge

- **WHEN** a flow container with `direction: column` and `gap: 2` holds three children resolving to 4
  tall in a padded inner box 20 tall
- **THEN** the first child's top edge is the inner box's top edge
- **AND** each later child's top edge is 2 below the previous child's bottom edge
- **AND** each child's left edge is the inner box's left edge

#### Scenario: A quarter turn packs in author space

- **WHEN** a flow container declares `rotate: 90` and `direction: row`
- **THEN** its children are packed along the author canvas's horizontal axis, which is its parent's
  vertical one
- **AND** its assembled extent swaps with the rest of its author-space aggregate, exactly as an
  absolutely arranged rotated container's does

### Requirement: A packed child carries no position and is sized by `layout-sizing` alone

A **packed child** is a direct child of a container carrying a `flow` block. Its box SHALL be sized by
`layout-sizing` against that container's **padded inner box**, exactly as any child of any container
is, and the arrangement SHALL then position that box without altering it.

Two refusals follow, each at load with the JSON path of the offending child:

1. **No position.** A packed child SHALL NOT carry `at` or `to`. The container decides where it goes,
   so a coordinate on it has no meaning to honour or to ignore.
2. **No `line`.** A `line` is contribution-only and is never asked for an intrinsic size
   (`layout-sizing`), so it has no box to pack. A `line` inside an absolutely arranged container is
   unchanged.

A packed child's **available extent** on each axis SHALL be the container's padded inner extent, which
is what `layout-sizing` gives an item with no anchor. Every rule keyed to the source then applies
unchanged, on both axes: an authored extent is checked and refused where it does not fit that box, a
content or frame extent is clamped to it, `max_w` and `max_h` bind the clamped ones and are inert on
the authored one, and a `text` is laid out against the box `layout-sizing` already hands it. Nothing
about a packed child is sized differently from the same item at `at: [0, 0]` in an absolutely arranged
container of the same padded inner box.

**`fill` needs no rule here and gets none.** `layout-sizing` says a frame extent reports
`min(intrinsic, max_*, available)` upward and takes the available extent downward, and that asymmetry
is what `fill` means. On a packed child it therefore reports what the child measures and takes the
container's padded inner extent, on either axis, with no reference to the arrangement. The consequence
SHALL be stated rather than refused:

- a `fill` child **alone** in its container reports its own intrinsic upward and is drawn at the
  container's inner extent, so under a `content`-sized container it is drawn at its own intrinsic size
  and under a sized one it stretches to fill it, which is what `fill` does everywhere else;
- a `fill` child **beside a sibling** takes the whole inner extent on that axis, so the packing puts
  one of them past the padded inner box and the render fails by the overflow requirement below. That
  is loud and it is correct: the author asked for the whole extent and got it.

`max_w` and `max_h` bind a frame extent like any other clamped one (`layout-sizing`), so a capped
`fill` child takes `min(inner extent, cap)` and not the whole inner extent. A `size: [fill, 4]` child
with `max_w: 10` in a 30-wide inner box is 10 wide and shares its line like any other 10-wide child.
The consequence above is therefore the **uncapped** case, and the capped one is not an exception to it
but the cap rule doing what it already does.

Giving `fill` on the primary axis the other meaning, the room the arrangement has left, is
[#260](https://github.com/pfa230/labeler/issues/260). It needs a contract for the circularity that
meaning creates, because a child whose box arrives from the arrangement cannot first report the extent
the arrangement needs in order to compute that box.

A packed child MAY carry every other key its item type allows: `size` with a number, a parameter
reference, `content` or `fill`; `max_w`; `max_h`; `when`; and on a `container` child `rotate`,
`frame`, `padding`, its own `items` and its own `flow` block.

A packed `container` that gives neither `size` nor `to` SHALL default to `size: [fill, fill]`,
exactly as any other container does (`layout-sizing`). No separate default is invented for a packed
one, because that would make the same spelling resolve differently according to which container it
sits in, and the reader would have to know the parent to read the child. The consequence follows from
the `fill` rule above and is stated rather than defaulted away: two such containers packed side by side
each take the whole padded inner extent, so the second is positioned past it and the render fails with
`item_out_of_frame`. A packed container meant to hug its own children says `size: [content, content]`,
and one meant to take a fixed slot says a number. The failure is loud, names the second container, and
is fixed by one line.

A packed child SHALL be represented as it was authored, carrying neither `at` nor `to`. A
representation supplying a default anchor would return, from `GET /api/templates/{id}`, a spelling
this requirement refuses on the way in, so reading a template and submitting it unchanged would fail.
Every item that is not a packed child SHALL continue to carry its anchor in that response exactly as
it does today.

This requirement supersedes the `at` and `to` rows of the frozen `docs/SPEC.md` §4 placement table for
packed children only, and states in full what replaces them: a packed child has neither. For every
item that is not a packed child, both rows remain authoritative exactly as written.

#### Scenario: A packed child with an explicit position is refused

- **WHEN** a child of a flow container declares `at: [2, 2]`
- **THEN** the template fails validation and is quarantined
- **AND** the message names the JSON path of that child's `at`
- **AND** a child declaring `to: [10, 4]` is refused the same way

#### Scenario: A line cannot be packed

- **WHEN** a child of a flow container is a `line` item
- **THEN** the template fails validation and is quarantined
- **AND** the message names the JSON path of that child

#### Scenario: A packed child is sized exactly as an unpacked one

- **WHEN** a `content`-sized `text` with a `font_size` range sits in a flow container whose padded
  inner box is 30 by 10
- **THEN** it is laid out against 30 by 10 and its box is the result, exactly as the same item at
  `at: [0, 0]` in a 30 by 10 absolutely arranged container
- **AND** its `overflow` policy is enforced against that same box

#### Scenario: A lone fill child behaves as fill does anywhere

- **WHEN** a `row` flow container with `size: [40, 6]` and no padding holds one child with
  `size: [fill, 4]`
- **THEN** that child is drawn 40 wide
- **AND** the same child in a `content`-width flow container is drawn at its own intrinsic width,
  because that is what the container sized itself to

#### Scenario: A fill child beside a sibling overflows

- **WHEN** a `row` flow container with a padded inner width of 30 and `gap: 2` holds a
  `content`-sized child resolving to 10 wide, then a child with `size: [fill, 4]`
- **THEN** the second child's box is 30 wide and its leading edge is at 12
- **AND** the render fails with `UnsupportedLayoutItem` and `details.reason` of `item_out_of_frame`

#### Scenario: A packed container with no size fills, and two of them collide

- **WHEN** a `row` flow container holds two `container` children that each carry `frame` and `padding`
  and neither `size` nor `to`
- **THEN** each resolves to `size: [fill, fill]`, so each is the whole padded inner box
- **AND** the render fails with `UnsupportedLayoutItem` and `details.reason` of `item_out_of_frame`,
  naming the second container
- **AND** the same two containers spelling `size: [content, content]` pack side by side and render

#### Scenario: A packed child round-trips without an anchor

- **WHEN** a template containing a flow container is read back through `GET /api/templates/{id}`
- **THEN** its packed children carry neither `at` nor `to` in the response
- **AND** submitting the returned document unchanged is accepted
- **AND** every item outside a flow container still carries its `at`, including one whose `at` the
  author omitted

### Requirement: Packing places the children that take up room along the primary axis

Each child of a flow container supplies **two** quantities, and this capability uses each exactly
where `layout-sizing` already puts it. A child's **requirement** is what it reports upward, and the
assembled extent below is built from requirements, as the absolute arrangement's largest-requirement
rule is. A child's **box** is what it takes downward, and the packing positions and draws boxes. For
an author or content source the two are one number. For a frame source they are not, and that
asymmetry is what `fill` means (`layout-sizing`); this capability neither adds to it nor works around
it.

A child **occupies** the packing axis when it is **active** and its box's primary extent is greater
than zero. Its **box** is the extent it resolved to against the frame it was sized against, and a flow
container is sized before its own extent is known, so that frame is its **provisional** padded inner
box while the container is being assembled, the one its unmeasured extent gives less its padding, and
its resolved padded inner box once it has one. For an
author or a content source the two give the same number, so occupancy and the extent are the same at
both. For a **frame** source they need not, because a frame extent *is* the frame it is given: a
`fill` child is the whole extent of whichever box it was sized against, so it occupies at both, and
the number it occupies with differs. That is `fill`'s declared asymmetry
(`layout-sizing`) and not a rule of this capability; the assembled-extent requirement below says what
it does to a container. The arrangement SHALL place children in template order, and:

- the first occupying child's **leading** edge SHALL be the padded inner box's leading edge on the
  primary axis;
- each later occupying child's leading edge SHALL be one `gap` past the previous occupying child's
  trailing edge.

A `gap` SHALL therefore fall only between two occupying children, and a container SHALL carry no
leading or trailing gap.

A child whose `when` gate does not match (`docs/SPEC.md` §5) SHALL occupy nothing, be drawn nothing,
and contribute nothing, so the children after it close the hole rather than leaving one. This is
`layout-sizing`'s existing rule that an inactive item imposes no requirement and is never asked for an
intrinsic size, read through an arrangement.

An **active** child whose box's primary extent is zero SHALL occupy nothing and SHALL still be drawn,
at the leading edge the next occupying child would take. It advances nothing and consumes no `gap`,
because a `gap` is the space between two adjacent children and a child with no extent along that axis
separates nothing from nothing. Emitting a gap on each side of it would lay down twice the space the
author wrote. It SHALL contribute its secondary extent like any other drawn child, and it SHALL be
sized and evaluated like any other child, so its own errors still surface. This keys on the child's
resolved extent and never on whether a value was empty: an empty interpolated value, a parameter
resolving to zero and a `content` container with no active children all reach it the same way. It is
also `layout-sizing`'s standing promise that "a content or frame extent of exactly zero renders an
empty box", kept rather than excepted: the only ink such a box can put on a label is a container
`frame` stroke, and that stroke is still drawn.

#### Scenario: A gated-off child leaves no hole

- **WHEN** a flow container with `direction: column` and `gap: 1` holds three text children resolving
  to 3 tall and the middle child's `when` gate does not match
- **THEN** two children render
- **AND** the second rendered child's top edge is 1 below the first child's bottom edge, the position
  it would occupy if the gated-off child were absent from the template

#### Scenario: An empty value leaves no double gap

- **WHEN** a `row` flow container with `gap: 2` holds three `content`-width text children and the
  middle child's interpolated value is empty
- **THEN** the third child's left edge is exactly 2 right of the first child's right edge
- **AND** the middle child is drawn at the third child's left edge, occupying no width

#### Scenario: A zero-extent child still draws its frame and still raises its errors

- **WHEN** a `row` flow container holds a `content`-width `container` child carrying a `frame`, an
  authored height of 6 and no active children of its own
- **THEN** that child is drawn at the current leading edge as a zero-width box, so its frame stroke
  appears, and the line is at least 6 tall
- **AND** a `text` child whose value interpolates a field the request does not supply still fails with
  `MissingField` whether its extent resolved to zero or not

#### Scenario: Reordering packed children reorders the label

- **WHEN** two packed children of a `row` flow container are swapped in the template
- **THEN** they are drawn in the new order, because template order is packing order
- **AND** the container's assembled extent is unchanged

### Requirement: The assembled extent is what a flow container reports

A flow container's **assembled extent** SHALL be, before padding:

- on the **primary** axis, the sum of the requirements of the children occupying that axis, plus one
  `gap` between each adjacent pair of them;
- on the **secondary** axis, the largest requirement among its active children.

Its intrinsic size SHALL be that assembled extent plus its padding on each axis, which is what
`layout-sizing`'s amended intrinsic requirement names. Everything downstream follows from
`layout-sizing` unchanged: `content` on the container resolves to that intrinsic clamped to its
available extent, `fill` on the container reports it bounded and takes the frame, an authored extent
is used as written and the arrangement packs into the padded inner box it defines, and `max_w` and
`max_h` bind the first two and are inert on the third. `fill` on the flow container **itself** is the
ordinary frame source it always was: its frame is its parent's, and this requirement changes nothing
about it.

The assembled extent is built from requirements, and the packing advances by boxes, so it SHALL be
read as what the children **need** rather than as what they will occupy. The two agree for every
author and content source. For a frame source they can disagree in either direction, and the
disagreement is `fill`'s declared asymmetry rather than anything this capability adds: a `fill` child
reports its own bounded intrinsic upward and takes the padded inner extent downward
(`layout-sizing`). A `content`-sized flow container holding a `fill` child beside any sibling
therefore assembles to less than it then packs, and the packing overruns exactly as the sizing
requirement above says it does. A `fill` child whose intrinsic is zero, such as a text bound to an
empty value, is the sharpest case: it contributes nothing to the assembled extent and still takes the
whole inner extent, so it still occupies and still overruns. None of this is a second rule; it is the
one consequence of `fill` on a packed child, seen from the container's side.

A flow container SHALL nest. A flow container that is itself a packed child reports its assembled
extent plus its padding, and its parent packs it at that.

When no child occupies the primary axis and none is drawn, the assembled extent SHALL be zero on both
axes, so a `content` axis resolves to the container's padding alone, a container with no padding
resolves to zero, and either renders an empty box. This is the outcome `layout-sizing` already gives a
zero inner box, reached from the other side, and SHALL NOT be an error.

Nothing about the arrangement depends on the template's format. A flow container SHALL pack the same
way at the root of a `sheet` slot, on a fixed-width `single` and on a dynamic-width `single`, its
frame being whichever one it was given.

#### Scenario: A content-sized flow container hugs its children

- **WHEN** a flow container declares `size: [content, content]`, `direction: row`, `gap: 2`,
  `padding: 1` and holds two children resolving to 10 and 6 wide and 4 tall
- **THEN** its width is `1 + 10 + 2 + 6 + 1 = 20`
- **AND** its height is `1 + 4 + 1 = 6`

#### Scenario: A flow container sizes a dynamic-width label

- **WHEN** a dynamic-width `single` with `width: { min: 10, max: 120 }` carries only a flow container
  at `at: [0, 0]` with `size: [content, content]` whose children assemble to 34 wide
- **THEN** the label's width is 34

#### Scenario: A flow container is the root of a sheet slot

- **WHEN** a `sheet` template's layout is a single flow container with `size: [content, content]`
- **THEN** it packs and renders by the same rules, its frame being the slot

#### Scenario: A nested flow container is packed at its assembled extent

- **WHEN** a packed child of a `row` flow container is itself a `column` flow container with
  `size: [content, content]` whose own children assemble to 8 wide and 14 tall
- **THEN** it is packed as an 8 by 14 box
- **AND** the outer container's assembled extent counts 8 for it on the primary axis

#### Scenario: A fill child makes the assembly understate the packing

- **WHEN** a `content`-width `row` flow container with `gap: 2` holds a `content`-width text resolving
  to 10 wide, then a `size: [fill, 4]` child whose own intrinsic width is 0 because its value is empty
- **THEN** the `fill` child occupies, because the box it was sized against while the container was
  assembled is that container's available extent and is not zero, so the `gap` before it counts
- **AND** the assembled width is `10 + 2 + 0 = 12`, so the container resolves to 12 wide
- **AND** the second child's box is the whole 12-wide inner extent, so it is positioned at 12 and the
  render fails with `item_out_of_frame`
- **AND** the same container holding only the `fill` child renders, because the assembly and the
  packing then agree at 0

#### Scenario: Every child gated off leaves a padding-sized container

- **WHEN** every child of a `content`-sized flow container with `padding: 1` has a `when` gate that
  does not match
- **THEN** the render succeeds and the container resolves to 2 on each axis, its padding alone

### Requirement: Packing past the padded inner box fails where it lands

A packed child SHALL be checked against the padded inner box twice, and both checks are **anchor-free**
because a packed child has no anchor to check:

1. **Its own extents**, at load and at render. Each resolved extent SHALL be no larger than the padded
   inner extent on its axis. A content or frame extent can never fail this, because `layout-sizing`
   clamps it there, and an extent authored in the template cannot either, because load refuses it
   where it is written. An extent authored through a **parameter** can: load validates geometry
   against the parameter's instantiated default, and a request may supply a larger value.
2. **Its arranged box**, at render only. The position the arrangement gives the child SHALL put its
   whole box inside the padded inner box on both axes. What the arrangement can put outside is the
   **accumulation**: the sum of the children's primary extents and the gaps between them.

Check 1 SHALL be evaluated before check 2, so a single child too large for the box is reported as
itself rather than as whatever the accumulation then does to it.

Both SHALL fail with `UnsupportedLayoutItem` and `details.reason` of `item_out_of_frame`, on either
axis and whichever edge lies outside. `coord_out_of_frame` SHALL NOT be raised for a packed child:
that slug reports a coordinate resolving outside its frame (frozen `docs/SPEC.md` §10.1), and a packed
child has no coordinate. This is stated because the two edges are not symmetric in a bottom-left,
y-up coordinate system: a `row` aligns its children to the padded inner box's **top** edge and a
`column` packs downward from it, so a child too tall for the box, and a column that overruns, hang
below the frame's origin rather than past its far edge. Feeding an arrangement-supplied position
through the anchored-placement path would report those as a coordinate outside its frame and the
`row` overrun as an item outside its frame, splitting one event into two slugs by direction. It is one
event: the arrangement could not fit the child, and no reason is added for it.

Whether an author can ask for that child to be dropped instead is
[#212](https://github.com/pfa230/labeler/issues/212), which adds `wrap` so a child that does not fit
starts a new line and an `overflow` policy so one that still does not fit can be trimmed. Until then
a flow container has one line and an overrun is an error.

#### Scenario: An accumulated overrun fails the render

- **WHEN** a `row` flow container with a padded inner width of 20 and `gap: 2` packs children
  resolving to 12 and 9 wide
- **THEN** the second child's leading edge is 14 and its trailing edge is 23
- **AND** the render fails with `UnsupportedLayoutItem` and `details.reason` of `item_out_of_frame`

#### Scenario: A single child too large for its box fails as itself

- **WHEN** a packed child declares `size: ["{box_w}", 4]` in a flow container whose padded inner box
  is 20 wide, and `box_w` defaults to 10
- **THEN** the template loads
- **AND** a request supplying `box_w: 30` fails with `item_out_of_frame` for that child, whether or
  not it has any sibling

#### Scenario: An authored extent too large for the inner box is still refused at load

- **WHEN** a packed child declares `size: [30, 4]` in a flow container whose padded inner box is 20
  wide
- **THEN** the template fails validation and is quarantined, exactly as the same child would be in an
  absolutely arranged container

#### Scenario: A secondary-axis overrun fails the same way

- **WHEN** a `row` flow container with a padded inner height of 6 packs a child whose height a
  parameter resolves to 9
- **THEN** the render fails with `UnsupportedLayoutItem` and `details.reason` of `item_out_of_frame`
- **AND** a `column` flow container with a padded inner height of 20 and `gap: 2` packing three
  children resolving to 8 tall fails the same way, with the same slug, on the third child
- **AND** neither raises `coord_out_of_frame`, although both put a box edge below the frame's origin
