## MODIFIED Requirements

### Requirement: A container establishes a padded frame, and rotation swaps it

A `container`'s children SHALL be sized against its **padded inner box**, its resolved extent less
its padding, clamped at zero on each axis. Where they are then placed SHALL be decided by the
container's **arrangement**: by each child's own coordinates under the **absolute** arrangement, the
default, and by packing them in order under the **flow** arrangement, which a `flow` block selects
(`flow-layout`). Sizing is identical under both, and this requirement's rules below govern both. A zero inner box SHALL render an empty container, whether
or not it has active children.

That rule governs the **container**, and nothing else. Every child retains every rule that applies to
it in the zero frame it was given: authored extents are still checked, bounds are still enforced,
intrinsics are still demanded where an axis asks, `line` endpoints must still differ, and a `text`
still enforces its `overflow` policy. Several of those can fail there, and none is suppressed:

- an active `text` with non-empty content is the extreme case of "cannot fit however short", so it
  raises `text_does_not_fit` under either policy;
- an active item with an **authored** extent that does not fit, such as `size: [1, 1]` in a zero-wide
  inner box, fails the authored-extent check exactly as it would in any other frame;
- an active `line` fails its bounds or degeneracy checks if its endpoints require room the frame does
  not have.

What renders empty is a child whose own extent resolves to zero, which is what a content or frame
source does in a zero frame. There is no precedence rule between the container and its children: the
container renders, and each child's own contract decides the rest. A padding that meets or exceeds a **constant** box SHALL be
refused at load, where nothing a request can supply could change the outcome. There is no
render-time `container_padding_no_room`.

A `container` that gives neither `size` nor `to` SHALL default to `size: [fill, fill]`, the
resolution its previous `[auto, auto]` default produced on every format.

This requirement supersedes the frozen `docs/SPEC.md` §4.2 in full and carries the complete
post-change rotation contract. Everything in that section SHALL continue to hold except its final
bullet:

- **Container-only, orthogonal.** `rotate` is valid only on a `container` and must be a multiple of
  90 degrees (`{0, 90, 180, 270}`, normalised via `rem_euclid(360)` within a small tolerance). Any
  `rotate` on another item type, or a non-orthogonal value, is a validation error.
- **Counter-clockwise.** Author canvas corners map to physical box corners as R90 BL→BR, BR→TR,
  TR→TL, TL→BL; R180 BL→TR and so on; R270 BL→TL and so on.
- **`at` and its extent stay parent-frame.** A rotated container is placed and bounds-checked exactly
  like an unrotated one. Rotation is an inner transform, so nested rotated containers compose without
  compounding coordinate flips.
- **The inner author canvas swaps for 90 and 270.** Children are authored in the container's natural
  reading orientation; the inner authoring box and child bounds swap to `[inner_h, inner_w]`. Padding
  is author-space and rotates with the design. The container's own paint, its `stroke` and its
  `background` (`shape-paint`), is not rotated: it is painted on the container's box in its parent's
  frame at every rotation.

The final bullet, "No `auto` under rotation", is **replaced**. Sizes SHALL compose through the swap:
a rotated container SHALL compute its intrinsic size in **author space** by the ordinary container
rule, padding plus its children combined on each author axis by whichever arrangement it carries, and
SHALL then swap the resulting pair to obtain its intrinsic size in its parent's physical frame. It is the completed
author-space aggregate that swaps, not the children's raw intrinsic sizes: author-space offsets,
`line` requirements, a flow container's author-space packing direction and author-space padding are
inside the aggregate and survive the swap with it.
`content` and `fill` SHALL be permitted anywhere beneath a rotated container and on the rotated
container itself, and the intrinsic pass SHALL recurse into it in author space.

ADR-0036 is amended accordingly: its §5 is retired and the rest stands.

#### Scenario: A zero inner box renders an empty container

- **WHEN** a container's resolved width equals its horizontal padding and its active children are a
  `line`, an `image` and a `text` bound to an empty value
- **THEN** it renders as an empty box and the render succeeds
- **AND** a template declaring `size: [10, 10]` with `padding: 6.0` fails validation instead

#### Scenario: An authored child in a zero box fails its own check

- **WHEN** that same container holds an active `image` with `size: [1, 1]`
- **THEN** the render fails the authored-extent check, because 1 does not fit a zero-wide inner box
- **AND** an active `line` whose endpoints need room the inner box lacks fails its own bounds check

#### Scenario: A text with content in a zero box still raises

- **WHEN** that same container holds an active `text` whose value is non-empty
- **THEN** the render fails with reason `text_does_not_fit`, under either `overflow` value
- **AND** the container's own rule is unchanged: it is the child's policy that raises

#### Scenario: A rotated container hugs its rotated content

- **WHEN** a `container` with `rotate: 90` and `size: [content, content]` holds a `text` with
  `size: [content, content]` whose laid-out size in author space is 40 by 6
- **THEN** its physical footprint in the parent is 6 wide by 40 tall

#### Scenario: The swap carries author-space padding and offsets

- **WHEN** a `container` with `rotate: 90`, `size: [content, content]` and `padding: 2.0` holds a
  `text` at author-space `at: [3, 1]` with `size: [10, 4]`
- **THEN** its author-space intrinsic size is 17 by 9, so its physical footprint is 9 wide by 17 tall
- **AND** an implementation swapping only the child's 10 by 4 would give 4 by 10 and fail

#### Scenario: A line inside a rotated container contributes through the swap

- **WHEN** a `container` with `rotate: 270` and `size: [content, content]` holds only a `line` from
  author-space `at: [0, 0]` to `to: [12, 0]`
- **THEN** its author-space intrinsic width is 12 and its physical intrinsic height is 12

#### Scenario: A stretching child beneath rotation takes the author canvas

- **WHEN** a `container` with `rotate: 90` and `size: [10, 40]` holds a `text` with
  `size: [fill, 6], max_w: 25`
- **THEN** the author canvas is 40 wide, and the text's author-space width is 25, its cap binding
- **AND** `max_w` names the author axis the item is authored in, not the physical one

#### Scenario: 180 degrees does not swap the canvas

- **WHEN** a `container` with `rotate: 180` and `size: [content, content]` holds a `text` whose
  laid-out size is 40 by 6
- **THEN** its footprint is 40 wide by 6 tall
