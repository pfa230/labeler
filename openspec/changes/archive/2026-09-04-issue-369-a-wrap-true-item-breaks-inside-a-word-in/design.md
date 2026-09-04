## Context

See proposal.md for motivation. What shapes the approach is code that already exists,
and the fact that the fix is a deletion plus a two-clause contract change rather than
new machinery.

- **`text_fits` already checks line width** (`src/render/helpers.rs:670-674`): every
  emitted line wider than the box fails the candidate size. Under `wrap: true` that
  check is today reachable only for an over-wide glyph, because `wrap_text` has
  chopped every over-wide word to size before it runs (the chunker pushes only when
  the chunk is non-empty, so a glyph wider than the box still yields an over-wide
  chunk). Removing the chunking loops extends it to over-wide words with no new
  predicate and no new parameter.
- **`largest_fitting_font` already re-breaks at each candidate**
  (`src/render/helpers.rs:679-715`) and `layout_text` already breaks again at the
  chosen size (`:777`). Step 2's "re-broken at each candidate's glyph advances"
  therefore needs no code change to cover the newly unmasked width failures; the
  loop keeps descending for a width miss exactly as it does for a height miss.
- **The ellipsis path already ellipsizes each over-wide line in place**
  (`src/render/helpers.rs:836-841`): once the chunking loops stop hiding the
  condition, an over-wide word at the floor is shortened to `<prefix>...` by the
  code that already shortens an over-wide `wrap: false` line, on whatever line it
  sits on, so a fitting later line is emitted untouched. The `width_pt <
  ellipsis_width` and one-line-height refusals above it (`:801-819`) already cover
  the cannot-shorten floor. No new policy and no new branch.
- **`wrap: false` never enters `wrap_text`'s word path** (`break_lines`, `:637-652`):
  it returns segments verbatim, so the width check already fails on them and
  `overflow` already resolves them. Deleting code inside `wrap_text` cannot change
  that path, which is why the existing `wrap: false` tests pin it untouched.
- **Load-time validation never measures text** (`layout-sizing`, "Load-time
  validation and render-time resolution are one algorithm"): a content source stands
  in at the available extent. This change alters only which candidate size the
  render-time fitter accepts, so load accepts exactly what it accepted before.

## Goals / Non-Goals

**Goals:**

- An over-wide word spends the `font_size` range before anything breaks it, and the
  chosen size is the largest 0.5 pt step at which the word fits whole.
- At the floor (or at a fixed size) the existing `overflow` policy decides, with
  the existing marker and the existing `text_does_not_fit` reason.
- No emitted line is ever a mid-word fragment without a marker, under either
  policy and at any size.

**Non-Goals:**

- Hyphenation, dictionary breaks, or any new mid-word rule with a marker. The issue
  deletes chunking rather than marking it.
- A keep-together control, a minimum-shrink floor distinct from `font_size.min`, or
  any new field or `overflow` value.
- Any change to `wrap: false`, to single-glyph overflow, to dynamic-width
  requirement accounting, or to the reason set.

## Decisions

### Delete both chunking loops; keep words whole on their own line

An over-wide word becomes a line of its own that is wider than the box. That line
then fails the width check, which is what drives the shrink loop down, and at the
floor it is what the ellipsis path shortens or `fail` refuses.

**Why deletion over gating the loops behind "only at `min`"**: a floor-only fallback
keeps the exact behavior this change removes, a silent mid-word break with no
marker, for the one case where the author has least room left, and it keeps two
loops whose only remaining purpose is producing output the contract forbids. The
ellipsis path already produces a marked, shortened form for that case.

**Why not shrink-then-chunk (chunk what still overflows at `min`)**: same outcome
with an extra rule. Anything that fits after shrinking fits whole; anything that
does not is an overflow, and overflows already have two authored resolutions.

### No new fit predicate; the existing width check is the predicate

`text_fits` needs no edit. Its line-width loop today catches only the over-wide
glyph under `wrap: true` and becomes the mechanism by which word width joins the fit
judgement. One predicate judges height and width together at every candidate, so
the two cannot drift.

**Alternative, not taken**: checking the longest word up front and pre-selecting a
size from it. That duplicates the width check in a second place with its own
rounding and tolerance, and it must still re-break at each candidate because other
words reflow as size falls. Judging the emitted lines at each candidate subsumes
it.

### No spec change for `text-wrap-flag`

That capability owns the `wrap` field's name, default, and migration, and points
at `layout-sizing` for the layout consequences without restating the splitting
rule. The delta therefore touches `layout-sizing` alone. A reader looking up what
`wrap: true` does with an over-wide word finds one rule in one place.

### The one test naming chunking is rewritten, not deleted

`layout_text_ellipsizes_every_over_wide_line_not_only_the_last`
(`src/render/helpers.rs:1602-1637`) exists to pin that every over-wide line is
ellipsized, not only the last, which stays the contract (see the in-place shortening
paragraph in the delta). Only its setup depends on chunking: `"WW"` splits into two
lines solely through the deleted loop. Rewriting it with a value that wraps without
chunking (two over-wide words, or a hard break) keeps every assertion meaningful.
Deleting it would drop the only pin on the mid-block marker behavior this change
makes common. Its neighbour,
`layout_text_ellipsis_leaves_a_final_line_that_fits_intact` (`:1643-1674`), already
pins the fitting-last-line case with `"W i"` and survives untouched.

### Assumptions recorded rather than asked

- The issue's "single glyph wider than the box remains unresolvable ... exactly as
  the current contract says" is read as the existing over-wide-glyph scenario and
  its ellipsis/fail pair, which this delta keeps verbatim.
- "No emitted line is ever a mid-word fragment without a marker" is read as
  covering the ellipsis marker as the only permitted marker; no hyphenation
  scheme is introduced to create another. Blank lines are exempt: they carry no
  glyphs and no marker, and the requirement mandates their line boxes.

## Risks / Trade-offs

- **Labels that relied on chunking now shrink further or ellipsize.** A long word
  that today renders split across lines at near-`max` will render whole at a
  smaller size, or as `<prefix>...` where the range cannot cover it. Accepted: the
  old output read as a spelling error, and pre-1.0 breaking changes carry no
  migration.
- **More shrink-loop candidates are evaluated.** Width misses now continue the
  descent instead of stopping early, so pathological boxes run more of the up to
  ~76 iterations. The loop is unchanged in shape and each step is the same
  measure it already performed; no new cost class.
- **Multi-word values may settle smaller.** A value whose longest word forces the
  size down carries every other word with it, since one size fits the whole item.
  That is what a single `font_size` for the item means; per-word sizes are not a
  concept in the model.
- **Whitespace-only and empty segments are unaffected.** `wrap_text`'s early
  return for blank segments stays; only the two word-overflow branches go.
