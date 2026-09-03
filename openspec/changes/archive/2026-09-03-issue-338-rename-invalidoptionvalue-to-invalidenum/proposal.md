# Proposal: Rename `InvalidOptionValue` to `InvalidEnumValue`

## Why

Implements [#338](https://github.com/pfa230/labeler/issues/338). `InvalidOptionValue` (`src/errors.rs:18,203`) is raised in two places. One is the dead option-map path (`src/render/mod.rs:1219`, inside `normalize_option` declared at `src/render/mod.rs:1211`) that no caller can reach once the option map is retired. The other is the live check that refuses a value outside an `enum` parameter's declared `values` (`src/render/mod.rs:356`), which stays exactly as it is. The surviving use names a concept the vocabulary no longer has. Renaming the code to `InvalidEnumValue` aligns the contract with the parameter type it actually guards.

## What Changes

- **BREAKING** The error `code` string `InvalidOptionValue` is renamed to `InvalidEnumValue`. Status (`422`), `message` (`"Invalid option selection"`), and every key/value in `details` (`selection`, `allowed`) stay byte-identical. No `details` reshaping.
- The constant, constructor, and every reference in `src/` and `ui/src/` are renamed; `InvalidOptionValue` appears nowhere after the change.
- One first-touch `ADDED` requirement under `enum-validation` supersedes the frozen `docs/SPEC.md` §5 enum-validation sentence (`docs/SPEC.md:566-567`), the `InvalidOptionValue` row of the error-code table in `docs/SPEC.md` §10 (`docs/SPEC.md:683`), and the CSV import clause in `docs/SPEC.md` § CSV import (`docs/SPEC.md:1069`) reading "and a disallowed enum value fails the row (`BatchInvalid` / `InvalidOptionValue`)", restating the complete post-change contract for this code. `docs/SPEC.md` §10.1 has no row for this code and is therefore not superseded except to note that it remains without a `reason`.
- `template-inputs` is `MODIFIED` to carry the new code in the two places its requirement currently documents `InvalidOptionValue` (the "Rendering is unchanged" paragraph and the "An out-of-range enum still fails a render with its own code" scenario).
- No migration, no desugaring, no deprecation window. Pre-1.0 breaking change, as permitted.

## Capabilities

### New Capabilities

- `enum-validation`: the error returned when a request supplies a value for an `enum` parameter that is not a member of its declared `values`. This is the first migration of the enum-validation behaviour out of the frozen spec, so it is an `ADDED` requirement carrying the complete post-change contract.

### Modified Capabilities

- `template-inputs`: updates the two prose references that currently name `InvalidOptionValue` to `InvalidEnumValue` so the input-list leniency description and its scenario stay consistent with the renamed code. No behavioural change beyond the code string.

## Impact

- **Code**: `src/errors.rs` (constant `CODE_INVALID_OPTION_VALUE` and constructor `invalid_option_value`), `src/render/mod.rs` (the two call sites at `:356` and `:1219` — inside `normalize_option` declared at `:1211` — and the `normalize_option` helper that becomes dead after #214), `src/templates.rs` and `src/lib.rs` tests that assert the code string, and any `ui/src` search hit (expected to be none, verified at implementation). `src/openapi.rs` if the code string is enumerated there; otherwise no schema change because the envelope is unchanged.
- **Specs**: one new delta under `enum-validation`, one modified delta under `template-inputs`. `docs/SPEC.md` is not edited (frozen).
- **Clients**: breaking for any client matching on `error.code === "InvalidOptionValue"`. Status, message, and `details` shape are unchanged, so a client matching only on status or `details` is unaffected.
- **Docs**: `docs/AUTHORING.md:753,766` informative troubleshooting rows for `InvalidOptionValue` are aligned to `InvalidEnumValue` as part of this change (documentation-only, no behavioural change).
