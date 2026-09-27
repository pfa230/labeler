## Context

See `proposal.md` (Why). The reservation today is four font-wide functions in `src/render/helpers.rs`: `ascent_overflow_em` (`:978`) and `descent_overflow_em` (`:989`) read the OS/2 ascender, cap height and descender; `overflow_em` (`:1019`) combines them per alignment for the fitter; `pad_em`/`pad_pt` (`:995`, `:1004`) give the emitter its inset. None of them sees a string. `overflow_em` feeds `block_height` (`:1072`), which answers `text_fits` (`:654`), the one-line floor in `layout_text` (`:810`) and `TextFit.height_units` (`:853-867`), and it feeds the closed-form `max_lines` budget (`:825`) directly. The emitter calls `pad_pt` again at `src/render/mod.rs:2242`, reloading the font instance to recompute a number the fitter already used.

The lines are available at every one of those points. `text_fits` breaks them per candidate size, `layout_text` holds the emitted lines before it builds `TextFit`, and `render_text_item` emits `TextFit.lines` verbatim.

Measured on `fonts/InterVariable.ttf` with fontTools and HarfBuzz, independently of this codebase [verified: `uv run --with fonttools --with uharfbuzz`, font units of 2048 per em, cap height 1490, typographic ascender 1984, descender −494]:

| glyph | top | bottom | note |
| --- | --- | --- | --- |
| `H E L I X T M A` | 1490 | 0 | flat capitals end exactly on the metric box |
| `O Q 0` | 1510–1514 | −20 to −24 | round overshoot, about 0.01em each way |
| `h b l` | 1490 | 0 to −24 | Inter's lowercase ascenders stop at cap height |
| `i` | 1524 | 0 | the dot clears cap height by 34 units |
| `É` (wght 400, opsz 20) | 1928 | 0 | 438 units above cap height |
| `É` (wght 700, opsz 20) | 1939.7 | 0 | the accent moves with the instance |
| `Å` | 2042 | 0 | above the ascender, clipped under today's `top` pad |
| `Ǻ` | 2269–2285 | 0 | 285 units past today's pad |
| `g` | 1076–1132 | −432 to −442 | inside the descender band |
| `.` | 233–323 | −13 to −19 | the ellipsis marker inks below the baseline |

Those numbers are the problem in miniature: `HELIX` needs nothing, `É` needs 0.214em above, `Å` needs more than the band provides, and every string pays 0.2412em on each side regardless.

## Goals / Non-Goals

**Goals:**

- One measurement of a block's outer ink, computed once per set of lines being judged, read by size selection, the line budget, the `fail` verdict, the intrinsic height and the emitted inset.
- The measurement sees the glyphs Typst draws, positions included, at the instance Typst draws them with.
- `center` emission is byte-identical to today's for every input that resolves the same font size, the same lines and the same box, so its baseline stays glyph-independent by construction rather than by test. A height-bound centred item does not qualify: the smaller reservation lets it pick a larger size, which changes its emission. Centred `HELIX` with ample width, a range of 10 to 20 pt and a 16 pt box fits at 20 pt under the measured reservation (a metric block of 14.55 pt and nothing reserved) and at 13.0 pt under today's (`1.2099 × s ≤ 16`).

**Non-Goals:**

