## MODIFIED Requirements

### Requirement: Text is laid out against the box it will get, and what does not fit is authored

Every **active** `text` item SHALL be laid out by the steps below and SHALL have its `overflow`
policy enforced, whether or not either of its axes asks for an intrinsic size. Laying out and
demanding an intrinsic size are different things: a text in a fully authored box (`size: [40, 10]`,
fixed `font_size`) reports no intrinsic size on either axis and is still broken, fitted and checked
against its policy. Its layout is the rendered output, not a measurement.

The box a text is laid out against is known before its content is: it is the item's own extent when
that extent is authored, and the available extent, capped, when it is content or frame. On a
dynamic-width `single` the frame used for that is `format.width.max`. Nothing about line breaking
consults the page format.

1. **Break.** `multiline: true` wraps the value to the box width, breaking at spaces and splitting a
   single word wider than the box character by character, which is the current algorithm and is
   retained unchanged. `multiline: false` keeps only the first input line. A line therefore remains
   wider than the box only when one **glyph** is, which no breaking rule can help.
2. **Shrink.** A `font_size` range picks the largest size in `[min, max]` at which the broken block
   fits the box height, in 0.5 pt steps, including the ink reservation the *Vertical fitting reserves
   the ink each alignment can expose* requirement defines for the item's `alignment.vertical`. The text SHALL be re-broken at each candidate's glyph advances, as today's
   `largest_fitting_font` does; the emitted breaks are the ones from the selected size, not breaks
   frozen at `font_size.max`. A fixed `font_size` skips this step.
