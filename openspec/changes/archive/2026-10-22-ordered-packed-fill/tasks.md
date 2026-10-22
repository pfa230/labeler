# Tasks

Prerequisites #497 and #498 were merged at the historical baseline `37ba4481`. Continue the existing implementation at `aab0f654` with the focused repair in the design, and run the checks against the resulting patch.

## 1. Allocate ordered packed fill

- [x] 1.1 Use the merged `resolver::FlowLine` rules to supply primary-axis fill availability from earlier boxes and gaps before child measurement, for both directions and with existing caps. Replace the current sibling-plus-fill overrun expectation and extend the existing flow tests to assert the 30/10/2 remainder of 18 in rows and columns; fail on whole-inner allocation or an incorrect gap. Update the authoring guide's flow and sizing-error advice to describe remaining-space fill.
- [x] 1.2 Preserve initial intrinsic claims while choosing content-parent and dynamic-label extents, and carry actual boxes and text fits through the existing prepared items into emission. Perform required final fitting before flow trimming discards children, including nested children, without changing ancestor sizes. Refit text when a packed-fill-dependent final box shrinks, using the existing wrapping/font-range/overflow logic. Extend renderer tests to compare fitted text with an authored box of equal dimensions and exercise a smaller final box; fail on stale lines/font size or feedback that changes the chosen ancestor size.
- [x] 1.3 Preserve actual and limit frames and corresponding child measurements through intrinsic assembly, final nested arrangement and marker evaluation, retaining the existing `SizingError` classification at each failing check. First record the focused design's nested 8-to-12-unit retry regression failing on `aab0f654`, then repair both callers and the shared selector and assert the selected outer prefix and rendered marker. Keep the nested unmapped-character regression's original error and path and the existing marker retry, geometry and final-fitting regressions passing.

- [x] 1.4 Qualify the authoring guide's fill-first statement as "An uncapped fill child" and check the wording against the retained cap behavior.

## 2. Integration gates

- [x] 2.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` and `npx --no-install openspec validate --all --strict --no-interactive`; require success before committing implementation.
