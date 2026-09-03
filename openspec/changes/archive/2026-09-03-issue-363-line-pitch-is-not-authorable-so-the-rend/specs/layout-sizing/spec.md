## MODIFIED Requirements

### Requirement: Vertical fitting reserves the ink each alignment can expose

A text line's metric box runs cap-height to baseline, so accented capitals ink above it and
descenders ink below it, and every layout item is drawn into a clipped box of its resolved size. For
a text item of `n` lines at a candidate font size `s`, the renderer SHALL define, from the metrics of
the font instance it will render with:

- `u` = ascent overflow = `max(0, typographic_ascender − cap_height) / units_per_em`
- `d` = descent overflow = `max(0, −typographic_descender) / units_per_em`
- `pitch(s)` = `line_spacing × s`, where `line_spacing` is the item's authored value or 1.2 when absent (`text-line-spacing`)
- `leading(s)` = `pitch(s) − cap_height(s)`, the paragraph leading the renderer emits for the item; between lines only
- `metric_block(n, s)` = `cap_height(s) + (n − 1) × pitch(s)`, leading between lines only
- `reserve(vertical)` = `u + d` for `top` and `bottom`, and `2 × max(u, d)` for `center`

A one-line block is one cap-height box whatever the pitch, so `metric_block(1, s)` is pitch-independent and the `text-line-spacing` single-line no-op holds at the fitting level with no special case.

A block of `n` lines at size `s` SHALL be treated as fitting a box of height `H` when
`metric_block(n, s) + reserve(vertical) × s ≤ H`, and the same reservation SHALL bound the number of
lines kept, at
`max(1, floor((H − reserve(vertical) × s + leading(s)) / pitch(s)))`. The floor of
one is not a containment claim: a box that cannot hold one line plus its reservation is refused by
the one-line check before the budget is consulted, so the budget never has to describe a block of no
lines.

The fit comparison and the one-line check carry the renderer's existing tolerance of 0.01 pt; the
budget's own division carries none. Every containment guarantee in this requirement is therefore
bounded by that tolerance: ink at the very edge of the declared band may sit up to 0.01 pt outside
the box. At 180 dpi that is one fortieth of a pixel, enough to change an antialiased edge pixel's
coverage and not enough to cut a stroke.

`center` SHALL reserve twice the larger overflow rather than their sum, because the block is centred
on its metric box: the slack `(H − metric_block) / 2` left on each side must absorb the overflow on
that side alone. For a font whose two overflows are equal the two quantities are the same number.

The reservation SHALL be applied identically whether the size was chosen from a `font_size` range or
written as a fixed number. A fixed size cannot shrink, so on a fixed-size item the reservation is
visible in the line count and in the verdict of the item's `overflow` policy, never in the size.

Placement inside a resolved box is unchanged by the reservation: a `top`- or `bottom`-aligned block
SHALL be inset at its aligned edge by that edge's own overflow (`u` for `top`, `d` for `bottom`) so
the ink there lands inside the slot, and a `center`-aligned block SHALL be inset by nothing, because
centring the metric box already places the reserved slack on both sides.

The reservation is nevertheless part of the item's **intrinsic height**, for every alignment, because
that height is the room the block needs and clipped ink is not room it has. An item asking for a
`content` height therefore resolves a box taller by `reserve(vertical) × s` than its metric block,
and its content sits centred, top-inset or bottom-inset within that taller box as its alignment says.
This is what `top` and `bottom` already do; extending the reservation to `center` extends this with
it, and a `center`-aligned `content`-height item consequently occupies more of its frame than before
and places its ink higher above its anchor. An item whose height is authored, or comes from `fill` or
`to`, is unaffected: its box is decided without reference to the intrinsic.

The guarantee is bounded by the font's declared band. The renderer SHALL NOT measure per-string glyph
bounds to decide a size or a placement, because that would make position and spacing depend on which
glyphs the data happens to carry, and a batch would then render at inconsistent heights. A glyph that
inks outside the font's own ascender/descender band MAY therefore still be clipped, at any alignment
and at any size the fitter chooses.

This requirement supersedes, in the frozen `docs/SPEC.md` §3.1, the "**`top` and `bottom` inset the
block so its ink stays inside the slot**" bullet, the "**Two limits worth knowing**" bullet, and the
definition of the `overflow` term in the wrapped line-count formula
`floor((H − overflow + leading) / (cap_height + leading))`. The rest of §3.1 — what
`alignment.vertical` aligns, the fixed metric box and the `top` schema default — is unchanged and
remains authoritative. Its blank-edge lines bullet is **not**: the text-layout requirement above
supersedes it, so a blank first or last line is emitted rather than dropped, and this requirement's
reservation applies to the block that includes it. The Typst-default leading (`0.65em`) the previous
revision of this requirement inherited is retired: `leading(s)` above is derived from the authored
pitch, and the pitch itself comes from the `text-line-spacing` capability, which owns its default.

