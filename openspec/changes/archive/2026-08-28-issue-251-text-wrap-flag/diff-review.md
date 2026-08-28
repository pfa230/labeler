# Diff review: issue-251-text-wrap-flag

AUTHOR: agy and claude
REVIEWER: codex

VERDICT: APPROVE

## What was reviewed

The renderer half, on current `main`. This code was written against the pre-#226 renderer and carried
across two rebases while `main` rewrote the same subsystem — #226 unified the layout pipeline
(ADR-0080/0081/0082), #200 moved input derivation into the service, #263 landed packed children
(ADR-0083) and ADR-0084 added the centred ink reservation. Every earlier diff review judged a tree that
no longer exists, so this round assumed nothing about the implementation.

The input list's layout-derived control and its `truncated_elsewhere` flag remain on this tree by
design: they are [#269](https://github.com/pfa230/labeler/issues/269), and `specs/template-inputs/`
in this change describes that interim state rather than changing it. The UI is untouched.

## Round 1: REVISE, two findings, both repository hygiene

- **[BLOCKER] Raw agent transcripts staged for commit.** The four `.agent-*.{conversation,json}` files
  were staged before `c29049c` gitignored them and returned through a `git stash`, which `.gitignore`
  cannot prevent for paths already in the index. `diff-review-1.md` was a raw `codex exec` capture
  rather than findings. Both violate the rule that a transcript belongs in a log, not the repository.
- **[MAJOR] Checked boxes describing the discarded plan.** `tasks.md` claimed an ADR numbered 0083 and
  a UI driven solely from declared parameters — the pre-#200 plan. A checked box is what the next
  reader trusts instead of redoing the work, so a false one is worse than an unchecked one.

Both fixed; the reviewer re-checked each against the tree rather than the task text and returned
`RECHECK: PASS` (`.gitignore:37`; `tasks.md:60-69` against `docs/adr/0085-text-wrap-flag.md:20-35`,
`docs/adr/README.md:92-94`, `docs/AUTHORING.md:393-397`).

## What it found nothing wrong with, having looked

This is the part worth recording, because it is the risk the two rebases created:

- **The merge.** Conflicts were resolved by keeping both sides, which is correct where each side adds a
  distinct test and wrong where both edited the same one. Three such tests were found and repaired
  before this review (`a_height_bound_fit_reserves_the_overflow` had ended up asserting both
  `aligned < centered` and `aligned == centered`). The reviewer was asked to hunt for any missed, in
  code as well as tests, and found none.
- **Contract conformance** — segmentation of every `\n` segment, CRLF normalised with a lone `\r` left
  to #259, blank lines laid out rather than trimmed, the block-level wrapper carrying the fitted size,
  the trailing `#linebreak()`, and the marker firing whenever any line is dropped.
- **ADR-0084's centred reservation and #263's packing**, which this change rewrote the functions for.
- **Test sensitivity.** The reviewer ran its own mutations rather than trusting the claim: removing the
  trailing linebreak and moving `size:` back to the inner runs each fail the block-height test.

## Gates on the reviewed tree

`cargo fmt --check` clean, `cargo clippy --all-targets --all-features` clean, `cargo test` 671 passed,
`npm run lint` clean, `npm test` 400 passed.
