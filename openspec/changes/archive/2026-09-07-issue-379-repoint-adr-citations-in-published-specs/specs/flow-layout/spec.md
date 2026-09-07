## MODIFIED Requirements

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
does inside the box it was given is settled by its own `overflow` policy
(`openspec/changes/archive/2026-08-27-issue-226-unify-size-resolution`) and is not the arrangement's
business.

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
   **accumulation**: the primary extents and gaps of the children sharing its line, and, when `wrap`
   is `true`, the **box** extents and `line_gap`s of the lines stacked before its own. Those stacked
   line box extents decide where the child sits on the secondary axis; what check 2 then tests is the
   child's own box at that position, never a line extent.

Check 1 SHALL be evaluated before check 2, so a single child too large for the box is reported as
itself rather than as whatever the accumulation then does to it.

Check 1 SHALL always fail. Check 2 SHALL be decided by the container's `overflow` policy:

- **`fail`**, the default, SHALL fail the render, which is the behaviour before this policy existed.
- **`trim`** SHALL leave the first child that fails check 2 undrawn, and every child after it in
  template order, whether or not a later one would have fitted. Packing stops there. Check 2 SHALL
  then raise nothing, so the overrun itself does not fail the render; whether the render succeeds
  still depends on every other rule that applies to the template, including the sizing and evaluation
  of the children that were trimmed.

A trim SHALL leave no mark on the label and SHALL NOT be reported to the caller. `fail` is the default
for exactly that reason, and it differs from the `ellipsis` default a `text` carries
(`openspec/changes/archive/2026-08-27-issue-226-unify-size-resolution`) on the same ground: an
ellipsis is visible on the printed label and a dropped child is not.

**A trim removes a child from the drawing and from the assembled extent, and from nothing else.**
In particular it does not exempt a child from being sized: every active child SHALL be sized and
evaluated as `layout-sizing` requires whether or not it is trimmed, so what a trimmed child still
raises is what sizing demands of it and nothing more: an active `text` is laid out and enforces its
own policy, a `content` or `fill` sized `qr` or `image` is asked for its intrinsic size and can raise
`qr_generation_failed`, `intrinsic_size_undefined` or an `image_*` reason, and an **authored**-size
`qr` or `image` is asked for nothing and so raises nothing, because its payload and its bytes are read
only when it is drawn. That asymmetry is `layout-sizing`'s, visible here only because trimming is the
first thing that skips drawing.

That is why a trimmed child contributes to neither the drawing nor the assembled extent while still
being able to raise. Dropping it from the assembly can never change the container's own size: `overflow: trim` is permitted only where the axis it acts on is
resolved, so the assembled extent on that axis determines nothing.

Check 1, and check 2 under `overflow: fail`, SHALL fail with `UnsupportedLayoutItem` and
`details.reason` of `item_out_of_frame`, on either axis and whichever edge lies outside. Check 2 under
`overflow: trim` raises nothing, because the child it would have reported is not drawn. `coord_out_of_frame` SHALL NOT be raised for a packed child:
that slug reports a coordinate resolving outside its frame (frozen `docs/SPEC.md` §10.1), and a packed
child has no coordinate. This is stated because the two edges are not symmetric in a bottom-left,
y-up coordinate system: a `row` aligns its children to the padded inner box's **top** edge and a
`column` packs downward from it, so a child too tall for the box, and a column that overruns, hang
below the frame's origin rather than past its far edge. Feeding an arrangement-supplied position
through the anchored-placement path would report those as a coordinate outside its frame and the
`row` overrun as an item outside its frame, splitting one event into two slugs by direction. It is one
event: the arrangement could not fit the child, and no reason is added for it.

With `wrap: true` a child that does not fit the room left on its line starts a new one, so an overrun
on the primary axis is confined to a child whose box is the whole inner extent, and what can overrun
is the stack of lines on the secondary axis.

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

#### Scenario: Trim drops the child that does not fit and every child after it

- **WHEN** a `row` flow container declares `overflow: trim`, `gap: 2`, an authored padded inner width
  of 20, and packs children resolving to 8, 8 and 2 wide
- **THEN** the first two children are drawn and the third is not, even though 2 units remained
- **AND** the render succeeds

#### Scenario: Trim drops a line that does not fit

- **WHEN** a wrapping `row` flow container with an authored padded inner box 20 wide and 9 tall,
  `line_gap: 1`, packs children into three lines of extent 4, 4 and 4
- **THEN** the first two lines are drawn, occupying `4 + 1 + 4 = 9`
- **AND** under `overflow: trim` the third line's children are not drawn and the render succeeds
- **AND** under `overflow: fail` the render fails with `item_out_of_frame`

#### Scenario: A trimmed child still raises what sizing demands of it

- **WHEN** a flow container with `overflow: trim` would trim a `content`-width `text` whose value
  interpolates a field the request does not supply
- **THEN** the render fails with `MissingField`
- **AND** a trimmed `image` with `size: [20, 10]` whose `name` names an absent data key raises
  nothing, because sizing never reads its bytes and drawing is what would have

#### Scenario: A single child too large still fails under trim

- **WHEN** a flow container with `overflow: trim` packs a child whose own resolved extent exceeds the
  padded inner box, from a parameter a request supplied
- **THEN** the render fails with `item_out_of_frame`, because check 1 is evaluated before check 2 and
  the policy governs only check 2
