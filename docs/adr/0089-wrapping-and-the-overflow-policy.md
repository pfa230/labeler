# 89. Wrapping and the overflow policy

Date: 2026-08-29

## Status

Accepted. Issue [#212](https://github.com/pfa230/labeler/issues/212). Amends
[ADR-0083](0083-packed-children-flow-layout.md) in three places: the fields in the `flow` block, the
single-line assembled extent, and its unconditional `item_out_of_frame` result for accumulated
overrun.

## Context

ADR-0083 introduced packed children as one row or column. It deliberately left a child that did not
fit as a render error. That makes variable-length sets awkward on a fixed label: authors can neither
continue on another line nor choose that a partial set is preferable to no label.

Wrapping cannot be decided against an extent the same children are computing. Trimming has a related
feedback problem on both axes: removing a child removes both of its requirements, so either
content-sized container axis could change after the trim. The arrangement also carries two distinct
quantities from sizing. A child's box is what must physically fit, while its requirement is what it
reports to a content-sized parent; `fill` can make them differ.

## Decision

1. The `flow` block adds `wrap` (default `false`), non-negative `line_gap` (default `0`), and
   `overflow` (`fail`, the default, or `trim`). `line_gap` is inert when wrapping is disabled.
2. Wrapping groups children into lines in template order. A positive-primary-extent child's resolved
   box decides whether it fits the current line. A line's largest secondary box positions the next
   line, while its largest secondary requirement contributes to the assembled extent. The assembled
   primary extent is the largest line requirement total; the secondary extent is the sum of line
   requirements and intervening `line_gap`s.
3. `wrap: true` requires the flow container's primary author axis to be resolved. `overflow: trim`
   requires both author axes to be resolved. Quarter-turn rotation swaps the resolved-axis state
   before these checks, because packing remains in author space.
4. Check 1 from ADR-0083 remains unconditional: a child's own resolved box must fit the padded inner
   frame. `overflow` governs only check 2, the arrangement-supplied position. Under `fail`, an
   arranged overrun remains `UnsupportedLayoutItem` with reason `item_out_of_frame`. Under `trim`,
   the first child failing check 2 and every later child are omitted from drawing and assembled
   extent, and the overrun raises nothing.
5. Trimming is not an evaluation exemption. Every active child is still sized and evaluated before
   arrangement. A trimmed content-sized child can therefore raise its ordinary intrinsic or
   interpolation error, while an authored-size image whose bytes are needed only for drawing is not
   read.

## Consequences

- Existing templates retain byte-for-byte arrangement behavior because the new defaults select one
  line and failure on overrun.
- Authors can build wrapped rows and columns and can opt into silent tail removal when a partial label
  is acceptable.
- `trim` is deliberately silent: the response does not report it and the label carries no marker.
  `fail` remains the default so missing output must be an explicit author choice.
- A wrapping flow on a content-sized primary axis, or trimming with either content-sized axis, is
  quarantined at template load instead of depending on provisional geometry.
