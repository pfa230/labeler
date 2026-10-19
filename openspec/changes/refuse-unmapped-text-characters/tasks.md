## 1. Unmapped text refusal

- [ ] 1.1 Before changing behavior, run the existing unmapped-character render case and reproduce the Inter-measurement/fallback-render discrepancy with a character an available embedded fallback actually supplies; save the character, measured missing-glyph metrics and rendered font evidence under `.agent-runs/`. Do not claim the historical overflow/tape effect from source inspection alone.
- [ ] 1.2 Add `TextCharacterUnmapped` and its `AppError` constructor, then check normalized text mappings at the common layout entry point before fitting. Replace the existing fallback-success render expectation with refusal asserting code, reason, `details.field` and `details.character`; include a value whose unsupported character would otherwise be shortened away, plus supported text containing line breaks/tabs/default ignorables. The checks must fail if refusal is absent, happens only after shortening, or rejects the existing non-drawing characters.

## 2. Validation

- [ ] 2.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, and `npx --no-install openspec validate --all --strict --no-interactive`; require every command to pass.
