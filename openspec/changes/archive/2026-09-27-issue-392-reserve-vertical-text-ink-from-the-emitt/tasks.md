## 1. Dependencies and the shaping face

- [x] 1.1 Add `rustybuzz = "0.20.1"`, `unicode-bidi = "0.3.18"` and `unicode-script = "0.5.8"` to `[dependencies]` in `Cargo.toml`, the versions `Cargo.lock` already carries through Typst. Done when `cargo build` succeeds and `git diff Cargo.lock` adds no `[[package]]` entry.
- [x] 1.2 Make `instance()` in `src/render/helpers.rs` return the shaping face (`rustybuzz::Face::from_face`), instanced at the item's `wght` and at `opsz` = size, so every existing measurement keeps reading `ttf_parser::Face` through the deref (design Decision 2). Done when `cargo test` passes with no test changed.

## 2. Measuring a line's ink the way Typst shapes it

- [x] 2.1 Add one line-shaping helper that copies Typst's pipeline per line (design Decision 2): `unicode_bidi::BidiInfo::new(line, Some(LTR))`; grouping by BiDi level and `UnicodeScript::script` with Typst's `is_generic_script`/`is_compatible` rule copied verbatim; each segment shaped in its own `UnicodeBuffer` with the segment's direction, language `en`, `guess_segment_properties`, `REMOVE_DEFAULT_IGNORABLES` and no features. It returns the line's `rise` and `fall` in points: each glyph's `glyph_bounding_box` offset by its shaped `y_offset`, maximised over the line. A line that draws no ink returns nothing.
- [x] 2.2 Put a test-only counter on that helper recording how many lines it shapes, for use by 5.6.
- [x] 2.3 Unit-test the helper against the independent fontTools/HarfBuzz figures in design Context, at the instance each test uses: `HELIX` rises exactly to cap height and falls 0; `É` at wght 400, opsz 20 rises to 1928 units and at wght 700 to 1939.7; `Ǻ` rises past the typographic ascender; `g` falls 432–442 units; `...` falls below the baseline; `É` rises to 1928, the same as `É`; `12:30` rises from `colon.case`, not `colon`; `αH́` rises to 1928 and not 1535.
- [x] 2.4 Add the block measurement: with `baseline_i = cap_height(s) + (i − 1) × pitch(s)`, `a = max(0, maxᵢ(rise_i − baseline_i))` and `d = max(0, maxᵢ(baseline_i + fall_i) − metric_block(n, s))`, with `reserve(vertical)` = `a + d` for `top`/`bottom` and `2 × max(a, d)` for `center`. Unit-test that `Hg\nÉH` at the default pitch gives `a = d = 0`, and that `ÉH\nHg` gives `a` = `É`'s accent above cap height and `d` = the depth of `g`.

## 3. Calibration against Typst's own frame

- [x] 3.1 Add the calibration test of design Decision 3a. It compiles each corpus line through the real renderer and walks the laid-out frame recursively, accumulating every group transform and item position down to each text item. It places each glyph's outline, bounded from that item's font at that item's size, at the item's accumulated position plus the glyph's `x_offset` and advance. It takes top and bottom relative to the line frame's baseline, carried through the same accumulated transform, and never reads `Glyph::y_offset`. It asserts that the extremes equal what 2.1 returns, within 0.01 pt. Corpus: `HELIX`, `Émile`, `É`, `12:30`, `αH́`, a Cyrillic–Latin mix, `g̣`, `Ǻ`, `gyp`, `...`.
- [x] 3.2 In the same test module, assert that a whole-line shaping of `αH́` (one buffer, guessed properties) fails the calibration comparison by about 3.84 pt at 20 pt, while the segmented measurement passes it. That makes the oracle's ability to tell the two apart a permanent test.

## 4. Fitting, line budget and intrinsic height

