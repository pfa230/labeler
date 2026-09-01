## MODIFIED Requirements

<!-- The scenario "A zero-extent child still draws its frame and still raises its errors" keeps that
     title deliberately, though its body now says `stroke` and `frame` is removed by `shape-paint`.
     OpenSpec compares a MODIFIED requirement's scenarios by name, so renaming one reports it as a
     scenario this delta drops and the change is refused. Do not "fix" the title here; rename it in a
     later change against the archived requirement. -->

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
`stroke`, `background`, `rounded` (`shape-paint`), `padding`, its own `items` and its own `flow`
block.

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

- **WHEN** a `row` flow container holds two `container` children that each carry a `stroke` and
  `padding` and neither `size` nor `to`
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

- the first occupying child of a **line** SHALL have its leading edge at the padded inner box's
  leading edge on the primary axis;
- each later occupying child on that line SHALL have its leading edge one `gap` past the previous
  occupying child's trailing edge.

A `gap` SHALL therefore fall only between two occupying children **on the same line**, and a line
SHALL carry no leading or trailing gap.

When `wrap` is `false` there is exactly one line, which is the arrangement as it stood before this
requirement gained lines. When `wrap` is `true`, an occupying child whose **box** does not fit the
room left on the current line SHALL begin a new line, and it SHALL be that line's first occupying
child. Line breaking reads the child's **box**, the quantity the packing positions, and never its
requirement, the quantity the assembled extent is built from: those two are one number for an author
or content source and differ for a frame source, and it is the box that must physically fit.

A line has **two** secondary extents, for the same reason a child has two, and each is used where its
kind is already used:

- a line's **box extent** is the largest secondary *box* among the children drawn on it. It is
  physical, so it decides where the next line begins, and therefore the secondary position every child
  on that next line is given;
- a line's **requirement extent** is the largest secondary *requirement* among those same children. It
  is reported, so it is what the assembled extent below is built from.

For a line whose children are all author or content sources the two are one number. They **may** differ
when a line holds a frame source, and only then: a `fill` child whose bounded intrinsic happens to
equal the extent it was given reports and takes the same number. That possibility is `fill`'s declared
asymmetry (`layout-sizing`) and not a rule of this capability.

Each later line's leading edge on the secondary axis SHALL be one `line_gap` past the previous line's
**box** trailing edge, so a `line_gap` falls only between two lines and a container carries no leading
or trailing one.

A child that occupies nothing SHALL belong to the line that is current when the arrangement reaches
it. Wrapping is decided only by occupying children, so such a child never triggers a break and never
follows one: it stays where template order put it, on the line before the break rather than after.
Within that line it is placed exactly as this requirement already says, at the leading edge the next
occupying child on that line would take. Only its **line membership** is new here, because once lines
exist the next occupying child may be on a different one, and the merged rule alone does not say which
line such a child belongs to.

A child SHALL NOT be broken across lines. Wrapping chooses which line a child sits on; what a `text`
does inside the box it was given is settled by its own `overflow` policy (ADR-0082) and is not the
arrangement's business.

An **uncapped** `fill` child's box is the whole padded inner extent (`layout-sizing`), so under
`wrap: true` it takes a line of its own whenever anything precedes it on the current line, and the
child after it begins another. That is the ordinary rule applied to a box the width of the line, not a
case of its own. A `fill` child carrying `max_w` or `max_h` on that axis is bounded by its cap, as the
merged requirement already says, so it is `min(inner, cap)` wide and shares a line like any other
child.

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
empty box", kept rather than excepted: the only ink such a box can put on a label is the container's
own `stroke` (`shape-paint`), and that stroke is still drawn.

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

- **WHEN** a `row` flow container holds a `content`-width `container` child carrying a `stroke`, an
  authored height of 6 and no active children of its own
- **THEN** that child is drawn at the current leading edge as a zero-width box, so its stroke
  appears, and the line is at least 6 tall
- **AND** a `text` child whose value interpolates a field the request does not supply still fails with
  `MissingField` whether its extent resolved to zero or not

#### Scenario: Reordering packed children reorders the label

- **WHEN** two packed children of a `row` flow container are swapped in the template
- **THEN** they are drawn in the new order, because template order is packing order
- **AND** the container's assembled extent is unchanged

#### Scenario: A row wraps to a second line

- **WHEN** a `row` flow container with `wrap: true`, `gap: 2`, `line_gap: 1`, an authored padded inner
  width of 20 and three children resolving to 8 wide and 4 tall
- **THEN** the first two sit on the first line, the second's left edge 2 right of the first's right
  edge
- **AND** the third's left edge is the inner box's left edge and its top edge is 1 below the first
  line's bottom edge

#### Scenario: A line's extent is its tallest drawn child

- **WHEN** a wrapping `row` container's first line holds children resolving to 4 and 9 tall
- **THEN** that line's extent is 9, and the next line begins one `line_gap` below 9

#### Scenario: A line's two extents differ when it holds a fill child

- **WHEN** a wrapping `row` container 20 wide and 30 tall holds a `size: [20, fill]` text whose
  intrinsic height is 4, then a 1 by 4 child, with `line_gap: 1`
- **THEN** the first line's box extent is 30 and its requirement extent is 4
- **AND** the second line begins one `line_gap` past 30, so it lies outside the box and is governed by
  the overflow requirement
- **AND** the container's assembled secondary extent counts 4 for that line, not 30

#### Scenario: An uncapped fill child takes a line of its own

- **WHEN** a wrapping `row` container with an authored inner width of 20 holds a `content`-width child
  resolving to 6, then a `size: [fill, 4]` child, then another `content`-width child
- **THEN** the `fill` child's box is 20 wide, which does not fit the room left after the first child,
  so it begins the second line and fills it
- **AND** the third child begins a third line

#### Scenario: A capped fill child shares a line

- **WHEN** that middle child instead declares `size: [fill, 4]` with `max_w: 8`, `gap: 2`
- **THEN** its box is 8 wide and it sits on the first line, its leading edge 2 past the first child
- **AND** the third child follows it on that line if the room left admits it

