# Tasks

Prerequisites #497 and #498 are merged at `37ba4481`. Apply the behavior change through their existing `FlowLine` and `evaluate_marker_candidate` paths, and run the checks on that baseline.

## 1. Allocate ordered packed fill

- [x] 1.1 Use the merged `resolver::FlowLine` rules to supply primary-axis fill availability from earlier boxes and gaps before child measurement, for both directions and with existing caps. Replace the current sibling-plus-fill overrun expectation and extend the existing flow tests to assert the 30/10/2 remainder of 18 in rows and columns; fail on whole-inner allocation or an incorrect gap. Update the authoring guide's flow and sizing-error advice to describe remaining-space fill.
- [x] 1.2 Preserve initial intrinsic claims while choosing content-parent and dynamic-label extents, and carry actual boxes and text fits through the existing prepared items into emission. Perform required final fitting before flow trimming discards children, including nested children, without changing ancestor sizes. Refit text when a packed-fill-dependent final box shrinks, using the existing wrapping/font-range/overflow logic. Extend renderer tests to compare fitted text with an authored box of equal dimensions and exercise a smaller final box; fail on stale lines/font size or feedback that changes the chosen ancestor size.
- [x] 1.3 Extend the existing `evaluate_marker_candidate` operation to distinguish allocation-dependent rejection at the failing bound from ordinary rendering errors, and complete final nested fitting before accepting a marker. Port the preserved marker text-fit, geometry, unmapped-character and capped-shrink reproductions plus the final-only fitting failure in the design. Assert the chosen prefix, drawn boxes and original error reasons so skipped retries, stale fits or hidden ordinary errors fail the checks.

## 2. Integration gates

- [x] 2.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` and `npx --no-install openspec validate --all --strict --no-interactive`; require success before committing implementation.
