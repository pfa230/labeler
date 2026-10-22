## Why

`arrange_flow` in `src/resolver.rs` keeps the line a flow is filling (its occupied primary extent, the pending gap, the wrap test and zero-primary occupancy) as loose locals, written twice: once to decide line membership and again as the placement cursor. #260 must ask that same line how much primary room is left before it measures a fill child, and its unmerged attempt failed review partly because it extracted these rules while also changing sizing. This refactor gives the line rules one home on main first, without changing behavior.

Source issue: https://github.com/pfa230/labeler/issues/497

Fixes #497

## What Changes

- Extract the flow line's state and rules from `arrange_flow` into one type in `src/resolver.rs`: where the next occupying child starts, whether a wrapping flow starts a new line before a child, adding a child, and the primary room left on the line.
- Have both of `arrange_flow`'s walks, line membership and placement, use it, so wrapping, gaps, zero-primary children, assembled extents and overflow errors come from one implementation.
- Leave child sizing unchanged: a packed `fill` still resolves against the whole padded inner frame, and nothing in the renderer calls the remaining-room query yet.

## Capabilities

### New Capabilities

### Modified Capabilities

None. This is a behavior-preserving refactor; `.openspec.yaml` sets `skip_specs: true`.

## Impact

`src/resolver.rs` only. No API, template schema, layout, rendering or error contract changes, and no new dependencies. `arrange_flow`'s signature stays, so `src/render/mod.rs` and every existing test compile unchanged.

Non-goals: an ordered resolution walk that sizes children against the remainder, final text refitting and trim-marker evaluation. The first two remain in #260 (`ordered-packed-fill`); marker evaluation is #498.
