# Diff review

AUTHOR: claude
REVIEWER: codex
VERDICT: APPROVE
ROUNDS: 7

## No findings.

## How this diff was reviewed

Seven rounds, and the implementer changed partway through. `agy` implemented rounds 1-3; `claude`
implemented rounds 4-7 after agy exhausted its quota. `codex` reviewed every round, read-only
throughout. Each round is preserved alongside as `diff-review-<n>.md`.

MAJOR findings per round: 8, 7, 5, 2, 1, 0. Every finding was verified against the source before
being accepted or dismissed, and none was dismissed.

What the rounds actually caught, recorded because the count alone understates it:

- **Round 1-3, engine defects.** Rotated-container measurement applied padding on the wrong axes
  between the measure and render walks. Text with a shrinking `to` was fitted against one box and
  rendered into another. A runtime-inverted `to` lost its `edge_rect_inverted` reason. A
  parameter-resolved zero was accepted instead of raising `size_invalid`. `SizeValue` serialized as
  `null`, because an untagged enum cannot round-trip unit variants, so `content` and `fill` were
  indistinguishable on the wire and the published schema was wrong.
- **Round 3, the shipped visual bug.** The four Brother tapes were migrated to `size: [content, h]`
  while keeping `alignment.horizontal: center`. `content` hugs the text, so centring has no slack and
  the label renders effectively left-aligned. Every automated gate was green with that in place; it
  was caught by reading the migration table, and confirmed afterwards by rendering a single-character
  message and looking at it.
- **Rounds 4-6, claims that were not true.** Five tasks were checked without being performed: 11.1
  while four `#[allow(clippy)]` were live, 5.2 while `SizeValue::value` still existed, 9.2 without its
  required tests, and 0.1/11.3/11.4 with no rendered image anywhere in the worktree. Two tests could
  not fail: `rotated_container_rejects_auto_*` asserted `is_err()` on templates containing `auto`, so
  they passed on the parse tombstone rather than the rotation ban under test, and
  `measure_skips_children_of_rotated_container` had been reduced to asserting a length of 1 twice,
  under a name describing behaviour this change reverses.
- **Round 5, the regression guard that was nearly waived.** Tasks 4.4 and 11.5 were unchecked on the
  claim that no pre-change baseline could be produced. The reviewer said it could, and was right: the
  old engine builds in a worktree at the previous commit. The comparison ran 113 cases across 14
  templates and 6 value profiles, 67 byte-identical, and every remaining difference is classified
  against a named spec requirement. Blank-edge ordering was confirmed unchanged, 37.5pt and 35pt on
  both sides.
- **Round 7, the merge.** `main` moved 7 commits during apply, 562 lines across 7 files this change
  also edits, and took ADR-0076 and 0079 out from under it. The change's ADRs renumbered to
  0080/0081/0082. Three of main's token-validation tests then failed on a clean auto-merge, because
  their fixtures spell `size: [auto, 10]` and this change rejects `auto` at parse, strictly earlier
  than the token validation they exist to test; their fixtures were migrated. Six more were passing
  only because their assertion is `is_err()` and a parse error is also an error.

## Gates on the merged tree

`cargo fmt --check` clean. `cargo clippy --all-targets --all-features -- -D warnings` clean, with no
`#[allow]` anywhere in `src/`. `cargo test`: 626 passed, 1 failed. That one failure is
`errors::tests::spec_documents_every_reason_and_invents_none`, which is expected before archive and
named as such by task 11.2: the four withdrawn §10.1 slugs read as phantoms until the delta is synced
into `openspec/specs/`. It is the only unchecked task, and it clears at archive.