- [x] 4.1 Change `block_height` to `metric_block_height + reserve(vertical)` over the lines being judged, measured on the face instanced at that size. Leave `metric_block_height` unchanged, so `block_height_matches_typst_layout` (`src/render/mod.rs:6251`) passes unmodified.
- [x] 4.2 In `text_fits`, test `metric_block(n, s) > H + 0.01 pt` before any shaping and return false without shaping when it holds. Otherwise judge `metric_block + reserve` against `H + 0.01 pt` on the lines broken at that size.
- [x] 4.3 In `largest_fitting_font`, keep parsing the face once and re-instancing it per candidate, and let each candidate's `text_fits` measure the lines broken at that candidate on that candidate's instance.
- [x] 4.4 In `layout_text`, delete the standalone one-line height check (`helpers.rs:810-818`) and the closed-form `max_lines` (`:825`). Replace them with the downward search of design Decision 4:
  - start at `min(n, k_metric)`, where `k_metric` is the last line whose baseline lies no more than 0.01 pt below the box's bottom edge;
  - the emitted form of `k` lines is the first `k` lines, each over-wide line shortened in place, with the marker appended to line `k` when `k < n`;
  - fill the prefix maxima of `rise_i − baseline_i` and `baseline_i + fall_i` in one upward pass;
  - shape each line's plain form at most once and one marker form per `k` tried;
  - keep the first `k` that fits, and raise `text_does_not_fit` only when no `k` from `k_metric` down to 1 fits.

  The refusal of a box narrower than `...` stays as it is.
- [x] 4.5 Add the emitted block's `a` and `d` to `TextFit`, and compute `height_units` as `metric_block + reserve(vertical)` on the lines finally emitted.
- [x] 4.6 Delete `ascent_overflow_em`, `descent_overflow_em`, `overflow_em`, `pad_em` and `pad_pt`. Done when `rg 'overflow_em|pad_em|pad_pt|ascent_overflow_em|descent_overflow_em' src` returns nothing.

## 5. Emission

- [x] 5.1 In `render_text_item` (`src/render/mod.rs`), take the pad from the `TextFit` it is given: `top` by `a`, `bottom` by `d`, `center` by nothing. Write it into the Typst source at full precision, as `derived_leading_pt` is written, not through `{:.2}`. Remove the `helpers::pad_pt` call, so emission no longer reloads the font instance.
- [x] 5.2 Rewrite `the_emitted_pad_is_the_aligned_edge_metric` (`mod.rs:2704`) for the measured pad. Top-aligned `HELIX` emits no `#pad`. Top-aligned `Édgy` emits a top pad equal to its measured accent height at 20 pt, not 4.82 pt. Bottom-aligned `gjpqy` emits a bottom pad equal to its measured descender depth. Centred text emits no `#pad`.

## 6. Containment oracle

- [x] 6.1 Add the test helper that judges containment as the spec defines it. It renders the label twice from the same Typst source: once as emitted, and once with only that item's `clip: true` removed, the item's box placed clear of every label edge. It fails if the unclipped render puts ink in any raster row lying wholly outside the item's box.
- [x] 6.2 Add sabotage tests for the oracle. Render a top-aligned `É` with its emitted top pad reduced by one raster row's height and assert the oracle fails. Do the same for a bottom-aligned `gyp` with its bottom pad reduced.

## 7. Tests for the alignment requirement

