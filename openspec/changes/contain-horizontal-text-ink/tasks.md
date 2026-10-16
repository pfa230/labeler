## 1. Measure horizontal geometry from the emitted shaping

- [ ] 1.1 Extend the existing paragraph/line shaping results and fractional outline measurement with shaped x advances, offsets and horizontal ink bounds, including advances without outlined ink; verify against the existing independent Typst/font oracles using the positive-kerning W run and negative-bearing j, preserving vertical measurements.
- [ ] 1.2 Apply the text delta's horizontal reservation to size selection, wrapping/ellipsis width checks, line-budget/failure decisions and final intrinsic width; verify centered asymmetric-overhang content reserves twice the larger side while left/right reserve their sum, and reuse one candidate shaping result with the existing metric-height early rejection.

## 2. Emit and verify contained text

- [ ] 2.1 Carry horizontal reservations to the existing text emitter, apply the alignment-specific insets alongside vertical padding and set optical overhang off; verify generated text retains its clips and independent emitted glyph positions agree with the measured advances and bounds.
- [ ] 2.2 Extend the existing raster containment helper to horizontal columns with a same-phase unclipped control, including enclosing clips when testing a capped container; add the reproduced dynamic W/ellipsis, capped j and centered overflowing-text cases plus narrow/wide ending controls, verify their final ink is contained and the selected dynamic outputs stop before the last raster column, and demonstrate failures when restoring each corresponding old width/inset/overhang behavior.
- [ ] 2.3 Update only the authoring guide's text-fitting explanation to describe shaped advances and horizontal ink reservation, including alignment-dependent content width; verify the prose matches the text delta and no new template option or fixed padding is documented.

## 3. Run the required gates

- [ ] 3.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test`; all must pass, including existing vertical ink, wrapping, shrink, overflow, empty-value and script/mark coverage, with changed width expectations justified by independent rendered geometry.
- [ ] 3.2 Run `npx --no-install openspec validate --all --strict --no-interactive`; verify the complete modified Layout box and Measured block and ink reservation requirements retain their applicable scenarios and validate.
