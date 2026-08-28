## Purpose

Defines the flow arrangement: how a container packs its children in order instead of placing each by
its own coordinate. An arrangement decides **position and nothing else**; how big a box is stays owned
by `layout-sizing`. This capability covers what a `flow` block declares, what a packed child may and
may not carry, how lines are chosen, what the container assembles from the result, and what happens
when the packed content does not fit.

## ADDED Requirements

### Requirement: A `flow` block selects the flow arrangement

A `container` MAY carry a `flow` block. Its presence, and nothing else, selects the flow arrangement
for that container's children. A container without `flow` keeps the absolute arrangement and is
unchanged in every respect.

The block SHALL be:

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `direction` | `row` \| `column` | — (required) | The main axis children are packed along. |
| `gap` | number ≥ 0 | `0` | Space between two adjacent children on the same line, in template units. |
| `wrap` | boolean | `false` | Whether a child that does not fit the room left on its line starts a new one. |
| `line_gap` | number ≥ 0 | `0` | Space between two adjacent lines, in template units. |
| `overflow` | `fail` \| `trim` | `fail` | What happens to a child that does not fit. |

The service SHALL refuse the template at load, quarantining it with the JSON path of the offending
key, when `direction` is absent or is neither `row` nor `column`, or when `gap` or `line_gap` is
negative or non-finite.

`line_gap` with `wrap: false` SHALL be **inert**, not refused. A container that never wraps has one
line, so the value separates nothing, exactly as `gap` separates nothing in a container with fewer
than two occupied slots. Refusing it would be a rule with no invariant behind it.

`direction: row` SHALL pack along +x from the padded inner box's **left** edge, and its main axis is
the horizontal one. `direction: column` SHALL pack along −y from the **top** edge, and its main axis
is the vertical one: the coordinate system is bottom-left origin, y-up (`docs/SPEC.md` §6), so a
column advances by decreasing `y` and its first child is the topmost.

Within a line, every child SHALL be aligned to the line's start edge on the cross axis: the line's top
edge for a row, its left edge for a column. This capability offers no cross-axis alignment control.

A `container` carrying both `flow` and `rotate` SHALL pack in **author space**, as every other layout
decision beneath a rotation does (`layout-sizing`). `direction` therefore names an author axis, and
the assembled extent is swapped with the rest of the author-space aggregate on the way out.

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
- **AND** the message names the JSON path of the `flow` block's missing `direction`

#### Scenario: line_gap without wrap is accepted and does nothing

- **WHEN** a template declares `flow: { direction: row, line_gap: 1 }` with `wrap` absent or `false`
- **THEN** the template loads
- **AND** the rendered layout is identical to the same template with no `line_gap`

#### Scenario: A column packs downward from the top edge

- **WHEN** a flow container with `direction: column` and `gap: 2` holds three children of height 4 in
  a padded inner box 20 tall
- **THEN** the first child's top edge is the inner box's top edge
- **AND** each later child's top edge is 2 below the previous child's bottom edge

### Requirement: A packed child's box comes from the child, never from the arrangement

A **packed child** is a direct child of a container carrying a `flow` block. Its box SHALL be sized by
`layout-sizing` alone, against the container's **padded inner box**, exactly as any child of any
container is. The arrangement SHALL then position that box and SHALL NOT alter it.

Two refusals follow, each at load with the JSON path of the offending child:

1. **No position.** A packed child SHALL NOT carry `at` or `to`. The arrangement decides where it
   goes.
2. **No `line`.** A `line` is contribution-only and is never asked for an intrinsic size
   (`layout-sizing`), so it has no box to place. A `line` inside an absolutely arranged container is
   unchanged.

**`fill` is permitted and means the whole padded inner extent**, the same thing it means for any other
child of that container, because a packed child's available extent is that inner box. It does not mean
the room left on the line: the arrangement never supplies a child's box, so there is nothing for a
frame source to consult but the frame it was given.

