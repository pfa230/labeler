# Tasks

## 1. Allocate ordered packed fill

- [ ] 1.1 Update the shared flow sizing/arrangement path to supply primary-axis fill availability from earlier boxes and gaps before child measurement, for both directions and with existing caps. Replace the current sibling-plus-fill overrun expectation and extend the existing flow tests to assert the 30/10/2 remainder of 18 in rows and columns; fail on whole-inner allocation or an incorrect gap. Update the authoring guide's flow and sizing-error advice to describe remaining-space fill.
- [ ] 1.2 Preserve initial intrinsic claims while choosing content-parent and dynamic-label extents, and carry text fits for actual allocation into emission. Refit text when a packed-fill-dependent final box shrinks, using the existing wrapping/font-range/overflow logic. Extend renderer tests to compare fitted text with an authored box of equal dimensions and exercise a smaller final box; fail on stale lines/font size or feedback that changes the chosen ancestor size.
- [ ] 1.3 Exercise the changed flow path with existing wrap/fail/trim coverage, adding only cases needed for fill after earlier siblings, zero remainder and text-fit errors. Assert drawn boxes and failure reasons so incorrectly retrying text on a new line or hiding its error under trim fails the check.

## 2. Integration gates

- [ ] 2.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` and `npx --no-install openspec validate --all --strict --no-interactive`; require success before committing implementation.
