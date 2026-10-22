# Tasks

Prerequisites #497 and #498 were merged at the historical baseline `37ba4481`. Continue the existing implementation at `da87e18a` with the focused repair in the design, and run the checks against the resulting patch.

## 1. Allocate ordered packed fill

- [x] 1.1 Use the merged `resolver::FlowLine` rules to supply primary-axis fill availability from earlier boxes and gaps before child measurement, for both directions and with existing caps. Replace the current sibling-plus-fill overrun expectation and extend the existing flow tests to assert the 30/10/2 remainder of 18 in rows and columns; fail on whole-inner allocation or an incorrect gap. Update the authoring guide's flow and sizing-error advice to describe remaining-space fill.
- [x] 1.2 Preserve initial intrinsic claims while choosing content-parent and dynamic-label extents, and carry actual boxes and text fits through the existing prepared items into emission. Perform required final fitting before flow trimming discards children, including nested children, without changing ancestor sizes. Refit text when a packed-fill-dependent final box shrinks, using the existing wrapping/font-range/overflow logic. Extend renderer tests to compare fitted text with an authored box of equal dimensions and exercise a smaller final box; fail on stale lines/font size or feedback that changes the chosen ancestor size.
- [ ] 1.3 Preserve the existing `SizingError` classification through nested intrinsic sizing, arrangement and candidate evaluation; classify only the raw failing geometry/text check and convert to `AppError` only at the outer boundary. Add the nested 8-versus-12-wide marker regression from the focused repair design, first recording its failure on the current patch, then asserting that the fixed render preserves `text_character_unmapped` and its original path. Keep the existing marker retry, geometry and final-fitting regressions passing.

- [ ] 1.4 Qualify the authoring guide's fill-first statement as "An uncapped fill child" and check the wording against the retained cap behavior.

## 2. Integration gates

- [ ] 2.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` and `npx --no-install openspec validate --all --strict --no-interactive`; require success before committing implementation.