- Characters the font does not map (#254). They measure as the font's `.notdef`, which is what the width measurement already does. Inter's `.notdef` inks from −416 to 1856 [verified: fontTools], so an unmapped character reserves close to the old band at its position; that is incidental, not a guarantee about the fallback glyph Typst draws.
- Shaping the width measurement. `text_width` (`helpers.rs:932`) sums unshaped advances and ignores kerning; changing it changes every line break and every width-bound size, which is a different change with a different blast radius.
- Any author-facing control, as the issue rules out.

## Decisions

### 1. Reserve the block's outer ink, not the font's band and not the ink box

The block's metric box stays the thing that is aligned, and the reservation becomes exactly the ink that leaves it: `a`, the highest ink above the first line's cap-height top, and `d`, the lowest ink below the last baseline, each maximised over every line and floored at zero. `top` and `bottom` reserve `a + d`, `center` reserves `2 × max(a, d)`, for the reason #245 gave: centring splits slack evenly, so each side must absorb its own ink alone.

Alternatives:

- **Keep the font band** (today). Charges 0.4824em to strings that need nothing and still clips `Å` and `Ǻ`, which ink past the band. It is the issue.
- **Align the ink box** (#127's `top-edge: "bounds"`/`bottom-edge: "bounds"`). Makes the baseline follow the glyphs at every alignment, including `center`, which ADR-0045 measured breaking the shared baseline of labels off one roll. The issue rules it out.
- **Sum each line's own overflow.** Charges a first line's descender and a last line's accent, which sit inside the metric box between the lines. It over-reserves every multi-line block and is what the issue's "without adding a font-wide reservation to every line" forbids in spirit.
- **Only first-line `a` and last-line `d`.** Equal to the chosen rule at any sane pitch, and wrong under a pitch small enough that an interior line pokes past the edge (`text-line-spacing` admits 0.5). Taking the maximum over all lines costs nothing extra, since each line is shaped anyway.
- **Never move a block outward, but also pull it inward when ink falls short** (a signed inset). That is ink-box alignment at the aligned edge, #127 again, and would lift `ace` to the top edge. The inset is floored at zero.

### 2. Shape each line with rustybuzz the way Typst does, segments included, rather than map characters one by one

The obvious implementation, and the one matching `text_width`, maps each `char` through `cmap` and reads its glyph's bounding box. It under-measures what Typst draws. HarfBuzz composes `E` + U+0301 into `Eacute`, whose top is 1928, while the per-character route measures the unpositioned `acutecomb` at 1558: 0.18em short, which is an accent clipped at a flush top edge that today's band would have protected. It also misses contextual substitutions: `12:30` shapes its colon to `colon.case`, 227 units higher than the `colon` that `cmap` names [verified: HarfBuzz on the bundled font]. NFC-normalising first would fix the first case and not the second, and not any mark sequence without a precomposed form.

Shaping the whole line as one buffer is not enough either. Typst does not hand a line to HarfBuzz whole: `shape_range` (`typst-layout` 0.15.1, `src/inline/shaping.rs:719-770`) first splits it by BiDi level and by script, a character of generic script (`Common`, `Inherited`, `Unknown`) joining the segment around it, and shapes each segment separately. The difference is not cosmetic. `αH\u0301` shaped as one buffer guesses Greek for the whole line and leaves the acute unpositioned at 1535 units; Typst shapes `H\u0301` as its own Latin segment and GPOS lifts the acute 393 units, to 1928 [verified: HarfBuzz on the bundled font at wght 400, opsz 20]. At 20 pt a whole-line measurement under-reserves by 3.84 pt with every character mapped.

Nor is shaping each line apart. Typst collects an item's lines as one paragraph, joined by `\n` (`src/inline/collect.rs:185-188`), and merges adjacent text of equal styles into one segment (`collect.rs:281-289`); the emitted `#text("…")` children set no style, so the whole block is one text for `shape_range`. A run of characters of no specific script therefore takes the script of the first line after it that has one: `0\u0301` over `α` is shaped in a Greek run, where Inter leaves the acute unraised, and on its own it is shaped under the default script, which raises the acute 393 units, 3.84 pt at 20 pt [verified: the multi-line calibration below, and a 40 pt box refused under per-line shaping]. An earlier revision of this design shaped lines apart and claimed the bundled font could not show the difference; its probe covered only a script carried forward to digits and punctuation, and it was wrong.

So the measurement reproduces Typst's pipeline for the emitted block, step for step:

- **Text.** The emitted lines' no-break forms (`to_nonbreaking`, as the emitter writes them) joined by `\n`.
- **BiDi.** `unicode_bidi::BidiInfo::new(text, Some(LTR))` over that whole text, as `prepare` does (`src/inline/prepare.rs:73-78`) with labeler's default direction.
- **Segmentation.** `shape_range`'s loop over the whole text, grouping by level and by `UnicodeScript::script` with Typst's `is_generic_script`/`is_compatible` rule copied verbatim, so a run may cross a line break.
- **Shaping.** `shape_segment` for each run: nothing for a run of only newlines, tabs or default ignorables (`typst::text::is_default_ignorable`); otherwise one `UnicodeBuffer` with the run's direction, language `en` (Typst's default; labeler sets none), `guess_segment_properties` for the script, `REMOVE_DEFAULT_IGNORABLES`, and no features. Typst's `features()` (`typst-library` `src/text/mod.rs:1396`) adds a feature only when a text setting departs from HarfBuzz's defaults, and the emitted source sets none, so its list is empty too. A `.notdef` sequence is trimmed and handed to the next font family, as Typst does; newlines, tabs and ignorables draw nothing there, and any other unmapped character measures as this font's `.notdef` (#254).
- **Lines.** Each line takes the runs it overlaps, up to its trimmed `\n` (`line.rs` `collect_range`). A run wholly inside the line is used as shaped; a run the line cuts keeps its glyphs where `slice_safe_to_break` allows and is shaped again on the line's piece alone where it does not (`ShapedText::reshape`), both copied.

`unicode-bidi` 0.3.18, `unicode-script` 0.5.8 and `rustybuzz` 0.20.1 are all in `Cargo.lock` already, through Typst, on the same `ttf-parser` 0.25.1 this crate uses. Making them direct dependencies at those versions adds no crate to the tree. `rustybuzz::Face::from_face` wraps a `ttf_parser::Face` and derefs to it, so `instance()` can return the shaping face and every existing measurement keeps reading through the deref unchanged.

Alternatives: call Typst's own shaper. `shape_range` lives in `typst-layout`'s private `inline` module (`src/lib.rs:7`, `mod inline;`), so it is not callable. Compile a Typst document per candidate and read the frame: up to 76 compiles per text item in the size search, for every item of every label in a batch, which trades a copy of 50 lines for a render that is orders of magnitude slower. Copying the pipeline is the cheaper way to be exact, and Decision 3a is what keeps the copy honest.

A glyph's ink is `ttf_parser::Face::glyph_bounding_box` offset by its shaped `y_offset`. That call outlines the glyph and is documented as "affected by variation axes" [verified: `ttf-parser-0.25.1/src/lib.rs:2170`], so it reads the instance set by `set_variation`, not the default master. A glyph with no outline (a space) contributes nothing.

Correction found in implementation: `glyph_bounding_box` returns `i16` bounds and truncates an instance's fractional extent, so the bold `É`, which tops out at 1939.7 units [fontTools], measures 1939. Up to a unit short is 0.0137 pt at 40 pt and 0.025 pt at 72 pt, past the 0.01 pt tolerance the spec bounds containment by. The measurement therefore traces the same instanced outline with a `ttf_parser::OutlineBuilder` and keeps the fractional extent, each curve's own extremum included (`glyph_ink` in `helpers.rs`). The calibration oracle of Decision 3a bounds Typst's glyphs with the same function, so the two still compare where glyphs are placed rather than how a box is rounded.

HarfBuzz context reaching across a line break is reproduced the same way Typst handles it: a cut run keeps its glyphs only where HarfBuzz marks the boundary safe to break, and is shaped again on the line's piece otherwise, with the same flags read from the same buffer.

### 3a. The copy is calibrated against Typst's own frame

A reproduced pipeline can drift from the original, on a Typst upgrade most of all, and the failure is silent: an under-measured accent is clipped and nothing errors. So one test compiles a corpus of single lines through the real renderer and reads where Typst actually put every glyph. A frame glyph's own `y_offset` cannot be used for that: Typst groups shaped glyphs by vertical offset, moves each group's offset into the text item's position (`pos = Point::new(offset, top + shift - y_offset)`, `typst-layout` `src/inline/shaping.rs:358`) and emits every `Glyph` with `y_offset: Em::zero()` (`:429`). Reading `Glyph::y_offset` would therefore see the acute of `αH\u0301` at 0 and agree with a whole-line measurement that is 3.84 pt wrong. The oracle instead walks the frame tree recursively, accumulating each group's transform and each item's position down to every text item. It places each glyph's outline, bounded from that item's font at that item's size, at the item's accumulated position plus the glyph's `x_offset` and advance, and expresses the outline's top and bottom relative to the line's baseline, read from the laid-out line frame's baseline and carried through the same accumulated transform. It asserts that the extremes equal what the measurement returns for the same line, to the same 0.01 pt tolerance. The corpus covers each way the two could part: `HELIX`, `Émile`, `E\u0301` (composition), `12:30` (`calt` to `colon.case`), `αH\u0301` and a Cyrillic–Latin mix (segmentation), `g\u0323` (mark below), `Ǻ` (above the ascender), `gyp`, and `...`. It is the same kind of guard `block_height_matches_typst_layout` already is for line stacking. `αH\u0301` stays in the corpus as the regression for this: the test is trusted only once the segmented measurement passes it and a whole-line shaping of the same line fails it, by 3.84 pt at 20 pt.

A second test does the same for blocks of several lines. Typst inlines each line frame into its paragraph's, so only the first baseline is read from the frame; line `i`'s baseline is that plus `i − 1` pitches, the stacking `block_height_matches_typst_layout` pins, and a glyph belongs to the line whose baseline its text item sits nearest. Its corpus crosses scripts at line breaks (`0\u0301` then `α`, and the reverse), mixes segmentation within lines, and runs marks and contextual forms across breaks. `0\u0301` over `α` is its regression: shaping the lines apart fails it by 3.84 pt.

### 3. One function produces the measurement, and `TextFit` carries it to the emitter

A single helper takes the face, the size, the pitch and a slice of lines and returns the block's `a` and `d` in points. `block_height` becomes `metric_block_height + reserve(vertical, a, d)` over the same lines, so `text_fits`, the line budget and the intrinsic height cannot drift from each other. `metric_block_height` stays as it is, because `block_height_matches_typst_layout` (`mod.rs:6251`) compares it against Typst's own line stacking and that calibration has no reservation in it.

`TextFit` gains the emitted block's `a` and `d`. `render_text_item` pads `top` by `a` and `bottom` by `d` from the `TextFit` it is given, and `center` by nothing, exactly as `pad_block` does today. `pad_pt`, `pad_em`, `overflow_em`, `ascent_overflow_em` and `descent_overflow_em` are deleted. That also removes the font reload `pad_pt` did at emission, along with the short-circuit it needed to keep `center` from depending on the font: the measurement now happens in `layout_text`, which already loads the instance for every text item.

The inset is written into the Typst source at full precision, as `derived_leading_pt` already is, rather than through the `{:.2}` format `pad_block` uses today. Two-decimal rounding can move ink 0.005 pt past the edge on top of the fit's 0.01 pt tolerance, and the spec bounds containment by the tolerance alone.

Alternative: keep a separate emitter-side computation, as today. It is how the two passes came to disagree about fonts before (ADR-0049), and the issue asks for one rule that cannot disagree.

### 4. The `ellipsis` budget becomes a downward search over emitted forms

The closed form `floor((H − reserve·s + leading) / pitch)` assumes the reserve is known before the kept lines are. It no longer is: dropping the line with the `g` removes the descender, and appending `...` adds 14 units of ink below the baseline. So the budget tries `k` downward and keeps the first whose emitted form (over-wide lines shortened in place, the marker on line `k` when `k < n`) fits. The first `k` tried that fits is the largest, which is the spec's rule.

Fitting is not monotonic in `k`, so there is no one-line floor. Today's standalone check (`helpers.rs:810-818`) refuses an item whose first line cannot fit alone, before the budget runs. Under the measured rule a longer run can fit where the first line alone does not: centred `g\u0323\nHELIX\nHELIX` at 20 pt, pitch 0.5, in 28 pt, needs about 34.55 pt for three lines, 24.83 pt for two (the dot, 865 units deep, lies inside the two-line block) and 31.45 pt for one (centring must reserve twice the dot) [verified: HarfBuzz depths on the bundled font, arithmetic per the spec's formulas]. The check is deleted, and `text_does_not_fit` is raised only when the search has tried every `k` down to 1 and none fits. The width refusal for a box narrower than `...` stays where it is; it does not depend on `k`.

