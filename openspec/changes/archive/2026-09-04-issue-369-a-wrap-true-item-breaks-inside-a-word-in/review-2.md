# Plan review: issue-369

No `ANSWERS.md` at the worktree root. No files edited; my one experiment ran on a copy under `/tmp`, now deleted (`git status` clean apart from the untracked change folder).

## What I verified as sound

The mechanism holds up against the code. `text_fits`'s width loop (`src/render/helpers.rs:670-674`) is real and currently masked under `wrap: true` only by the chunker, exactly as `design.md:7-13` now states. Deleting both loops leaves an over-wide word alone on its line, because `current_width + space_width + word_width <= width_pt` (`src/render/helpers.rs:930`) is already false once `current_width > width_pt`. `largest_fitting_font` re-breaks per candidate (`:679-715`) and `layout_text` re-breaks at the chosen size (`:777`), so no new predicate is needed. The ellipsis path shortens each over-wide line in place (`:836-842`) with the two cannot-shorten refusals above it (`:801-819`).

Every citation in `proposal.md` and `design.md` checks out against the file, so review-1's F5 and F6 are fixed. F1 is fixed: the delta now carries 19 of the 20 published scenarios plus 5 new ones, and the ADR-0045 supersession paragraph is back (`specs/layout-sizing/spec.md:45-50`). F3 is fixed (`:152` now scopes to "every emitted line carrying glyphs"). F4 is fixed by the new paragraph at `:81-88` and the scenario at `:156-163`.

The impact claim about tests holds. I traced every `wrap: true` layout test: `layout_text_ellipsis_leaves_a_final_line_that_fits_intact` (`src/render/helpers.rs:1643-1674`) survives, because `"W i"` produces `["W","i"]` with or without the chunker; `layout_text_multiline_wraps_and_width_is_longest_line`, `whitespace_only_segment_keeps_its_line`, the centre-alignment pair, `tests/acceptance_issue_263.rs:374` and `:870`, and the catalog templates all use ordinary words in boxes far wider than any word, so none is affected. `layout_text_ellipsizes_every_over_wide_line_not_only_the_last` is indeed the only casualty, and the design's suggested replacement value works: two over-wide words yield two lines, both ellipsized to `...`, keeping both assertions live. `tasks.md`'s absence is correct (`tools/openspec-loop/workflow/run-stage.sh:295`), `text-wrap-flag` genuinely needs no delta, and no non-frozen doc restates the splitting rule.

## Findings

**F1 (blocker). The delta cannot be validated or archived: it drops a scenario by omission, which the tooling refuses.**

`openspec validate issue-369-... --strict --no-interactive` fails today:

```
✗ [ERROR] layout-sizing/spec.md: MODIFIED "Text is laid out against the box it will get, and what
  does not fit is authored" omits scenario(s) the current spec still has:
  "A long word is split, not overflowed".
```

This is not advisory. `dist/core/specs-apply.js:333-336` throws the same loss at archive time, and CI runs `openspec validate --all --strict --no-interactive` (`.github/workflows/ci.yml:124`). Name matching is exact (`requirement-blocks.js:269-289`, `scenarioNameAt` at `:308-319`), and there is no scenario-level `REMOVED` mechanism: a MODIFIED block must carry every scenario name the published requirement holds. The plan deliberately deletes `openspec/specs/layout-sizing/spec.md:804-808`, so as written the change stalls at the archive stage. I confirmed on a `/tmp` copy that re-adding the heading with a rewritten body makes the change validate clean, and that no other scenario is missing.

The repo already has the idiom for this: `A hugging parent hugs the emitted lines, not the trimmed ones` kept its name through an inverted body and says so in-line (`openspec/specs/layout-sizing/spec.md:864`).

**F2. Step 2's new clause uses "emitted" against the meaning the same requirement gives the word, which inverts the clause this change turns on.**

