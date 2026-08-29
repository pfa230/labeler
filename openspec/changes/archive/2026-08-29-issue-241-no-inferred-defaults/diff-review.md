# Diff review — issue-241-no-inferred-defaults

AUTHOR: agy
REVIEWER: claude

- **Rounds**: 2
- **Tool restrictions**: reviewer read-only; every fix was made by the implementer, never by the reviewer.
- **Scope**: `git diff HEAD` in the worktree plus `docs/adr/0087-explicit-parameter-defaults.md`, audited
  against the six capability deltas, `tasks.md`, and `AGENTS.md`.

## Round 1

Two blocking findings and several majors, all confirmed against the code before being raised:

1. **Wrong reason slug on the write path.** The new load-time refusals lived in `TryFrom<RawParamSpec>`,
   inside `parse_template`, and `parse_and_validate` maps every `parse_template` error to
   `template_parse_failed`. A `PUT` carrying `default: "{message}"` therefore reported that the YAML did
   not parse, where `interpolation-tokens` requires `template_validation_failed`. Masked by a test that
   asserted only the status code.
2. **`null` became an omission for every parameter type**, not just `datetime`: a declared `string`
   receiving `null` went from printing empty to `422 MissingField`, unlisted as breaking and contrary to
   `interpolation-tokens`, which still stringifies a resolved `null` to the empty string.
3. The `enum` load-time default check was dropped for non-string defaults, moving a decidable load-time
   refusal to a per-request failure.
4. Four tests passed against the pre-change implementation — each asserted only that output was
   non-empty, which the removed inference also produced.
5. `tests/fixtures/templates/brother_24mm_printed_on.yaml` was migrated with `default: "{sys.now}"`,
   which `proposal.md` explicitly forbids, and which propped up two tests still asserting the old rule.
6. Task 6.3 was checked with one of its three edits unmade; tasks 5.5 and 5.17 claimed assertions that
   were absent.

All returned to the implementer, which fixed them. Verified after: the checks moved to
`TemplateContent::validate()`, `null` is scoped to `datetime`, the `enum` refusal is restored for
non-string defaults, the fixture is reverted, and the CSV test now asserts the outline container is
inactive rather than that a PDF exists.

## Round 2

1. **Task 6.4 unperformed.** `openspec/specs/datetime-params/spec.md`'s Purpose still described the
   render-instant default this change removes, and the delta carried no `## Purpose` block, so archive
   would have published a capability contradicting its own first requirement. `archive-merge-check.sh`
   compares requirements only and would not have caught it.
2. **`docs/AUTHORING.md` misdescribed the migration this change prescribes**, calling
   `default: "{sys.now}"` "the current render instant" when it is the render date at local midnight — so
   a `time: true` parameter defaulted that way prints `00:00`. A new trap, in the file task 6.3 was
   scoped to make honest.
3. Two checked boxes over partly-performed work; a dead `_now` parameter this change introduced; a caller
   bug reported as a template bug; and the coercion rule duplicated rather than shared.

Returned to the implementer and fixed. Verified after: the Purpose no longer claims a render-instant
default, `AUTHORING.md` states the render date and spells out the `time: true` consequence, and `_now` is
gone.

## Gates, run by the reviewer on the rebased tree

- `cargo fmt --check`: clean
- `cargo clippy --all-targets --all-features`: 0 warnings
- `cargo test`: 687 passed, 0 failed
- `ui` vitest: 411 passed, 0 failed

## Verdict

VERDICT: APPROVE
