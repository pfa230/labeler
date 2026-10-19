## 1. Repeat markers

- [ ] 1.1 Add `trim_marker` to raw/domain container models and conversion; extend parser/conversion coverage to accept it on a repeat and reject it without `repeat`, asserting the field path.
- [ ] 1.2 Implement shared repeat-marker selection for flow measurement and rendering using the existing arranger; extend the repeat-trim render coverage to distinguish four tags rendered as `A B ...` from the exactly fitting three rendered as `A B C`. The test must fail if the marker is dropped, if space is not reclaimed for it, or if an exactly fitting entry is replaced.
- [ ] 1.3 Add an authoring-guide example using the new field and the repeated text value; verify the example parses and produces PNG artifacts for both input lists under `.agent-runs/`.

## 2. Validation

- [ ] 2.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, and `npx --no-install openspec validate --all --strict --no-interactive`; require every command to pass.
