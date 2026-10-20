# Design

## Context

At `3dddfb20`, `measure_items` measures flow children independently against the provisional whole inner frame. `intrinsic` and `render_items` separately call `resolve_packed` and `arrange_flow`. `Measured` retains text fits for emission. The existing `uncapped_and_capped_fill_child_in_flow` test explicitly expects a sibling followed by uncapped fill to overrun. The issue's #263 dependency is closed and its anchorless-child contract is present. See the proposal and layout/text deltas for scope.

## Goals / Non-Goals

**Goal:** Allocate and fit ordered packed fill consistently during intrinsic measurement and final emission.

**Non-goal:** Solve layout by iterating parent and child sizes to a fixed point.

## Decisions

- User decision: fill consumes the remainder after earlier siblings, in template order. The user accepted this recommendation during scoping on 2026-10-19. Do not reserve room for later siblings or divide it equally between fills.
- Default: for a row, determine each child's width allocation before measuring it, then resolve its height from that measurement; for a column, determine its height allocation first, then resolve width. Subtract earlier resolved primary box extents and intervening gaps, including the pending gap only when calculating a positive fill allocation. Clamp the remainder at zero and apply existing caps through the shared extent resolver. Other extent sources and the cross axis retain their existing availability. Non-fill boxes still decide wrapping using the whole inner frame, so ordinary content wrapping remains reachable.
- Keep arrangement arithmetic in `resolver.rs` and use one ordered flow walk for initial measurement and final placement, with item measurement in `render/mod.rs`. The walk must obtain a child's measurement after supplying its allocation; precomputing every child against the whole frame causes the overrun and stale text fits visible in the current code. Carry resulting text fits to emission, including through nested containers and repeats, rather than recomputing a different arrangement while drawing.
- Default: preserve the one-way intrinsic probe. Unknown content-parent axes use the current provisional inner bounds; dynamic labels use `width.max`. Each child's box advances the cursor; its existing claim, `min(intrinsic, cap, available)`, contributes upward. Choose parent/label extents once from that initial assembly. A later smaller actual text box reruns existing text fitting; it does not feed a new requirement into ancestors. This gives a finite evaluation order without rejecting content parents or adding a solver. Final packing or text fitting can still fail under their specified policies.
- Existing claim-versus-box differences remain relevant: a content row with gap 2, a first fill capped at 8 but claiming 4, then an authored width 10 can initially fit a width-30 frame, request width 16 and subsequently overrun with boxes totalling 20. The present claim and arrangement functions already permit that outcome. This change does not promise that successful provisional measurement guarantees final success.
- Zero allocation uses existing zero-box rules: it occupies no primary slot or gap and does not itself trigger a new line; its own text errors still apply. An uncapped positive fill consumes the available line remainder. Wrapping does not retry a text fitting failure on a fresh line. `trim` still removes arranged overflows while sizing trimmed children; it does not hide an item's own text or authored-size failure. These follow existing occupancy and overflow contracts, so no duplicate delta rules or special recovery paths are added.
- Update the existing flow overrun expectation to remaining-space success and extend its row/column cases. Compare packed text fits against the existing text fitter given the allocated box, including wrapping and font ranges. Exercise final smaller-box fitting and existing fail/trim interactions through the same renderer tests. Each assertion must distinguish a full-inner-box allocation, stale fit, incorrect intrinsic claim or altered overflow result. Update the authoring guide's whole-inner-fill advice.

## Risks / Trade-offs

Source order now determines who gets primary-axis fill space. Uncapped fill normally belongs after the content it should share a line with. Initial intrinsic claims and final boxes remain distinct, as the contract requires; final refitting may shrink, shorten or fail text inside the chosen ancestor dimensions.

## Migration Plan

Implement and archive through the loop after explicit enrollment. Replace the old behavior without adding a compatibility spelling or configuration switch.
