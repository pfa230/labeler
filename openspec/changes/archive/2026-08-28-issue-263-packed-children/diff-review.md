# Diff review

AUTHOR: agy (implementation), claude-orchestrator (merge resolution)
REVIEWER: claude-subprocess (rounds 1-2), claude-subagent (merge round)
VERDICT: APPROVE
ROUNDS: 3

Three rounds against the implementation, recorded as `diff-review-1.md` and `diff-review-2.md`,
then a fourth scoped to the merge of `main` (`de11197`) into this change.

## What the rounds found

**Round 1** raised three blocking findings and six others. The measurement path formatted
`arrange_flow`'s index into the `when`-filtered child list straight into `items[N]`, so an overflow
named the wrong child whenever a gated-off sibling preceded it; task 7.2 was checked with no record
and a 395-line acceptance test that asserted nothing; and the headline spec scenarios reached the
gated case only through a `FlowChildInput { active: false }` flag production code never sets. The
non-blocking six covered `resolve_placement_box` being bypassed rather than diverted, uncommitted
agent transcripts, an ADR sentence claiming the `wrap` this change excludes, a dead duplicate of the
packed-`line` refusal, a bare `flow:` selecting the absolute arrangement, and an incidental panic.

**Round 2** raised one blocking finding: a trailing zero-extent packed child was placed one `gap`
past the last occupying child and bounds-checked there, so every `content`-sized flow container with
an empty trailing child failed to render. Reproduced against a running server before it went back to
the implementer: the same child rendered at 200 in the leading and middle positions and returned 422
`item_out_of_frame` at the trailing one. The fix is a lookahead: a non-occupying child followed by an
occupying one sits where that child will sit, and one with nothing after it sits at the cursor,
because the gap it was being offered by is one no occupying child will ever be laid down after.
Proved red before green at both levels, by reverting the lookahead and watching
`flow_trailing_zero_extent_child_places_at_cursor_without_gap` and the HTTP-level acceptance test
fail, then restoring it.

**The merge round** verified that main's input derivation reads `placement.extent` and never
`placement.at`, which this change turned into an `Option`, and recurses into containers through
`items`, so packed children are reached; that the `src/templates.rs` test-block concatenation kept
all 96 of main's test functions in order with no duplicates and no losses, and `src/render/mod.rs`
all 163; that the `src/openapi.rs` union added exactly `Flow` and `FlowDirection` and dropped
nothing from either side; that main's and this change's `render/mod.rs` edits touch disjoint
functions; and that the `MODIFIED` delta still resolves, because main touched `datetime-params` and
`template-inputs` while this change targets `layout-sizing`. Its one blocking finding was that a
failed agent run had overwritten a tracked transcript `de11197` committed, replacing the #200 review
record with a quota error, which would have ridden into the landing commit; it was restored and
re-checked.

## What was filed rather than fixed

- #265, to untrack the four agent transcripts `de11197` committed.
- #266, a composition hazard between this change and `de11197`: thumbnail placeholder data ignores
  `when:`, so two mutually-exclusive packed siblings can both activate and overrun a container that
  every real label renders.
- #267, the missing assertions that flow packing and service-derived inputs compose.

## Acceptance evidence

Recorded under task 7.2 in `tasks.md`. Every render was opened and checked, and re-run after the
merge: all byte-identical, with the two deliberate overflow cases still returning 422
`item_out_of_frame` at the expected JSON paths.