3. **Overflow.** What still does not fit is resolved by the item's `overflow` policy.
4. **Trim blank edges.** A blank first or last line carries no ink but occupies a line box, which
   would shove the visible text off centre, so it is dropped at emission (#127). Interior blank lines
   are real spacing and are kept.

The order of steps 2 and 4 is normative and matches today's: blank edge lines are counted while the
font size is chosen and removed only when the lines are emitted. Dropping them earlier would let a
value with a leading newline select a larger font than it does today.

The item's **intrinsic height** SHALL be the block height of the lines actually emitted, after step 4,
at the size step 2 chose. The two counts differ exactly when a value has a blank first or last line:
the font is chosen against the untrimmed block, so it never overflows, and the size reported upward
is the block that is drawn, so a hugging parent hugs what it can see. Reporting the untrimmed height
would make a `content` container reserve room for a line that carries no ink.

Neither the breaks nor the size SHALL be re-decided when the item's box turns out to be larger than
the box it was laid out against. A `fill` text on a label that clamps up to `width.min` keeps the
lines and size it was laid out with, and the extra width becomes slack for `alignment.horizontal`.

An item's box SHALL be its box regardless of `alignment.horizontal`, which positions content inside
it. This supersedes ADR-0059, under which a centred auto-width text on a dynamic-width label was
given the alignment slot as its box while a left-aligned one was given the laid-out width.

**`overflow`.** A `text` SHALL carry an `overflow` field with the values `ellipsis` (the default) and
`fail`. Both shorten nothing the other would not; they differ in when they give up:

| | `ellipsis` | `fail` |
| --- | --- | --- |
| fits as authored | render it | render it |
| fits once shortened | render the shortened form | `text_does_not_fit` |
| cannot fit however short | `text_does_not_fit` | `text_does_not_fit` |

Shortening keeps the lines that fit and appends `...` to the last, trimming characters until it fits.
The shortest form it can produce is the marker alone, so shortening succeeds whenever `...` fits the
box width and the box holds at least one line, and fails otherwise. Two cases therefore reach the
third row, and neither is a separate rule:

- the box is narrower than `...` itself, so there is nothing shorter to produce;
- the box is shorter than one line at the chosen size, since a line's height comes from the font size
  and the line count is already at its floor of one.

An over-wide **glyph** is not by itself one of them. A box can be too narrow for a glyph and still
wide enough for the marker, and under `ellipsis` that case renders `...`; it fails only when the
marker does not fit either. Under `fail` it fails as soon as the content overflows, marker or no
marker.

Clipping SHALL NOT be an outcome of the policy: a box that cannot hold the shortest representable
form of its content is an error, not a label with half a glyph on it.

The policy SHALL be evaluated against the **metric model** ADR-0045, ADR-0050 and ADR-0084 define:
the cap-height-to-baseline line box plus the ink reservation for the item's `alignment.vertical`,
including `center`, and not against glyph outlines. Widening that model widens what the policy
refuses, and both effects are intended: a `center`-aligned item whose block fits its box but whose
block plus reservation does not SHALL be shortened under `ellipsis` and SHALL raise
`text_does_not_fit` under `fail`, and a box that cannot hold one line plus its reservation at the
chosen size SHALL raise `text_does_not_fit` under either.

One ADR-0050 consequence stands and is not superseded: a glyph inking outside the font's own
ascender/descender band can still clip at any alignment. That is ink leaving a box the metric model
says it fits in, which no policy evaluated on metrics can see. Centred text clipping in a slot
shorter than `1.21 × font_size` is no longer one of them: such a slot is now an overflow, and the
policy resolves it.

This requirement supersedes the frozen `docs/SPEC.md` §3.1 sentence "If the content still overflows
at `font_size.min`, the fitting lines are kept and the last is ellipsized" and its multiline wrap
paragraph, and the §4.1 clause "A range auto-shrinks the text to fit the box (0.5pt steps) and
truncates with an ellipsis if it still overflows", generalising both to every format and every
`font_size` spelling.

#### Scenario: A fully authored text is still laid out and still enforces its policy

- **WHEN** a `text` declares `size: [40, 10]` with a fixed `font_size` and `overflow: fail`, so
  neither axis asks for an intrinsic size, and its value does not fit
- **THEN** the render fails with reason `text_does_not_fit`
- **AND** with `overflow: ellipsis` it is broken and ellipsized to the 40 by 10 box, rather than
  emitted unfitted for the renderer to clip, which is what happens today

#### Scenario: A long word is split, not overflowed

- **WHEN** a `multiline: true` `text` carries a single word far wider than its box, and the box is
  tall enough for the resulting lines
- **THEN** the word is split character by character across lines that each fit
- **AND** no overflow occurs and neither policy is consulted

#### Scenario: An over-wide glyph is shortened when the marker still fits

- **WHEN** a `multiline: true` `text` with `overflow: ellipsis` and a fixed `font_size` carries a
  glyph wider than its box, in a box still wider than `...` at that size
- **THEN** it renders as `...`, because a shortened form exists
- **AND** the same item with `overflow: fail` fails with reason `text_does_not_fit`

#### Scenario: A box narrower than the marker cannot be shortened

- **WHEN** the same `ellipsis` item sits in a box narrower than `...` at its chosen size
- **THEN** the render fails with reason `text_does_not_fit`, because no shortened form exists
- **AND** no clipped marker is emitted

#### Scenario: A box too short for one line cannot be shortened

- **WHEN** a `text` with a fixed `font_size` of 20 sits in a box 40 wide and 2 units tall
- **THEN** the render fails with reason `text_does_not_fit` under either policy

#### Scenario: A hugging text never shortens on its own account

- **WHEN** a `content`-width `text` with no `max_w` renders in a frame wide enough for its available
  extent
- **THEN** it is laid out at its natural width with no truncation, because its box is its content
- **AND** shortening applies only when a cap or the available extent binds

#### Scenario: An empty value in a zero-width box is not an overflow

- **WHEN** a `content`-width `text` bound to an empty value resolves to a zero-wide box
- **THEN** it renders empty and no error is raised, because there is no content to shorten

#### Scenario: Shrinking happens before the policy

- **WHEN** a `text` with `font_size: { min: 8, max: 20 }` and `overflow: fail` carries a value that
  does not fit at 20 but fits at 12
- **THEN** it renders at 12 and no error is raised

#### Scenario: A leading blank line shrinks the chosen font

- **WHEN** a `multiline: true` `text` with `font_size: { min: 8, max: 20 }` receives a value of one
  blank line followed by **two** non-blank lines that need no wrapping, in a box tall enough for
  exactly two lines at 20 pt and for three at 14 pt
- **THEN** the block is three line boxes while the size is chosen, so 20 pt does not fit and 14 pt is
  selected
- **AND** it is dropped at emission, so the label shows the visible text at that smaller size
- **AND** an implementation trimming blank edges before choosing the size would select 20 pt

#### Scenario: A hugging parent hugs the emitted lines, not the trimmed ones

- **WHEN** a `container` with `size: [20, content]` holds a `multiline: true` `text` with a fixed
  `font_size` whose value is a blank line followed by two non-blank lines
- **THEN** the container's intrinsic height is the block height of **two** lines, not three
- **AND** the font size, had it been a range, would still have been chosen against three

#### Scenario: Alignment does not change the box

- **WHEN** the same `content`-width `text` renders once with `alignment.horizontal: left` and once
  with `center`
- **THEN** both are drawn into a box of the laid-out text width, and neither gets the frame remainder

#### Scenario: Centring is authored

- **WHEN** a `text` declares `size: [fill, 16.1]` with `alignment.horizontal: center` on a
  dynamic-width label clamped to `width.min`
- **THEN** its box spans the frame remaining from its anchor, and the text is centred within it

#### Scenario: The policy is independent of the format

- **WHEN** the same item and value are placed on a fixed-width `single`, a `sheet` slot, and an
  auto-length `single` clamped to `width.max`, each with the same resolved box
- **THEN** all three produce the same lines and the same overflow outcome

#### Scenario: A centred item that overflows only by its ink is an overflow

- **WHEN** a `center`-aligned `text` with `overflow: fail` and a fixed `font_size` carries a value
  whose metric block fits its box height but whose block plus reservation does not
- **THEN** the render fails with reason `text_does_not_fit`
- **AND** the same item with `overflow: ellipsis` drops or shortens lines until the block plus its
  reservation fits, or fails if no such form exists

## ADDED Requirements

### Requirement: Vertical fitting reserves the ink each alignment can expose

A text line's metric box runs cap-height to baseline, so accented capitals ink above it and
descenders ink below it, and every layout item is drawn into a clipped box of its resolved size. For
a text item of `n` lines at a candidate font size `s`, the renderer SHALL define, from the metrics of
the font instance it will render with:

- `u` = ascent overflow = `max(0, typographic_ascender − cap_height) / units_per_em`
- `d` = descent overflow = `max(0, −typographic_descender) / units_per_em`
- `metric_block(n, s)` = `n × cap_height(s) + (n − 1) × leading(s)`, leading between lines only
- `reserve(vertical)` = `u + d` for `top` and `bottom`, and `2 × max(u, d)` for `center`

A block of `n` lines at size `s` SHALL be treated as fitting a box of height `H` when
`metric_block(n, s) + reserve(vertical) × s ≤ H`, and the same reservation SHALL bound the number of
lines kept, at
`max(1, floor((H − reserve(vertical) × s + leading(s)) / (cap_height(s) + leading(s))))`. The floor of
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
definition of the `overflow` term in the multiline line-count formula
`floor((H − overflow + leading) / (cap_height + leading))`. The rest of §3.1 — what
`alignment.vertical` aligns, the fixed metric box, the `top` schema default, and blank-edge lines —
is unchanged and remains authoritative.

#### Scenario: A centred block auto-shrunk into a tight box keeps its descenders

- **WHEN** a 24 mm tape template whose `center`-aligned, `multiline: true` `text` item is 120 mm wide
  and fills the full 18.1 mm printable height, with `font_size: { min: 10, max: 32 }`, renders a
  value carrying a descender that breaks to exactly two lines at 24.0 pt and again at 19.5 pt, such
  as "Kitchen Utensils and a much longer second line here"
- **THEN** the chosen size is 19.5 pt, the largest 0.5 pt step whose
  `metric_block(2, s) + 2 × max(u, d) × s` fits 51.31 pt, rather than the 24.0 pt the metric block
  alone allowed before this requirement
- **AND** the two stated candidates decide it on height alone: line count does not increase as size
  falls, so a value that is two lines at both ends is two lines at every candidate between them, and
  every larger candidate fails on height at whatever count it breaks to — two lines at 24.5 pt
  already demand 51.57 pt of the box's 51.31 pt before the reservation is added at all
- **AND** in the rendered PNG the descender of `g` is a closed stroke, where the same template and
  value before this requirement cut it mid-stroke on the box's final raster row

#### Scenario: A centred item with headroom is unaffected

- **WHEN** a `center`-aligned text item whose height is authored, `fill` or `to` has a `font_size.max`
  that still satisfies `metric_block(n, max) + 2 × max(u, d) × max ≤ H`
- **THEN** the size the fitter chooses is decided by width alone, exactly as before
- **AND** the emitted output is byte-identical to what the same template and data produced before this
  requirement

#### Scenario: A centred multiline block's line budget counts the reserve

- **WHEN** a `center`-aligned `multiline: true` text item at a fixed `font_size` sits in a box that
  holds one line plus its reservation, so the one-line check passes, and wraps to more lines than
  `max(1, floor((H − 2 × max(u, d) × s + leading(s)) / (cap_height(s) + leading(s))))`
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

- **WHEN** a `top`- or `bottom`-aligned text item is fitted and rendered
- **THEN** it reserves `u + d` as before, and is inset at its aligned edge by that edge's overflow
- **AND** the emitted output is byte-identical to what the same template and data produced before this
  requirement

#### Scenario: A glyph outside the declared band still clips

- **WHEN** a string containing a glyph that inks above the font's ascender or below its descender is
  rendered into a box the fitter judged to fit
- **THEN** that glyph may be clipped, at any `alignment.vertical`
- **AND** the fitted size is the same as for a string of the same width without such a glyph
