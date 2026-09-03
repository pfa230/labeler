## 1. Schema and validation plumbing

- [x] 1.1 Add `line_spacing` to `TextRaw` in `src/raw.rs` with presence-tracking deserialisation so an explicit null is refused rather than read as absent, keeping `deny_unknown_fields` closed on every other item type
- [x] 1.2 Add `line_spacing` to `LayoutItem::Text` in `src/models.rs`, omitted from read-back when absent and reported as authored otherwise
- [x] 1.3 Validate the value in `src/convert.rs` (`bare number, finite, greater than zero`, error naming the item path and the key) and in `TemplateDefinition::validate` in `src/templates.rs`, mirroring `font_weight`
- [x] 1.4 Add load tests proving each refusal quarantines without aborting startup: string (`"1.2em"`, `"{{ pitch }}"`), boolean, array, explicit null, zero, negative, NaN and infinite values, each error naming the file and the `line_spacing` key
- [x] 1.5 Add tests proving `line_spacing` on a `container` (and each of `qr`, `image`, `line`) fails as an unknown field, the write endpoint refuses before writing, and read-back omits an absent key while reporting an authored `0.99`

## 2. Pitch-parameterised fitting and emission

- [x] 2.1 Replace `leading()` and the `cap_height + 0.65em` stacking in `src/render/helpers.rs` with a pitch-parameterised block height (`cap_height + (n-1) x pitch`), budget divisor and derived emitted leading (`pitch - cap_height`), threaded through `TextLayoutItem` from one shared function
- [x] 2.2 Emit a block-scoped Typst `par(leading:)` per text item in `src/render/mod.rs`, threaded through `TextRenderArgs`, so the setting cannot leak across items sharing one Typst source
- [x] 2.3 Add render-measured tests on identical repeated lines (`"Hxy\nHxy"`) proving band distances of 0.99, 0.5 (below the cap-height ratio), 1.5 and the 1.2 default, plus absent-renders-identically-to-explicit-`1.2`
- [x] 2.4 Add render-measured tests proving a tighter pitch settles a height-bound range item at a larger size than a looser one, and that a single-line item renders byte-identically with and without the field
- [x] 2.5 Extend the Typst-layout agreement probe to authored leading values (0.5, 0.99, 1.2, 1.5 over one to three lines) within the existing 1% tolerance

## 3. Docs, schema registration and fallout

- [x] 3.1 Show `line_spacing` on a worked `text` example in `docs/AUTHORING.md` with its meaning stated alongside
- [x] 3.2 Verify `src/openapi.rs` needs no hand edit (derive picks up the optional member) and re-baseline every golden render expectation the 1.3775em-to-1.2em default move shifts, checking each updated expectation is explained by the pitch change alone

## 4. Gates

- [x] 4.1 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test` until clean, fixing root causes rather than silencing lints
- [x] 4.2 Run strict OpenSpec validation (`openspec validate --all --strict --no-interactive`) until it passes