- [x] 7.1 Unaccented capitals sit flush: top-aligned `HELIX` at a fixed 20 pt in a box taller than its cap height has its metric top on the box's top edge, and its first inked raster row is within one row of that edge.
- [x] 7.2 An accented capital is inset by its own accent and stays whole: top-aligned `Émile` at 20 pt, in a box exactly its intrinsic height, is inset by `É`'s accent height at that instance and is contained (6.1).
- [x] 7.3 A descender is inset by its own depth and stays whole: bottom-aligned `gyp` at 20 pt, in a box exactly its intrinsic height, is inset by its lowest descender at that instance and is contained.
- [x] 7.4 A centred baseline does not follow the glyphs: one centred item at a fixed size renders `HELIX`, `Émile`, `testj` and `gyp`. None emits a pad, all four baselines fall on the same raster row, and the ink gaps to the box edges differ between the values.
- [x] 7.5 A block is never pulled toward its aligned edge: top-aligned `ace` and `HELIX` in the same box share a baseline row, and `ace`'s first inked row lies below the box's top edge.
- [x] 7.6 Ink between the lines moves nothing. Top-aligned, `Hg\nÉH` emits no pad and `ÉH\nHg` emits a top pad of `É`'s accent height. Bottom-aligned, `ÉH\nHg` emits a bottom pad of `g`'s depth and `Hg\nÉH` emits none.

## 8. Tests for the fitting requirement

- [x] 8.1 Aligned edges are unchanged (the scenario keeps its old name): top-aligned `HELIX`, fixed 20 pt, `overflow: fail`, renders in a box exactly `cap_height(20 pt)` tall.
- [x] 8.2 Auto-shrink sees the emitted ink: top-aligned `HELIX` and `HÉLIX` with a `font_size` range, in a box too short for either at the maximum. `HÉLIX` settles smaller than `HELIX` and is contained.
- [x] 8.3 A centred block auto-shrunk into a tight box keeps its descenders: re-pin `center_aligned_multiline_auto_shrink_descender_fits_and_closes_stroke` (`mod.rs:9430`, currently 21.0 pt) to the size the measured rule chooses, the largest 0.5 pt step with `metric_block(2, s) + 2 × max(a, d) ≤ 51.31 pt`. Pin it to that exact value, never to a range, and assert the block is contained.
- [x] 8.4 A centred multiline block's line budget counts the reserve. Replace `layout_text_center_aligned_multiline_line_budget_reserves_overflow` (`helpers.rs:1870`): centred, fixed size, box exactly `metric_block(3, s)`. `HELIX\nHELIX\nHELIX` keeps three lines with no marker. `HELIX\nHELIX\nHELgX` keeps two, the second ending in `...`. Under `overflow: fail` the first renders and the second fails `text_does_not_fit`, and every render that succeeds is contained.
- [x] 8.5 Ink between the lines reserves nothing: `size: [content, content]`, top-aligned. `Hg\nÉH` resolves exactly `metric_block(2, s)` and `ÉH\nHg` resolves `metric_block(2, s) + a + d`.
- [x] 8.6 A centred item asking for a content height grows by the reservation. Replace `layout_text_center_aligned_content_height_includes_reservation` (`helpers.rs:2149`):
  - `HELIX` resolves `cap_height(s)` under `top` and `center`;
  - top-aligned `Égypt` resolves `cap_height(s) + a + d` and is contained;
  - centred `Égypt` resolves `cap_height(s) + 2 × max(a, d)`, with ink gaps of `max(a, d) − a` above and `max(a, d) − d` below.
