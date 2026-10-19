# Tasks

## 1. Publish the needed count

- [ ] 1.1 Emit `position_count` for sheet entries from validated positions in the existing index generator, extend its test coverage with an exact sheet count assertion, and regenerate `catalog/index.json`. The assertion must fail on a missing or incorrect count; inspect the generated diff to confirm it publishes no geometry.

## 2. Reuse format presentation

- [ ] 2.1 Update Catalog's entry type and refactor the existing badge/callers to pass format type and sheet count, then replace Catalog's plain format text with that badge. Update existing badge and Catalog tests; assertions must fail on a wrong label/count or Catalog bypassing the common badge. Check Catalog and installed cards in light and dark themes in a browser and record presentation evidence in `.agent-runs/`.

## 3. Integration gates

- [ ] 3.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, the UI lint/test/build commands, and `npx --no-install openspec validate --all --strict --no-interactive`. Verify index freshness by running the generator against a temporary copy of `catalog/` under `.agent-runs/` and comparing its output with the committed index, leaving the checked tree unchanged.
