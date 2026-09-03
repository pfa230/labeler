# Design: Rename `InvalidOptionValue` to `InvalidEnumValue`

## Context

`src/errors.rs:18` defines `CODE_INVALID_OPTION_VALUE = "InvalidOptionValue"` and `src/errors.rs:203` exposes it through `AppError::invalid_option_value(selection, allowed)`. Two call sites raise it: `src/render/mod.rs:356` coerces a supplied `data` value against `ParamType::Enum { values }` and returns the error when the value is not a member of `values` (live path), and `src/render/mod.rs:1219` validates an option-map selection through `normalize_option` declared at `src/render/mod.rs:1211` (dead path after #214 retires the option map). The frozen spec documents the live behaviour at `docs/SPEC.md:566-567` ("If a request supplies a value for an `enum` parameter that is not member of its declared `values` list, the request is rejected with `422 InvalidOptionValue`"), the error-code table at `docs/SPEC.md:683` (`InvalidOptionValue | 422 | Option selection not allowed by the template.`), and the CSV import clause at `docs/SPEC.md:1069` ("and a disallowed enum value fails the row (`BatchInvalid` / `InvalidOptionValue`)"). The code has no `details.reason` and never had one, so `docs/SPEC.md` §10.1 has no row for it.

The vocabulary mismatch is intentional scope for this change: the option map is retired elsewhere, and the error code should name the surviving concept (`enum`) rather than the retired one (`option`). The issue explicitly constrains the rename to the `code` string alone.

See `proposal.md` for why this change exists and what it changes.

## Goals / Non-Goals

- Goals: rename the `code` string to `InvalidEnumValue` everywhere it is produced, asserted, documented in specs, and rendered in the per-row `BatchInvalid` envelope; keep status `422`, message `"Invalid option selection"`, and `details` keys `selection`/`allowed` byte-identical; remove the dead name from `src/` and `ui/src/`; publish the complete post-change contract as a first-touch `ADDED` requirement superseding the frozen rows.
- Non-Goals: reshaping `details` (renaming `selection`/`allowed` to better names), adding a `reason` slug, changing the message, migrating the message to match the new code, touching `docs/SPEC.md` or `docs/adr/`, handling stored data, or producing a migration shim. A `details` reshape is a different change and is out of scope here; the first attempt at this work was rejected for doing exactly that.

## Decisions

- **Rename only `code`, not `details` or `message`.** The issue forbids a `details` reshape and codex caught that substitution in the first attempt. Keeping `details.selection` and `details.allowed` unchanged avoids a second breaking dimension in one change and keeps the contract minimal. Alternative (rename `details` keys to `param`/`values`) was rejected as out of scope.
- **New capability `enum-validation` for the `ADDED` requirement, plus a `MODIFIED` on `template-inputs`.** `enum-validation` holds the canonical contract: it supersedes the frozen §5 sentence, the §10 row and the § CSV import clause and restates the whole behaviour (trigger, status, code, message, `details` shape, batch per-row reporting, no `reason`). Placing it in its own capability rather than overloading `template-inputs` keeps the error contract separate from the input-list capability that merely references it. The `MODIFIED` on `template-inputs` updates its two prose mentions so the spec tree does not drift: the "Rendering is unchanged" paragraph and the "An out-of-range enum still fails..." scenario. Alternative (new capability `error-codes` or reusing `request-error-envelope`) was considered; `enum-validation` was chosen because the code is about enum validation, not request admission, and a dedicated validation capability mirrors how `param-resolution` owns `TemplateInvalid`.
  - Supersession is explicit: the `ADDED` requirement names `docs/SPEC.md:566-567`, `docs/SPEC.md:683` and `docs/SPEC.md:1069`, notes that `docs/SPEC.md` §10.1 has no row for this code so no row is superseded there, and leaves every other row of §10, every row of §10.1 and the rest of § CSV import authoritative. The CSV import site is prose, not a table row, so the "row" framing is not used there.
- **Keep the Rust identifier rename mechanically parallel to the code string.** `CODE_INVALID_OPTION_VALUE` becomes `CODE_INVALID_ENUM_VALUE` and `invalid_option_value` becomes `invalid_enum_value`. This is a find-and-replace across the codebase, not a semantic refactor. Alternative (keep the old ident and only change the string literal) would leave `InvalidOptionValue` in the source, violating the acceptance criterion that the old name appears nowhere in `src/` or `ui/src/`.
- **Leave the dead `normalize_option` path renamed, not deleted.** `src/render/mod.rs:1219` is dead after the option map is retired, but this change does not delete dead code it did not create; it only renames the error that path would have returned. Deletion belongs to the option-map retirement (#214).
- **Spec-first, code-second.** The spec delta is written to describe the error after the rename (new `code`, old `message`/`details`). Implementation then makes the code match the spec, not the other way around.

## Risks / Trade-offs

- **Breaking clients matching on `code`.** Any client switching on `error.code === "InvalidOptionValue"` will stop matching. Mitigation: pre-1.0 breaking is permitted, and the change is limited to the `code` string; a client matching on status or `details` is unaffected. No shim is provided, per the project's breaking-change rule.
- **Missed string occurrence.** A grep that skips `ui/src`, `openspec/specs`, or `docs/AUTHORING.md` would leave a stray `InvalidOptionValue`. Mitigation: implementation task explicitly covers `src/`, `ui/src/`, `openspec/changes/issue-338-rename-invalidoptionvalue-to-invalidenum/specs/template-inputs/spec.md`, and `docs/AUTHORING.md:753,766` (documentation-only alignment); `openspec/specs/` is written by archive and must not be edited by the implementer.
- **Spec duplication drift.** The same code appears in `enum-validation` (canonical) and `template-inputs` (reference). Mitigation: the reference text in `template-inputs` is a one-line citation of the canonical code, not a redefinition, so it cannot drift in semantics, only in the literal.
- **Batch envelope confusion.** The per-row `BatchInvalid` entry carries the renamed code per label, while the top-level `BatchInvalid` code stays unchanged. Mitigation: scenarios in `enum-validation` explicitly cover the per-row `BatchInvalid` shape.

## Migration Plan

- Pre-1.0 breaking change, no data migration. `store.rs:154-168` does not apply (no stored error codes).
- Rollout: merge the single commit. Rollback is revert.
- No feature flag.

## Open Questions

- None. The `code` string, status, message, and `details` keys are all pinned by the issue. The only decision left is the capability name, which this design fixes as `enum-validation`.