- [x] 8.7 An asymmetric font reserves twice its larger overflow: `É` with `size: [content, content]` resolves `cap_height(s) + 2a` centred and `cap_height(s) + a` top-aligned.
- [x] 8.8 The reservation is read from the instance rendered: top-aligned content-height `É` at weight 400 and at weight 700 resolves, for each weight, `cap_height(s)` plus that instance's accent height, and the two heights differ.
- [x] 8.9 A glyph outside the declared band still clips (the scenario keeps its old name): top-aligned `Ǻ` in a box exactly its intrinsic height renders and is contained.
- [x] 8.10 A value of ten thousand lines. With a fixed size, top-aligned, `wrap: false`, `overflow: ellipsis` and a box holding two lines, a value of 10,000 short lines keeps two lines, the second ending in `...`. The 2.2 counter records at most four shaped lines in the budget and none in `text_fits`. A 10,000-line value at a pitch small enough that every baseline lies within the box, too short for it, shapes each line at most twice.
- [x] 8.11 The metric cutoff honours the fit tolerance: top-aligned `HELIX`, fixed 20 pt. It renders in a 14.54578125 pt box and fails `text_does_not_fit` in a 14.53578125 pt box, under `overflow: fail` and again under `overflow: ellipsis`.
- [x] 8.12 A mark after a change of script is measured in its own segment: top-aligned `αH́` at 20 pt, in a box exactly its intrinsic height, emits the same top pad as `H́` alone and is contained.
- [x] 8.13 A character the font does not map: a value containing a character the bundled font lacks is fitted and rendered, not refused. Its reservation at that position equals what the font's `.notdef` outline gives.
- [x] 8.14 A centred item with headroom is unaffected. Capture, on the unmodified base commit, the emitted Typst source for a centred single-line item whose box fits both the old font-band reservation and the measured one at `font_size.max`. Pin that source as the expected literal, and assert the changed tree emits it byte for byte.
- [x] 8.15 A height-bound centred item picks a larger size: centred `HELIX`, ample width, `font_size: { min: 10, max: 20 }`, in a 16 pt box renders at 20 pt.

## 9. Tests for the text-layout requirement

- [x] 9.1 A longer run fits where the first line alone does not: centred `wrap: false` `g̣\nHELIX\nHELIX`, fixed 20 pt, `line_spacing: 0.5`, `overflow: ellipsis`, in a 28 pt box. It keeps two lines, the second ending in `...`, is not refused, and is contained.
- [x] 9.2 A box's verdict follows the ink the value carries: top-aligned, fixed 20 pt, box exactly `cap_height(20 pt)` tall. `HELIX` renders under `overflow: fail`. `Égypt` fails `text_does_not_fit` under both `fail` and `ellipsis`.
- [x] 9.3 An empty value is one empty line: its intrinsic height is `cap_height(s)`, with no reservation.
- [x] 9.4 Update `layout_text_ellipsis_refuses_a_box_shorter_than_one_line` (`helpers.rs:1541`) so that it asserts the no-run-fits refusal rather than a one-line floor, and keep *A box too short for one line cannot be shortened* passing.
- [x] 9.5 Replace `overflow_is_the_ink_outside_the_cap_height_line` (`helpers.rs:2347`) and the #124/#245 raster tests in `src/render/mod.rs` that assert the 0.2412em pad or the font-band reservation. Their new assertions are the measured values above, or 6.1 containment, never a loosened tolerance.
- [x] 9.6 Re-measure every test that renders `tests/fixtures/templates/avery5163_asset_tag.yaml` or pins a fitted size or line count changed by this rule, and re-pin each to its new exact value.

## 10. Red-first evidence

- [x] 10.1 Run the proof tests against the unmodified base commit, in a separate worktree with its own `target/`, and save the failing output to `.agent-runs/issue-392-red.log`. The proof tests are 7.1, 8.1, 8.2, 8.5, 8.8, 8.9 and 9.1. Each must fail there. A proof that passes on the base commit is a broken test and is rewritten before continuing.

## 11. Documentation

- [x] 11.1 Rewrite `docs/AUTHORING.md` for the measured reservation:
  - the overflow-policy paragraph at line 375, which says the policy is judged on the metric model and not on glyph outlines;
  - the §11 "Text is smaller than expected" entry;
  - the §11 "Lowercase text sits lower in its slot than all-caps" entry.

  State that `top`/`bottom` inset by the ink the value carries past that edge, that `center` keeps a glyph-independent baseline, and that characters the font lacks remain outside the guarantee (#254).

## 12. Gates

- [x] 12.1 `cargo fmt --check` passes.
- [x] 12.2 `cargo clippy --all-targets --all-features` passes with no new `#[allow(clippy::...)]`.
- [x] 12.3 `cargo test` passes.
- [x] 12.4 `openspec validate issue-392-reserve-vertical-text-ink-from-the-emitt --strict` passes.
