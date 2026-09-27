## Why

Implements #392. Every text item reserves the font's whole accent band above its cap-height line and its whole descender band below its baseline, whatever it prints. A top-aligned `HELIX` is therefore pushed down 0.2412em in Inter although no glyph it draws rises above cap height, so an author cannot put capitals flush with a slot's top edge. The same font-wide band is charged when a `font_size` range picks a size, when `ellipsis` decides how many lines survive, and when a `content` height is resolved, so text shrinks or loses lines to protect ink it does not have.

The quantity the reservation stands for, ink falling outside the cap-height-to-baseline metric box, is a property of the emitted glyphs, and the renderer already holds those glyphs when it fits and places a block. It never measures them.

## What Changes

- The ink outside a block's metric box is measured from the lines the block actually emits, after wrapping and ellipsis, shaped at the font instance the item renders with and split into script and direction segments exactly as Typst splits them. Only the block's **outer** ink counts: ink above the first line's cap-height top and ink below the last line's baseline. Ink between lines is inside the box and reserves nothing.
- **BREAKING** A `top`-aligned block is inset by the ink it carries above its metric top, and a `bottom`-aligned block by the ink it carries below its baseline, instead of by the font's accent and descender bands. `HELIX` sits flush with the top of its slot; `Émile` is inset by exactly its accent. A top- or bottom-aligned baseline therefore moves between strings when, and only by as much as, one of them inks past the aligned edge.
- **BREAKING** Fitting reserves `a + d` for `top` and `bottom` and `2 × max(a, d)` for `center`, where `a` and `d` are the emitted block's outer ink above and below. Size selection, the `ellipsis` line budget, the `overflow: fail` verdict and the `content` intrinsic height all read that one quantity. The standalone one-line check goes: under measured ink a two-line block can fit where its first line alone does not, so an item is refused only when no leading run of lines fits. Height-bound text without accents or descenders renders larger than today, fixed-size blocks keep lines they used to drop, and a `content`-height text item shrinks to what its ink needs.
- **BREAKING** A size or a line count can now differ between two values of the same width, because their ink differs. The current contract forbids exactly that ("The renderer SHALL NOT measure per-string glyph bounds to decide a size or a placement"); this change deliberately replaces that sentence.
- `center` keeps placing the fixed metric box, so a centred baseline never depends on the glyphs a string carries. This is the line #127 crossed and #133 restored, and it is not crossed here: no alignment moves a block toward its aligned edge because its ink falls short of the metric box.
- Glyphs the rendering font supplies are contained wherever they ink, including the 211 Inter glyphs that rise above its ascender (`Å`, `Ǻ`). A character the font lacks is drawn by Typst from a fallback font this measurement does not read; that remains #254 and is named as the one hole in the guarantee.
- No template field is added. There is no margin, trim or language option.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `layout-sizing`: *Vertical fitting reserves the ink each alignment can expose* is rewritten around the emitted block's outer ink instead of the font's bands, and gives up placement to the new requirement below. *Text is laid out against the box it will get, and what does not fit is authored* has its overflow policy and its line budget re-stated against that measurement, and loses its one-line floor. A new requirement, *Vertical alignment places a fixed metric box, inset only by the ink the block carries past its aligned edge*, holds the complete post-change alignment contract and supersedes the remaining vertical-alignment paragraph of frozen `docs/SPEC.md` §3.1.

## Impact

- `src/render/helpers.rs`: `ascent_overflow_em`, `descent_overflow_em`, `overflow_em`, `pad_em` and `pad_pt` are replaced by one measurement of the emitted block's outer ink, read by the fit predicate, the line budget, the intrinsic height and the emitted pad. `TextFit` carries the measured insets to the emitter.
- `src/render/mod.rs`: `render_text_item` takes its pad from the `TextFit` it is handed instead of recomputing a font-wide one.
- `Cargo.toml`: `rustybuzz`, `unicode-bidi` and `unicode-script` become direct dependencies at the versions Typst already pulls in (0.20.1, 0.3.18 and 0.5.8 in `Cargo.lock`), so the tree gains no crate.
- Tests: the unit tests pinning the 0.2412em pad and the font-wide reservation (`helpers.rs` `overflow_is_the_ink_outside_the_cap_height_line`, `layout_text_center_aligned_content_height_includes_reservation`; `mod.rs` `the_emitted_pad_is_the_aligned_edge_metric` and the #124/#245 raster tests) change their expected values or are replaced.
- `docs/AUTHORING.md`: the overflow-policy paragraph (line 375) and the §11 "Text is smaller than expected" and "Lowercase text sits lower" entries describe the font-wide reservation and are rewritten.
- Templates: every height-bound or `content`-height text item can render at a different size or position. The catalog tapes (`catalog/tape/brother/`) are centred, so their placement rule does not change, but a value that is height-bound on them can pick a different size. `tests/fixtures/templates/avery5163_asset_tag.yaml` is centred and height-bound and may pick a larger size.
- No API, schema, error code or `details.reason` changes.
