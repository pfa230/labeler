# Design

## Context

The historical main baseline at `37ba4481` includes #495's preparation/emission split, #497's `resolver::FlowLine` and #498's `RenderContext::evaluate_marker_candidate`. On that baseline, packed fill takes the whole inner frame. `FlowLine` supplies `lead`, `wraps_before`, `add` and `remainder`; candidate evaluation measures against the whole frame and saves its measurement, leaving nested preparation for later. See the proposal for the behavior change.

The current implementation was built on that baseline; continue its focused repair below. The failed implementation at `d90b9420ae7dbac1dfcce209a91f7397e8c45060` is preserved on local branch `backup/issue-260-before-split`; the split plan is preserved at `0feaf730` on `backup/issue-260-before-497-498`. The earlier attempt remains on `backup/issue-260-before-495` at `6de1a72fcdd63e2e2cbd97e914784f61462bd0f2`. Use their focused reproductions without replaying the superseded implementation. Keep the current selected-face text fitter and matcher.

## Goals / Non-Goals

**Goal:** Allocate and fit ordered packed fill consistently during intrinsic measurement and final preparation.

**Non-goal:** Solve layout by iterating parent and child sizes to a fixed point.

## Decisions

- User decision: fill consumes the remainder after earlier siblings, in template order. The user accepted this recommendation during scoping on 2026-10-19. Do not reserve room for later siblings or divide it equally between fills.
- Default: for a row, determine each child's width allocation before measuring it, then resolve its height from that measurement; for a column, determine its height allocation first, then resolve width. Subtract earlier resolved primary box extents and intervening gaps, including the pending gap only when calculating a positive fill allocation. Clamp the remainder at zero and apply existing caps through the shared extent resolver. Other extent sources and the cross axis retain their existing availability. Non-fill boxes still decide wrapping using the whole inner frame, so ordinary content wrapping remains reachable.
- Use the merged `resolver::FlowLine` rules to drive ordered sizing for initial measurement and final placement, with item measurement in `render/mod.rs`. The walk must obtain a child's measurement after supplying its allocation; precomputing every child against the whole frame causes the overrun and stale text fits visible in the current code. Carry resulting boxes and text fits through the existing prepared items, including nested containers and repeats; emission consumes those results without resolving or refitting them.
- Default: preserve the one-way intrinsic probe. Unknown content-parent axes use the current provisional inner bounds; dynamic labels use `width.max`. Each child's box advances the cursor; its existing claim, `min(intrinsic, cap, available)`, contributes upward. Choose parent/label extents once from that initial assembly. A later smaller actual text box reruns existing text fitting; it does not feed a new requirement into ancestors. This gives a finite evaluation order without rejecting content parents or adding a solver. Final packing or text fitting can still fail under their specified policies.
- Existing claim-versus-box differences remain relevant: a content row with gap 2, a first fill capped at 8 but claiming 4, then an authored width 10 can initially fit a width-30 frame, request width 16 and subsequently overrun with boxes totalling 20. The present claim and arrangement functions already permit that outcome. This change does not promise that successful provisional measurement guarantees final success.
- Zero allocation uses existing zero-box rules: it occupies no primary slot or gap and does not itself trigger a new line; its own text errors still apply. An uncapped positive fill consumes the available line remainder. Wrapping does not retry a text fitting failure on a fresh line. `trim` still removes arranged overflows while sizing trimmed children; it does not hide an item's own text or authored-size failure.
- Resolve and fit actual child boxes before trimming can discard them, including nested children. Keep the initial ancestor sizes unchanged and carry the prepared fits into emission. A fitting check attached only to retained children repeats the last implementation's observed trim failure.
- Extend the existing `evaluate_marker_candidate` operation so marker sizing and final nested fitting distinguish three outcomes: a fully fitted candidate, rejection caused by the candidate's allocated space, and an ordinary rendering error. Determine the outcome where the failing bound is measured, including through nested containers and after applying authored dimensions and caps. Run the required final nested fit inside that operation before returning an accepted candidate and retain its fitted result for preparation; a later fit after prefix selection cannot trigger the required shorter-prefix retry. Only allocation-dependent rejection tries a shorter prefix. Ordinary errors propagate unchanged. Error reason codes alone cannot distinguish these outcomes: the same `text_does_not_fit` reason can describe a marker that fits a larger allocation or a fixed-size marker whose box cannot change between candidates.
- Update the existing flow overrun expectation to remaining-space success and extend its row/column cases. Compare packed text fits against the existing text fitter given the allocated box, including wrapping and font ranges. Exercise final smaller-box fitting and existing fail/trim interactions through the same renderer tests. Each assertion must distinguish a full-inner-box allocation, stale fit, incorrect intrinsic claim or altered overflow result. Update the authoring guide's whole-inner-fill advice.