The search grows with the line count, not its square. The contract lays out every hard break, so a small request can carry 10,000 lines, and re-measuring every prefix would shape about 50 million lines. A line's ink can depend on the lines after it (Decision 2), so each `k` tried measures its own emitted block; what keeps that linear is how few `k` are tried and how far down the first one is:

- **The search stops within a few steps.** Every step down removes a pitch of metric height while the reservation stays under the font's largest ink, so once the steps taken exceed that ink divided by the pitch the metric block alone frees enough room and a run fits. At the default pitch that is two or three tries; only an authored `line_spacing` far below 1 raises it, and that is a constant of the template, not of the data. The block judged whole before the budget runs is not measured again for `k = n`.
- **The metric block bounds the search before any shaping.** Reservations are never negative, so a run whose `metric_block(k)` exceeds `H + 0.01 pt` cannot fit: the cutoff carries the same tolerance as the fit comparison, or it would refuse a block the comparison accepts. At 20 pt `HELIX` has a metric block of 14.55078125 pt and no reservation, so it fits a 14.54578125 pt box within tolerance, and a cutoff at `H` alone would reject it before measuring anything. The search starts at `min(n, k_metric)`, where `k_metric` is the last line whose baseline lies no more than 0.01 pt below the box's bottom edge, and no line past it is shaped. `text_fits` applies the same `H + 0.01 pt` test first and shapes nothing when the metric block alone overflows it.

  Both boundaries are pinned by regressions: 20 pt `HELIX` is accepted in a box 0.005 pt shorter than its metric block and refused in one 0.015 pt shorter, once through `text_fits` and once through the `ellipsis` budget with `k_metric` at the boundary.