`specs/layout-sizing/spec.md:26-28` reads "picks the largest size in `[min, max]` at which the broken block fits the box height **and every emitted line fits the box width**". The same requirement defines the emitted lines as the post-overflow output: `:34` ("Every line produced by step 1 ... gets its own line box") and `:39-41` ("the item's **intrinsic height** SHALL be the block height of the lines emitted **after** the overflow policy has been applied"). Read that way, step 2's predicate is evaluated against lines step 3 has not yet produced, and post-overflow lines fit the box width by construction, so the predicate is vacuous and the shrink loop never descends for a width miss. That is the opposite of the change. The clause the sentence already contains, "the broken block", is the term that should carry through.

**F3. The shortening prose contradicts the in-place paragraph the change adds.**

`specs/layout-sizing/spec.md:69` says "Shortening keeps the lines that fit and appends `...` to the last", and `:71-72` says the marker "lands at the end of the last retained line whatever that line holds", both unconditional. `:81-83` then says an over-wide line "is shortened in place, wherever it sits in the block ... so the marker may sit on a middle line while a later fitting line is emitted untouched". Both describe where the marker goes, and only one can be the rule as stated. The code runs both paths (`src/render/helpers.rs:836-842`: over-wide, or dropped-and-last). `:69-72` came over unchanged from the published spec, where the conflict could not arise under `wrap: true`; adding `:81-83` creates it.

## Non-blocking notes

`proposal.md:69-70` says the test rewrite keeps "the per-line assertions and the block-fits check". The test has no per-line assertions: `src/render/helpers.rs:1626-1636` asserts `m.lines.len() > 1` and `m.width_units <= box_w + 1e-4`. The intent is clear enough to act on; the wording just overstates what is there.

## Required changes

**1. Restore the dropped scenario under its exact heading, with an inverted body.** In `openspec/changes/issue-369-a-wrap-true-item-breaks-inside-a-word-in/specs/layout-sizing/spec.md`, insert the following block immediately before `#### Scenario: An over-wide word shrinks whole instead of breaking` (`:124`), preserving the heading text character for character:

```
#### Scenario: A long word is split, not overflowed

- **WHEN** a `wrap: true` `text` carries a single word far wider than its box, and the box is
  tall enough for the resulting lines
- **THEN** the word is not split: it stays whole on one line, step 2 spends the `font_size` range
  on it, and whatever still does not fit at the floor is resolved by the item's `overflow` policy
- **AND** this scenario keeps its name from the superseded version, where the word was split
  character by character and neither policy was consulted
```

Then, in `proposal.md`, extend the `layout-sizing` bullet under "Modified Capabilities" (`:47-52`) with one sentence recording why the scenario is inverted rather than deleted: a `MODIFIED` block must carry every scenario name the published requirement holds, because `openspec validate --strict` and the archive step both refuse a block that drops one, so the scenario keeps its name and states the new behaviour. After the edit, `openspec validate issue-369-a-wrap-true-item-breaks-inside-a-word-in --strict --no-interactive` must report the change valid.

**2. Fix step 2's predicate wording.** In the same delta file, at `:26-28`, replace `and every emitted line fits the box width` with `and every line step 1 broke it into fits the box width`. Leave the rest of the sentence unchanged.

**3. Reconcile the shortening prose with the in-place paragraph.** In the same delta file, replace the sentence at `:69`, `Shortening keeps the lines that fit and appends `...` to the last, trimming characters until it fits.`, with:

```
Shortening has two independent paths. Lines that do not fit the block are dropped, and the marker is
appended to the last retained line; a line that is wider than the box is shortened where it sits,
whether or not anything was dropped. Either path trims characters until the line and the marker fit.
```

Then, at `:71-72`, change `and it lands at the end of the last retained line whatever that line holds` to `and for a dropped line it lands at the end of the last retained line whatever that line holds`, so the unconditional claim is scoped to the path it describes.

The author applies these and no further review follows.

VERDICT: APPROVE_WITH_CHANGES