## Observed failures to retain

Reviews of the preserved implementation found the following cases; port their focused reproductions to the new path and add the missing variants rather than copying the old sizing machinery:

- A fixed 10×4 marker containing a nested row and `[fill, 4]` text must preserve its ordinary `text_does_not_fit` error. A nested flow alone does not make its enclosing fixed box depend on the candidate allocation.
- A marker's absolute descendant at `[-4, 0]` can fail `coord_out_of_frame` at zero allocation and fit a shorter prefix's 8-unit allocation. Classify the failing resolver bound, not a whitelist of error reasons. Apply the same rule to frame-dependent `to` inversions.
- A 30-unit trimming row with a 29-unit child followed by a content-width row containing a fill QR capped at 10 (intrinsic width 2.1) and fill text `H` at 8 pt with `overflow: fail` must run the text's final zero-width fit even if the outer row trims that nested row.
- An initially capped title or ancestor can shrink below that cap at final allocation and still needs refitting. Conversely, a fixed-height marker may fit a wider candidate because wrapping uses fewer lines; unavoidable hard-line height errors remain ordinary. The preserved capped-title and authored-height marker cases distinguish these outcomes.
- The latest attempt selected a marker before final nested fitting. A marker containing a content-width row with an 8-wide fill box claiming 4, followed by wrapped `WW WW` fill text at 8 pt with `overflow: fail`, can measure at width 14 and fail after the row hugs its claims, while a wider candidate fits. Keep this final-only fitting failure inside candidate selection so it can try a shorter prefix.

## Focused repair of the current implementation

Continue the existing patch at `da87e18a`; this repair does not rebuild or extract its layout machinery. It already has `SizingError::{Allocation, Ordinary}`, but `intrinsic`, `arrange_flow_children` and `evaluate_marker_candidate` return plain `AppError`. A nested failure therefore loses its classification before a parent classifies an aggregate operation again. Preserve the existing `SizingError` through nested intrinsic sizing, arrangement and candidate evaluation, converting to the public error only when leaving the sizing/fitting path. Classify raw geometry or text checks where they fail; an error returned by a nested operation retains its existing classification. A successful larger subtree is not evidence that its smaller subtree's ordinary error was caused by allocation.

The concrete regression is a nested trimming row with three 4-wide instances: at width 8 it uses a marker containing an unmapped character; at width 12 no marker is needed. An enclosing candidate must propagate the original `text_character_unmapped` error, even though a wider probe omits the invalid inner marker and succeeds. Demonstrate this failure on the current implementation before fixing it, then require the same error code and path afterward. Retain the previous allocation-retry and final-fitting regressions. Use the existing error type and resolver checks; neither a reason whitelist nor another renderer abstraction serves this repair.

## Risks / Trade-offs

Source order now determines who gets primary-axis fill space. Uncapped fill normally belongs after the content it should share a line with. Initial intrinsic claims and final boxes remain distinct, as the contract requires; final refitting may shrink, shorten or fail text inside the chosen ancestor dimensions.

## Migration Plan

Continue the existing implementation at `da87e18a` with the focused repair above, and run the checks against the resulting patch before archiving through the loop. Replace the old behavior without adding a compatibility spelling or configuration switch.