So one judgement shapes at most `min(n, k_metric)` lines per `k` tried. The size search measures one block per candidate whose widths and metric height already fit, and those are few for the same reason, which is the order of work it already does today, breaking and measuring the width of every line at every candidate.

This is verified by count, not by clock: a test-only counter on the line-shaping helper asserts that a 10,000-line value in a two-line box shapes at most four lines in the budget and none in `text_fits`, and that a 10,000-line value in a box too short for it at a pitch small enough to admit all of them shapes each line at most twice. A timing assertion would be flaky on CI; a count is exact.

Shortening stays width-driven. The search drops whole lines and never trims characters to shed an accent or a descender, because a label that silently turns `Égypt` into `Ég...` to fit vertically reports a width problem that does not exist.

### 5. `top` and `bottom` baselines move by the ink past their edge, and `center` baselines do not

The issue's acceptance list asks both for a flush top edge on `HELIX` and for "a stable baseline" across ordinary strings. At `top` both cannot hold for every pair of strings: flush `HELIX` and a whole `É` put their baselines 438 units apart, and any rule that keeps the two on one baseline has to inset `HELIX` for an accent it does not carry, which is the defect. The issue's own scope bullets settle it: `top` is "inset only by ink that the emitted block actually carries above its metric top". The stable-baseline criterion is therefore read as the `center` guarantee, which the issue's test list names ("centered baseline stability") and which is what #127 broke and #133 restored, plus the zero floor of Decision 1: no alignment moves a block toward its edge because its ink falls short.