The consequence SHALL be stated rather than smoothed over. A `fill` packed child occupies a whole
line, so any sibling sharing that line puts the line total past the inner box and is governed by the
overflow requirement, which under the default `overflow: fail` fails the render. That is loud and it is
correct: the author asked for the whole extent and got it. A lone `fill` child, or one whose siblings
are trimmed, renders normally. Giving `fill` the other meaning, the room the arrangement has left, is
[#260](https://github.com/pfa230/labeler/issues/260); it needs a contract for the circularity that
meaning creates, because a child whose box arrives from the arrangement cannot first report the extent
the arrangement needs to compute that box.

A packed child MAY carry every other key its item type allows: `size` with a number, `content` or
`fill`, `max_w`, `max_h`, `when`, and on a `container` child `rotate`, `frame`, `padding`, its own
`items` and its own `flow` block.

A packed child's **available extent** SHALL be the container's padded inner extent on that axis. It
has no anchor to subtract and no inset to reserve, so `layout-sizing`'s available-extent formula
degenerates to exactly that. Every rule keyed to the source then applies unchanged: an authored extent
is checked and refused if it does not fit that box, a `content` extent is
`min(intrinsic, max_*, inner extent)`, and a `text` is laid out against the box `layout-sizing`
already gives it. Nothing about a packed child is sized differently from the same child in an
absolutely arranged container at `at: [0, 0]`.

A packed child SHALL be serialized as it was authored, with no `at` and no `to`. A representation
supplying a default anchor would return, from `GET /api/templates/{id}`, a spelling this requirement
refuses on the way in.

This requirement supersedes the `at` and `to` rows of the frozen `docs/SPEC.md` §4 placement table for
packed children only, and states in full what replaces them: a packed child has neither. For every
item that is not a packed child, both rows remain authoritative exactly as written.

#### Scenario: A packed child with an explicit position is refused

- **WHEN** a child of a flow container declares `at: [2, 2]`
- **THEN** the template fails validation and is quarantined
- **AND** the message names the JSON path of that child's `at`

#### Scenario: A packed fill child takes the whole inner extent

- **WHEN** a flow container with a padded inner width of 30 holds one child with `size: [fill, 4]`
- **THEN** that child is drawn 30 wide
- **AND** the same child in a container with no `flow` block is drawn the same width, as it is today

#### Scenario: A fill child beside a sibling overflows the line

- **WHEN** that container instead holds a `content`-sized child resolving to 10, `gap: 2`, then the
  `size: [fill, 4]` child
- **THEN** the line total is `10 + 2 + 30 = 42`, past the 30 inner box
- **AND** under the default `overflow: fail` the render fails with `item_out_of_frame`
- **AND** under `overflow: trim` the first child renders and the `fill` child does not

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

#### Scenario: A packed child round-trips without an anchor

- **WHEN** a template containing a flow container is read back through `GET /api/templates/{id}`
- **THEN** its packed children carry neither `at` nor `to` in the response
- **AND** submitting the returned document unchanged is accepted

### Requirement: Packing places the children that occupy a slot

The arrangement SHALL place, in template order, every **active** child whose main-axis extent is
greater than zero. Such a child **occupies a slot**.

A child whose `when` gate does not match (`docs/SPEC.md` §5) SHALL occupy no slot, so later children
close the hole rather than leaving one. A child whose main-axis extent is zero SHALL likewise occupy
no slot; a zero extent is what `layout-sizing` produces for an empty value, and giving it a slot would
print two gaps where one belongs.

A `gap` SHALL fall only between two children occupying the same line. A line SHALL carry no leading or
trailing gap, and a `line_gap` SHALL fall only between two lines.

Packing SHALL advance along the main axis by the child's own extent, which is the box `layout-sizing`
resolved and the box that is drawn. There is no second number.

#### Scenario: A gated-off child leaves no hole

- **WHEN** a flow container with `direction: column` and `gap: 1` holds three text children of height
  3 and the middle child's `when` gate does not match
- **THEN** two children render
- **AND** the second rendered child's top edge is 1 below the first child's bottom edge, the position
  it would occupy if the gated-off child were absent from the template

#### Scenario: An empty value leaves no double gap

- **WHEN** a flow container with `direction: row` and `gap: 2` holds three `content`-sized text
  children and the middle child's interpolated value is empty
- **THEN** the third child's left edge is exactly 2 right of the first child's right edge

### Requirement: The assembled extent is what a flow container reports

A flow container's **assembled extent** SHALL be, before padding:

- on the **main** axis, the largest line total, where a line's total is the sum of the extents of the
  children occupying it plus one `gap` between each adjacent pair;
- on the **cross** axis, the sum of the line extents plus one `line_gap` between each adjacent pair,
  where a line's extent is the largest cross-axis extent among the children occupying it.

The container's intrinsic size SHALL be that assembled extent plus its padding on each axis, which is
what the amended `layout-sizing` intrinsic requirement names. Everything downstream follows from
`layout-sizing` unchanged: `content` on the container resolves to that intrinsic, `fill` on the
container reports it bounded and takes the frame, an authored extent is used as written and the
arrangement packs into the padded inner box it defines, and `max_w`/`max_h` bind the first two and are
inert on the third. `fill` on the flow container itself is the ordinary frame source it always was: its
frame is its parent's and is known before it packs.

A flow container SHALL nest. A flow container that is itself a packed child reports its assembled
extent plus padding as its extent, and its parent places it at that.

When no child occupies a slot, the assembled extent SHALL be zero on both axes, so a `content` axis
resolves to the container's padding alone and a container with no padding resolves to zero and renders
an empty box. This is the outcome `layout-sizing` already gives a zero inner box, reached from the
other side, and SHALL NOT be an error.

#### Scenario: A content-sized flow container hugs its children

- **WHEN** a flow container declares `size: [content, content]`, `direction: row`, `gap: 2`,
  `padding: 1` and holds two children resolving to 10 and 6 wide and 4 tall
- **THEN** its width is `1 + 10 + 2 + 6 + 1 = 20`
- **AND** its height is `1 + 4 + 1 = 6`

#### Scenario: A flow container sizes a dynamic-width label

- **WHEN** a dynamic-width `single` carries only a flow container at `at: [0, 0]` with
  `size: [content, content]` whose children assemble to 34 wide
- **THEN** the label's width is 34 clamped to `[width.min, width.max]`

#### Scenario: A flow container is the root of a sheet slot

- **WHEN** a `sheet` template's layout is a single flow container with `size: [content, content]`
- **THEN** it packs and renders by the same rules, its frame being the slot
- **AND** nothing about the arrangement depends on the template's format

#### Scenario: A nested flow container is placed at its assembled extent

- **WHEN** a packed child is itself a flow container with `size: [content, content]`
- **THEN** it is placed at its own assembled extent plus its padding
- **AND** the outer container's assembled extent counts that value for it

#### Scenario: Every child gated off leaves a padding-sized container

- **WHEN** every child of a `content`-sized flow container with `padding: 1` has a `when` gate that
  does not match
- **THEN** the render succeeds and the container resolves to 2 on each axis, its padding alone

### Requirement: Wrapping starts a new line when the main axis runs out

When `wrap` is `true`, the arrangement SHALL place each child that occupies a slot on the current line
while its extent fits the room remaining, and SHALL start a new line for the first child whose extent
does not. A new line SHALL begin one `line_gap` past the previous line's cross-axis end, at the line
start edge, and the child that triggered the wrap SHALL be its first child.

Wrapping is possible because a packed child's extent is clamped to the container's **whole** padded
inner extent rather than to the room left on its line: a child can therefore be too wide for what
remains and still be a legal box. The same clamp means no single child can exceed a whole line, so
wrapping never has to place a child it knows cannot fit. A `fill` child fills a line exactly, so under
`wrap: true` it takes a line of its own and the next child begins a new one.

A child SHALL NOT be broken across lines. Wrapping chooses which line a child sits on and never
re-breaks the child's own content: what a `text` does inside its box is settled by its own `overflow`
policy (ADR-0082) and is not the arrangement's business.

`wrap: true` SHALL require the container's **main** axis to be resolved, as `layout-sizing` defines
resolved, and the service SHALL refuse the template at load otherwise, naming the JSON path of `wrap`.
The axis tested SHALL be the one `direction` names in the container's own author space: the horizontal
axis for `direction: row` and the vertical axis for `direction: column`, both read after the axis swap
a `rotate` of 90 or 270 applies. An unresolved main axis has no extent to wrap against when lines are
chosen: a `content` main axis is derived from the very children being packed, so wrapping against it
would ask the arrangement for the answer it is computing. This is the second rule that consults the
resolved-axis state, and the amended `layout-sizing` requirement names it.

When `wrap` is `false`, every child that occupies a slot SHALL be placed on one line regardless of the
room remaining, and what runs past the inner box is governed by the overflow requirement.

#### Scenario: A row wraps to a second line

- **WHEN** a flow container with `direction: row`, `wrap: true`, `gap: 2`, `line_gap: 1` and a padded
  inner width of 20 holds three children resolving to 8 wide and 4 tall
- **THEN** the first two sit on the first line, the second starting 2 after the first
- **AND** the third sits on a second line whose top edge is 1 below the first line's bottom edge,
  starting at the inner box's left edge

#### Scenario: wrap on an unresolved main axis is refused

- **WHEN** a flow container declares `direction: row`, `wrap: true` and `size: [content, 10]`
- **THEN** the template fails validation and is quarantined
- **AND** the message names the JSON path of `wrap`

#### Scenario: A column tests its own main axis

- **WHEN** a flow container declares `direction: column`, `wrap: true` and `size: [content, 10]`
- **THEN** the template loads, because the vertical axis its direction names is authored
- **AND** the same container spelling `size: [10, content]` is refused, naming `wrap`

#### Scenario: Rotation swaps which axis wrap tests

- **WHEN** a flow container declares `rotate: 90`, `direction: row`, `wrap: true` and, in its parent's
  physical frame, `size: [content, 10]`
- **THEN** the template loads: a quarter turn makes the author width the physical **height**, which is
  authored here, so the author axis `row` names is resolved
- **AND** the same container with `size: [10, content]` is refused, naming `wrap`, because that
  spelling leaves the author width unresolved

#### Scenario: wrap is accepted on a resolved axis under a sign-negative anchor

- **WHEN** a flow container on a dynamic-width `single` declares `at: [-40, 0]`, `size: [fill, 20]`
  and `wrap: true`
- **THEN** the template loads, because that axis is resolved at 40 before any child is sized

### Requirement: Lines, trimming and overflow are decided at render

Load-time validation SHALL NOT run the arrangement. `layout-sizing` has load take a content source to
yield its available extent, since load cannot measure text, encode a QR or decode an image, so at load
every `content` child of a flow container reports the whole inner box and their accumulation says
nothing about how many lines they occupy or which of them fit together.

Load SHALL therefore check what the template alone decides: this capability's structural refusals, and
each packed child against the padded inner box by `layout-sizing`'s ordinary rules, which is the same
check that child would receive in an absolutely arranged container. Line selection, the trim decision
and the overflow outcome SHALL be decided at render, against measured extents.

This is the load/render division `layout-sizing` already draws, and no part of it is weakened: sizing
still has exactly one implementation, run at both stages, and the arrangement is not sizing. A refusal
at load still depends only on the template, and passing load is still not a claim that a request will
render.

#### Scenario: A template that packs correctly is not refused at load

- **WHEN** a flow container with an authored inner width of 20 holds two `content`-sized text children
  that each render 5 wide with `gap: 2`
- **THEN** the template loads
- **AND** the render places both on one line

#### Scenario: An authored extent too large for the inner box is still refused at load

- **WHEN** a packed child declares `size: [30, 4]` in a flow container whose padded inner box is 20
  wide
- **THEN** the template fails validation and is quarantined, exactly as the same child would be in an
  absolutely arranged container

### Requirement: `overflow` decides what happens to a child that does not fit

Because every packed child is clamped to the padded inner box, no single child can overflow it. What
can is their **accumulation**: a line total on the main axis when `wrap` is `false`, and the stack of
lines on the cross axis when it is `true`. A `fill` child makes that accumulation certain on any line
it shares, which is what the sizing requirement above says it means. A child **does not fit** when the position the arrangement
would give it puts its far edge past the padded inner box.

`overflow` SHALL decide the outcome:

- **`fail`**, the default: the render SHALL fail with `UnsupportedLayoutItem` carrying
  `details.reason` of `item_out_of_frame`, the reason an item that leaves its frame already produces.
  No new reason is introduced.
- **`trim`**: the first child that does not fit SHALL NOT be drawn, and neither SHALL any child after
  it, whether or not a later child would have fitted. The render SHALL succeed.

A trim SHALL leave no mark on the label and SHALL NOT be reported to the caller. The default is `fail`
for that reason, and it differs from the `ellipsis` default `text` carries (ADR-0082) on that same
ground: an ellipsis is visible on the printed label and a dropped child is not.

**A trim removes a child from the drawing, and from nothing else.** Every active child of a flow
container, trimmed or not, SHALL be sized by `layout-sizing` against the padded inner box exactly as it
would be anywhere else, and every error that raises SHALL surface. This costs nothing to implement,
because every child's box is known before the arrangement runs and none of it depends on where the
child lands. `overflow: trim` grants no child an exemption from its own contract.

What that means per item follows from `layout-sizing` and is not a rule of this capability. Sizing
demands an intrinsic size only where an axis asks for one, so:

- an active `text` is laid out and enforces its `overflow` policy whatever its spelling (ADR-0082), so
  a trimmed `text` still raises `MissingField` for an unresolvable value and `text_does_not_fit` under
  `overflow: fail`;
- a `content`-sized `qr` or `image` is asked for its intrinsic size, so a trimmed one still raises
  `qr_generation_failed`, `intrinsic_size_undefined` or its `image_*` reason;
- an **authored**-size `qr` or `image` is asked for nothing, because "an `image` with `size: [20, 10]`
  never has its dimensions read". Its payload and its bytes are touched only when it is drawn, and a
  trimmed child is not drawn, so those errors SHALL NOT surface for it.

That asymmetry is `layout-sizing`'s, visible here only because trimming is the first thing that skips
drawing. A child that must not fail its own layout says so with its own `overflow: ellipsis`, which is
the `text` default.

A trimmed child SHALL count toward neither the drawing nor the assembled extent, so a flow container
reports what it actually rendered.

#### Scenario: Overflow fails the render by default

- **WHEN** a flow container with no `overflow` key packs children whose accumulated line runs past its
  padded inner box
- **THEN** the render fails with `UnsupportedLayoutItem` and `details.reason` of `item_out_of_frame`

#### Scenario: Trim drops the child that does not fit and every child after it

- **WHEN** a flow container declares `overflow: trim`, `direction: row`, `wrap: false`, `gap: 2`, a
  padded inner width of 20, and holds children resolving to 8, 8 and 2 wide
- **THEN** the first two children render and the third does not, even though 2 units remained
- **AND** the render succeeds

#### Scenario: A trimmed child still raises its own errors

- **WHEN** a flow container with `overflow: trim` would trim a `content`-sized `text` child whose
  value interpolates a field the request does not supply
- **THEN** the render fails with `MissingField`
- **AND** a trimmed `text` carrying `overflow: fail` whose text cannot fit the padded inner box still
  fails with `text_does_not_fit`

#### Scenario: A trimmed content-sized qr still raises

- **WHEN** a flow container with `overflow: trim` would trim a `qr` child with
  `size: [content, content]` whose payload cannot be encoded
- **THEN** the render fails with `qr_generation_failed`, because its intrinsic size was demanded

#### Scenario: A trimmed authored-size image raises nothing

- **WHEN** a flow container with `overflow: trim` would trim an `image` child with `size: [20, 10]`
  whose `name` names a data key the request does not supply
- **THEN** the render succeeds with `200` and that image is absent
- **AND** the same child left untrimmed fails, because drawing is what reads its bytes

#### Scenario: A trimmed child does not count toward the assembled extent

- **WHEN** a `content`-cross-axis flow container trims a child
- **THEN** its assembled extent reflects only the children that rendered
