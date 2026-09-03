## 1. Rename error code in Rust sources

- [x] 1.1 Rename `CODE_INVALID_OPTION_VALUE` / `invalid_option_value` to `CODE_INVALID_ENUM_VALUE` / `invalid_enum_value` in `src/errors.rs:18,203`, changing only the `code` string to `InvalidEnumValue` while keeping status `422`, message `Invalid option selection`, and `details` keys `selection`/`allowed` byte-identical (`enum-validation`).
- [x] 1.2 Update the two call sites in `src/render/mod.rs:356` (strict enum coercion) and `src/render/mod.rs:1219` (inside `normalize_option` declared at `src/render/mod.rs:1211`, dead path after #214) to call the renamed constructor; do not delete `normalize_option` or reshape `details`.
- [x] 1.3 Replace every remaining `InvalidOptionValue` literal/assertion in `src/` (`src/templates.rs`, `src/lib.rs`, `src/openapi.rs` if enumerated) and verify `ui/src` contains none; `InvalidOptionValue` must appear nowhere in `src/` or `ui/src/` after the change.

## 2. Align documentation

- [x] 2.1 Update `docs/AUTHORING.md:753` and `docs/AUTHORING.md:766` from `InvalidOptionValue` to `InvalidEnumValue` (documentation-only, no behavioural change).

## 3. Pin the renamed contract with tests

- [x] 3.1 Pin single-label render rejection: a request carrying a value outside an `enum`'s `values` fails with `code` `InvalidEnumValue`, status `422`, message `Invalid option selection`, and `details` exactly `{ selection, allowed }` with byte-identical keys/values as today (`enum-validation` scenario 1).
- [x] 3.2 Pin batch per-row reporting: a `POST /api/batch` row failing the same way appears in `details.failures` with `code` `InvalidEnumValue` and the same message, no `reason` key (`enum-validation` scenario 2); covers `POST /api/import/csv` / `POST /api/print` per-row path by the same constructor.

## 4. Verification

- [x] 4.1 Run `cargo fmt`, `cargo clippy --all-targets --all-features`, and `cargo test` and fix any failures.
