## 1. Newline normalization

- [x] 1.1 Extend `crlf_normalisation_matches_lf` to cover lone CR and mixed terminators, preserving edge/interior blank lines under both wrapping settings; retain and run its bare-CR nonzero-width guard. Add lone-CR normalization at the existing pre-segmentation site and require equal emitted lines, font size and fitted dimensions against LF. Confirm the lone-CR comparison fails without the added normalization.
- [x] 1.2 Add a CR-versus-LF auto-length tape render comparison using the existing helpers, asserting equal decoded dimensions and pixels; require it to fail if lone CR reaches fitting unchanged.

## 2. Validation

- [ ] 2.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, and `npx --no-install openspec validate --all --strict --no-interactive`; require every command to pass.
