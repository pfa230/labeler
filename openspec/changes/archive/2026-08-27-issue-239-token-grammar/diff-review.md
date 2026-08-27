# Diff review

AUTHOR: agy
REVIEWER: claude
VERDICT: APPROVE
ROUNDS: 2

## Scope

The implementation diff for `issue-239-token-grammar`, reviewed against `specs/`, `design.md`, the
issue's acceptance criteria and `AGENTS.md`. 15 files modified, 2 added (`src/interpolation.rs`,
`docs/adr/0079-token-grammar.md`). The reviewer did not edit any file under review; the two tamper
checks below restored their targets and were verified by checksum.

## Gates, run by the reviewer rather than taken from the implementer's report

- `cargo fmt --check`: clean.
- `cargo clippy --all-targets --all-features`: 0 warnings, 0 errors, no `#[allow]` added.
- `cargo test`: 612 passed, 0 failed, 2 ignored.
- `npx vitest run`: 48 files, 407 tests passed.
- `openspec validate issue-239-token-grammar --strict`: valid.
- `.workflow/review-gate-check.sh --plan-only <root> src/...`: passes.

The Rust count is unchanged across round 2 because the new assertions were added inside existing
`#[test]` functions rather than as new ones. That is why two of them were proved red rather than
assumed live; see below.

## Round 1 findings, all closed in round 2

**MAJOR 1. The UI hid every declared parameter from the field walk, not just datetime ones.**
`ui/src/lib/templateFields.ts` excluded any declared parameter while the backend excludes only
datetime ones (`src/render/mod.rs`, `is_datetime_param`). Not cosmetic: `multilineFields` and
`singleLineTextFields` gate on `isDataField`, and `FieldForm.tsx:73` uses their intersection as
`truncatedSomewhere.has(name)` for declared parameters, so a declared `string` param that is
multiline in one option branch and single-line in another silently stopped raising its truncation
warning. Closed: the predicate is now `params[valPath]?.type === "datetime"`
(`ui/src/lib/templateFields.ts:221`), with a test over a declared `string` param across both branches.
**Proved red**: reverting that one line and re-running `templateFields.test.ts` fails 1 of 22; the
file was restored and re-verified.

**MAJOR 2. The refusal message named the replacement for only one of the two old spellings.**
`{datetime.long_date}` suggested `{sys.now:long_date}` but `{printed_on.short_date}`, the datetime
parameter spelling and the one an operator with an existing template most likely carries, said only
"unknown source 'printed_on'". `specs/interpolation-tokens/spec.md` requires both refusals to state
the replacement. Closed in `validate_interpolated_string` (`src/templates.rs`), which is the only
layer holding `params` and therefore the only one able to tell; the message is left unenriched when
the root names nothing, so an arbitrary `{a.b}` is not given a wrong suggestion. **Proved red**:
neutering the enrichment fails `load_time_token_validation_refusals`; file restored and checksum
verified.

**MODERATE 3. Two tokenizers survived.** `interpolate` hand-scanned to the first `}` while load-time
validation used `scan_tokens`, so `{bad{token}` validated as `{token}` at load and errored at render.
Closed: `interpolate` now consumes `scan_tokens` and processes the spans between tokens through
`process_literal_chunk`, which preserves `{{`/`}}` and still returns `400 InvalidRequest` /
`interpolation_syntax` for an unterminated `{` and an unmatched `}`. The trailing span after the last
token is processed too. Tests added for `a}id` and `{bad{token}`.

**MODERATE 4. ADR-0079 did not record superseding ADR-0068's token list**, and claimed the UI scanner
and backend parser were "strictly aligned", which findings 1 and 3 contradicted. Closed: the status
line now names both ADR-0028 and ADR-0068 with the portion each loses, ADR-0068's own header and its
`README.md` row are marked, and the consequence bullet now states what is true.

**MINOR 5. Two spec scenarios had no test, and one grammar hole was open.** Closed: an `image` item
with `src: "logos/{datetime.brand}.png"` is asserted to fail load naming `datetime` as an unknown
source, and `parse` now rejects whitespace-only segments (`{vars. }`, `{sys. }`) as `EmptySegment`,
which is what "no segment may be empty" requires.

## Verified against the spec, not just the diff

Parameter-name liberation (`datetime`, `vars`, `sys` all load as ordinary names); the single captured
instant shared by `{sys.now}` and un-overridden datetime parameters, with all three render paths
passing `with_instants`; request data unable to shadow a declared datetime parameter; `MissingField`
naming the whole token text `<path>:<format>`; the write path inheriting every load-time refusal
through the shared `validate()` and returning `422 TemplateInvalid` / `template_validation_failed`
with nothing stored; both fixtures and `docs/AUTHORING.md` rewritten; all 28 tasks genuinely
performed rather than merely checked.

## Not fixed, deliberately, and not blocking

Two spec scenarios about binding a colon-keyed connector field (`custom:<name>`) through the field
mapping have no automated test. They describe behavior that already works and that this change does
not touch: `ui/src/lib/connectorRows.ts` maps any template field to any connector key. Asserting it
would test the pre-existing mapping, not this change.

## Reviewer note on the record

This verdict was produced through `.workflow/apply-with-agy.sh`, which implements and does not write
`diff-review.md`; only `.workflow/apply.sh` does. This file was written by the reviewer to keep the
verdict checkable by the landing gate rather than living only in an untracked log (#223). The roles
are still distinct and were distinct in fact: agy wrote every line of the implementation, this
session wrote none of it.
