## Why

Implements [#369](https://github.com/pfa230/labeler/issues/369).

A `wrap: true` text item breaks inside a word rather than shrinking, even when its
`font_size` range has headroom left. `wrap_text` (`src/render/helpers.rs:895-965`)
character-chunks any word wider than the line at whatever size it was called with; the
caller then judges the candidate size on block height alone, which the chopped lines
satisfy, so the shrink loop stops early. The output reads as a spelling error
(`Refrigera / tion`) with no marker and no error, and the author must size
`font_size.max` against the longest single word any data might contain instead of
against the design.

## What Changes

- **Word width joins the fit predicate.** With the chunking loops gone, an over-wide
  word leaves a line wider than the box, the existing width check in `text_fits`
  (`src/render/helpers.rs:670-674`) fails, and the shrink loop keeps descending toward
  `font_size.min` instead of stopping at the first size whose height works.
- **At the floor, `overflow` decides unchanged.** When the longest word still exceeds
  the width at `font_size.min`, or at a fixed `font_size` where there is no range to
  spend, the item is over its box and its `overflow` policy applies: `ellipsis` (the
  default) shortens the line to `<prefix>...` via the existing ellipsize path
  (`src/render/helpers.rs:836-841`), `fail` returns `text_does_not_fit`.
- **Character-chunking is deleted, not kept as a floor.** Both chunking loops in
  `wrap_text` (`src/render/helpers.rs:910-925` and `941-954`) go. A mid-word break
  with no hyphen and no marker reads as a typo on a printed label; an ellipsis reads
  as a truncation and the `fail` policy can refuse it outright.
- **BREAKING** (pre-1.0, no migration): a template that today renders
  `Refrigera / tion` will render `Refrigeration` at a smaller size where the range
  allows it, and `Refrigera...` or a 4xx where it does not. No second spelling, no
  opt-out.

Out of scope: `wrap: false` is unchanged; a single glyph wider than the box remains
unresolvable by any breaking rule and reaches `overflow` like anything else;
dynamic-width (`single`, `width: content`) labels inherit the behavior with no special
case. No new field, no new `overflow` value, no new error reason.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `layout-sizing`: requirement "Text is laid out against the box it will get, and what
  does not fit is authored" — Step 1 (Break) no longer splits a word wider than the
  box character by character, and Step 2 (Shrink) judges a candidate size on line
  width as well as block height. The superseded `A long word is split, not overflowed`
  scenario keeps its name and states the new behaviour rather than being deleted: a
  `MODIFIED` block must carry every scenario name the published requirement holds,
  because `openspec validate --strict` and the archive step both refuse a block that
  drops one. `text-wrap-flag` points at this requirement for the
  layout consequences of the flag but does not restate the splitting rule, so it
  needs no delta.

## Impact

- `src/render/helpers.rs`: `wrap_text` (delete both chunking loops; over-wide words
  stay whole on their own line), no change needed in `text_fits`/`largest_fitting_font`
  beyond what the unmasked width check already does; existing ellipsis path handles
  the floor.
- Tests: new render/HTTP tests asserting emitted lines and status for the
  shrink-to-fit, floor-ellipsis, floor-fail, and fixed-size cases, shown red before
  green; existing `wrap: false` tests pass untouched. One existing `wrap: true`
  test does not survive the deletion and is rewritten, not deleted:
  `layout_text_ellipsizes_every_over_wide_line_not_only_the_last`
  (`src/render/helpers.rs:1602-1637`) lays out `"WW"` in a box between `...` and
  `W` wide and asserts the chunker split it into two lines. With chunking gone
  `"WW"` stays one over-wide line and is ellipsized to one line, so the assertion
  fails. Its intent, every over-wide line ellipsized in place, stays live and is
  re-expressed with a value that still wraps without chunking (two over-wide words
  or a hard break), keeping the per-line assertions and the block-fits check.
- `docs/SPEC.md` is frozen and is not edited; `docs/adr/` is frozen and takes no
  entry. Rationale lives here and in `design.md`.