#### Scenario: A centred block auto-shrunk into a tight box keeps its descenders

- **WHEN** a 24 mm tape template whose `center`-aligned, `wrap: true` `text` item is 120 mm wide
  and fills the full 18.1 mm printable height, with `font_size: { min: 10, max: 32 }`, renders a
  value carrying a descender that breaks to exactly two lines at 24.0 pt and again at 21.0 pt, such
  as "Kitchen Utensils and a much longer second line here"
- **THEN** the chosen size is 21.0 pt, the largest 0.5 pt step whose
  `metric_block(2, s) + 2 × max(u, d) × s` fits 51.31 pt
- **AND** the two stated candidates decide it on height alone: line count does not increase as size
  falls, so a value that is two lines at both ends is two lines at every candidate between them, and
  every larger candidate fails on height at whatever count it breaks to — two lines at 24.5 pt
  already demand 59.04 pt of the box's 51.31 pt
- **AND** in the rendered PNG the descender of `g` is a closed stroke, where the same template and
  value before this requirement cut it mid-stroke on the box's final raster row

#### Scenario: A centred item with headroom is unaffected

- **WHEN** a `center`-aligned text item rendering a single line, a value with no line breaks that fits
  its box width at `font_size.max`, sits in a box whose height is authored, `fill` or `to` with
  `metric_block(1, max) + 2 × max(u, d) × max ≤ H`
- **THEN** the size the fitter chooses is decided by width alone, exactly as before
- **AND** the rendered output is byte-identical to what the same template and data produced before this
  requirement, because a one-line block carries no pitch term

#### Scenario: A centred multiline block's line budget counts the reserve

- **WHEN** a `center`-aligned `wrap: true` text item at a fixed `font_size` sits in a box that
  holds one line plus its reservation, so the one-line check passes, and wraps to more lines than
  `max(1, floor((H − 2 × max(u, d) × s + leading(s)) / pitch(s)))`
- **THEN** the lines beyond that budget are dropped and the last kept line is ellipsized, or the
  render fails, according to the item's `overflow` policy
- **AND** the kept block's ink, accents and descenders included, is inside the box to within the
  0.01 pt tolerance above

#### Scenario: A centred item asking for a content height grows by the reservation

- **WHEN** a `center`-aligned `text` with `size: [content, content]` is laid out at size `s`
- **THEN** its resolved box height is `metric_block(n, s) + 2 × max(u, d) × s`
- **AND** its metric block is centred in that box, so the block sits `max(u, d) × s` higher above the
  item's anchor than it did before this requirement
- **AND** its declared-band ink is contained rather than centred: the gaps above and below it are
  `max(u, d) × s − u × s` and `max(u, d) × s − d × s`, which are equal only when `u = d`
- **AND** a `top`-aligned item in the same position is unchanged, because its intrinsic already
  carried `u + d`

#### Scenario: An asymmetric font reserves twice its larger overflow

- **WHEN** a font supplied through `LABELER_FONTS_DIR` has a descent overflow larger than its ascent
  overflow, and a `center`-aligned item is height-bound in it
- **THEN** the reservation is twice the descent overflow, not the sum of the two
- **AND** the fitted block's descenders are inside the box

#### Scenario: Aligned edges are unchanged

- **WHEN** a single-line `top`- or `bottom`-aligned text item is fitted and rendered
- **THEN** it reserves `u + d` as before, and is inset at its aligned edge by that edge's overflow
- **AND** the rendered output is byte-identical to what the same template and data produced before this
  requirement, because a one-line block carries no pitch term

#### Scenario: A glyph outside the declared band still clips

- **WHEN** a string containing a glyph that inks above the font's ascender or below its descender is
  rendered into a box the fitter judged to fit
- **THEN** that glyph may be clipped, at any `alignment.vertical`
- **AND** the fitted size is the same as for a string of the same width without such a glyph

#### Scenario: The default pitch tightens existing multi-line items

- **WHEN** a two-line `wrap: true` item declaring no `line_spacing` renders with the bundled font at its fitted size `s`
- **THEN** its metric block is `cap_height(s) + 1.2 × s`, not the `2 × cap_height(s) + 0.65 × s` the previous revision reserved
- **AND** a height-bound item with a `font_size` range settles at a size no smaller than before, because the reservation never grows under that font
