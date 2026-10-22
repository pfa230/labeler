# Design

## Context

`arrange_flow` receives every child's resolved box and requirement, already computed by `flow_child_input` in `src/render/mod.rs` against the whole padded inner frame, and walks them twice.

- The membership walk keeps `line_box_primary` and `line_has_occupying`. A child occupies its line when its primary box extent is positive. A wrapping flow starts a new line before an occupying child when the line already has an occupying child and `line_box_primary + gap + extent` exceeds the inner primary extent by more than `BOUNDS_EPSILON`. A zero-primary child joins the current line and never starts one.
- The placement walk re-derives the same running extent per line as `cursor`, adding the gap before every occupying child after the first. It places an occupying child at `cursor + gap` (or `cursor` for the first one), a zero-primary child at `cursor + gap` only when an occupying child precedes it and another follows it on the line, and at `cursor` otherwise. It checks each box with `fits_frame`, applies `fail`/`trim` overflow, and assembles the requirement extents separately from the boxes.

Both walks perform the same additions in the same order, so their running extents agree bit for bit. #260's design (`openspec/changes/ordered-packed-fill/design.md`) defines a fill child's primary availability as the line's inner extent less its occupying boxes, their gaps and the pending gap, clamped at zero.

## Goals / Non-Goals

**Goals:**

- One type owns the line rules both walks use: where the next occupying child starts, the wrap test, adding a child, and the remaining primary room.
- The remaining-room query exists for #260's ordered walk to call before it measures a child.

**Non-Goals:**

- Sizing any child against the remainder, or an ordered resolution walk in the renderer. With sizing unchanged, such a walk would pass a remainder its only caller ignores.
- Merging the two walks into one. Placement of a zero-primary child depends on whether a later child on the same line occupies it, which the membership walk decides first.
- Changing requirement assembly, `fits_frame` checks, the overflow policies or `FlowResult`.

## Decisions

### A `FlowLine` value with four operations

`src/resolver.rs` gains a small `FlowLine` struct holding the line's occupied primary extent and whether a box occupies it, starting empty (`Default`). Its operations:

- `lead(flow)`: where the next occupying child starts, the occupied extent plus the gap when the line is occupied.
- `wraps_before(flow, inner_primary, extent)`: true when `flow.wrap`, `extent > 0`, the line is occupied and `primary + flow.gap + extent > inner_primary + BOUNDS_EPSILON`, the existing test written in the existing arithmetic order.
- `add(flow, extent)`: for a positive extent, adds the gap when the line is occupied, then the extent, and marks the line occupied; a zero extent changes nothing.
- `remainder(flow, inner_primary)`: `inner_primary - primary - pending_gap`, clamped at zero, where the pending gap is `flow.gap` only when the line is occupied.

The membership walk starts a new line and a fresh `FlowLine` when `wraps_before` holds, then calls `add`. The placement walk keeps a fresh `FlowLine` per line; it places a child at `lead` when the child occupies the line or an occupying child follows it on the line, and at the occupied extent otherwise, then calls `add`. `num_occupying` stays only for counting the gaps in the line's requirement.

Keeping the wrap test and the additions in today's arithmetic order keeps every float result identical, including the epsilon boundary `flow_wrap_boundary_including_pending_gap_uses_epsilon` pins. Rewriting the wrap test as `extent > remainder + BOUNDS_EPSILON` would reassociate the sum and the clamp could change which side of the boundary a child lands on.

The stopped #260 attempt (`d90b9420`) combined the wrap test and the addition into one `push` returning whether it wrapped. That shape fits the membership walk but not placement, which must not wrap and must read the lead before adding; four operations serve both walks without a flag.

### One focused test, for `remainder` only

Every other operation replaces code the existing `arrange_flow` tests already pin: placement, gaps, zero-primary children, wrapping in both directions, the epsilon boundary, overflow and trim. `remainder` has no caller yet, so a resolver unit test drives one row `FlowLine` with gap 2 in an inner extent of 30: 30 when empty, 18 after a child of 10, and 0 after a further child of 18 fills the line. It fails if the remainder counts the pending gap on an empty line (28), omits it on an occupied one (20), or goes negative instead of clamping (-2).

## Risks / Trade-offs

- [`remainder` has no production caller until #260] → The issue asks for it, and the unit test pins its contract until #260 calls it.
- [A walk drifts from today's arithmetic] → Every existing `arrange_flow` and renderer flow test must pass unedited, including the epsilon and zero-primary cases.