What moves under `top` in Inter is stated in the spec, because it is more than accents: the dots of `i` and `j` clear cap height by 34 to 49 units, and round capitals and figures overshoot by about 20. Top-aligned `line` and `lane` differ by 0.017em, 0.33 pt at 20 pt. That is the rule working, and a slot whose baselines must not move at all is what `center` is for.

### 6. Spec shape: one ADDED requirement, two MODIFIED, and old scenario names kept

The layout-sizing capability already owns the reservation (*Vertical fitting reserves the ink each alignment can expose*, from #245 and #363) and the overflow policy (*Text is laid out against the box it will get…*). Both state the font-band model as normative, so both are MODIFIED with their full text. The placement rule, "What `alignment.vertical` aligns" in frozen `docs/SPEC.md` §3.1, has never been migrated; under the first-touch rule it arrives as an ADDED requirement carrying the complete post-change alignment contract and naming the paragraph it supersedes. Placement moves out of the fitting requirement into that one, so no rule is stated twice.

The fitting requirement keeps its name. "The ink each alignment can expose" still describes it, and a rename needs a RENAMED block that no change in `openspec/changes/archive/` has used. The validator refuses a MODIFIED block that drops a scenario the current spec has [verified: `openspec validate --strict`], so five scenarios keep their names over rewritten bodies, each saying so, following the precedent of *A long word is split, not overflowed*. The last bullet of *The default pitch tightens existing multi-line items*, that a height-bound item "settles at a size no smaller than before, because the reservation never grows under that font", is dropped: the reservation now does grow for `Å` and `Ǻ`, and the claim compared against a pitch model #363 retired.

## Risks / Trade-offs

- **A size or line count now depends on the value.** Two names in one batch can print at different sizes where today they print alike, and `content`-height stacks under a `flow` container shift with the ink their text carries. → This is the contract the issue asks for, marked BREAKING, and the previous rule forbidding it is replaced by name. Fixed `font_size` items change line count and verdict only, never size.
- **Templates change appearance without being edited.** A height-bound item without accents or descenders grows by up to 0.4824em of its size, and a `top` or `bottom` item moves toward its edge. → No migration, per the pre-1.0 rule. `avery5163_asset_tag.yaml` and any test pinned to a fitted size (`center_aligned_multiline_auto_shrink_descender_fits_and_closes_stroke`, `mod.rs:9430`, pins 21.0 pt) are re-measured and re-pinned to the new value, never loosened to a range.
- **The fit is only as good as the shaping match.** A segmentation, direction, language or feature difference can move a glyph vertically, as `αH\u0301` shows. → Decision 2 copies Typst's pipeline, and Decision 3a's calibration test compares the measurement against glyph offsets read from Typst's own frame. That test, not a raster, is what catches a shaping mismatch.
- **A raster check can certify an image that is clipped twice.** Comparing an exact box against a taller box of the same alignment proves nothing: the pad keeps the text at the same distance from the clip, so an under-reserved accent is cut identically in both (`pad_block`, `mod.rs:667`). → The spec defines containment against an unclipped reference: the same source with only that item's `clip: true` removed, its box placed clear of the label edges, so the text lands at the same raster phase and any ink the clip would cut shows up in a row wholly outside the box. The oracle is proved by sabotage before it is trusted: rendering the same source with the emitted pad reduced by one raster row's height must fail it at the top, and likewise for the bottom pad and a descender.
- **Performance of the size search.** Each candidate size now shapes its lines, up to 76 candidates at 0.5 pt steps. → Only when the metric block already fits (Decision 4), each line once, and the face is parsed once and re-instanced per candidate, as `largest_fitting_font` already does (`helpers.rs:679-713`). The work stays linear in the line count, the same order as today's per-candidate breaking.
- **Tests that pass against the old code prove nothing.** → The scenarios split into proofs and guards, and each proof must be run red against the unmodified tree first. Proofs: the flush `HELIX` (inset 4.82 pt today), the cap-height box accepting `HELIX` under `fail` (refused today), `HÉLIX` settling below `HELIX` (equal today), `Hg\nÉH` resolving a bare metric block (reserved today), `É` at weight 700 and 400 differing (equal today), `Ǻ` contained in its exact box (clipped by 285 units today) and `g\u0323\nHELIX\nHELIX` keeping two lines (one today, by the closed-form budget). `αH\u0301` passes against today's font-band pad, so it proves nothing about the old tree; it must be shown red against a whole-line shaping of the new measurement instead. Guards, green before and after: the contained `É` and `gyp`, the centred baselines and the byte-identical headroom case.
- **Measurement numbers need an oracle that is not the code under test.** → The fontTools and HarfBuzz figures in Context come from independent implementations and are what unit-level expectations are checked against, at the instance each test renders.
- **Unmapped characters.** A fallback glyph Typst draws can ink anywhere, and the measurement sees Inter's `.notdef`. → Named in the spec as the one gap, and it is #254's.

## Migration Plan

None. The service is stateless and nothing persisted changes shape. Rollback is a revert of the one commit.
