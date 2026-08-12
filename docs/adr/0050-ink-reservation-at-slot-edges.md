# 50. Reserve ink room at slot edges instead of changing the line box

Date: 2026-08-12

## Status

Accepted. Issue [#124](https://github.com/pfa230/labeler/issues/124). Reaffirms
[ADR-0045](0045-vertical-text-alignment.md)'s fixed-metric principle and builds on
[ADR-0049](0049-weight-aware-text-measurement.md)'s measurement backend. Bends
[ADR-0041](0041-vertical-alignment-delegated-to-typst.md) deliberately; see Consequences. None of
them are edited.

## Context

Typst's text line box runs `top-edge: "cap-height"` to `bottom-edge: "baseline"`, and every layout
item is emitted inside a `clip: true` box, so ink outside that band is cut. The issue reported
bottom-aligned descenders. **The defect is symmetric and the issue recorded half of it**: measured
against the bundled `InterVariable.ttf` (cap 0.7275em, ascender 0.9688em, descender −0.2412em),

| glyph | escapes the line box by |
| --- | --- |
| `É` | 0.2148em **above** cap height |
| `g` / `y` | 0.2158em / 0.2080em below the baseline |
| `Q` | 0.0098em above, 0.0684em below |

Confirmed by rendering, not arithmetic: top-aligned `Édgy` at 20pt rendered as `Edgy`, the acute
sliced off; bottom-aligned `gjpqy` lost every tail.

## Decision

**Keep the line box; inset the block.** A `top`- or `bottom`-aligned block is wrapped in a `#pad` at
its aligned edge — `ascender − cap_height` for top, `|descender|` for bottom, both 494 font units in
Inter — so alignment places the padded block and the content ends up inset by exactly that. Typst's
`#pad` grows the frame and translates the child inward, so this is a placement change, nothing more.

**The fitter reserves both overflows, not one.** Padding the aligned edge alone leaves a hole: a
top-aligned block that just fits (`H = cap + top_pad`) puts its baseline on the slot floor, and `g`
still reaches `1.1846s`. So the auto-shrink fitter holds back
`(ascender − cap_height) + |descender|` = 0.4825em for any `top`/`bottom` item. These are two
different constants doing two different jobs: the pad is *placement* and appears in the generated
source; the reservation is *fit* and never does.

**`center` is left alone.** It already splits the slack; reserving both sides would cost a full em on
top of a 0.7275em line — arriving back at the em box and shrinking exactly the bundled tape templates
this decision protects. Centered text can still clip in a slot shorter than `1.21 × font_size`, which
SPEC §3.1 records.

**Metrics come from where Typst reads them**: `typographic_ascender()`/`typographic_descender()` with
the hhea fallback, and `cap_height`'s own fallback was corrected to match. This only matters for a
font supplied through `LABELER_FONTS_DIR`, which is precisely the case bundled-font tests cannot see.

## Rejected alternatives

**Changing the edges to `ascender`/`descender`.** The obvious fix, designed and reviewed before being
rejected on measured evidence. Edges apply *per line*, so baseline-to-baseline would stretch from
`cap + leading` = 1.3775em to 1.86em — every multi-line label 35% airier. Worse, it clips more than
it fixes: `avery5163_asset_tag` holds fixed-12pt multiline text in a 0.41in box where two lines fit
today (25.26pt of 29.52pt) and would need 36.84pt under a per-line em box; fixed `font_size` has no
shrink path, so the second line would simply be cut. And it would not even deliver containment — 211
of Inter's 2989 glyphs ink above its ascender, 197 below its descender.

**`bounds` edges.** Contain every glyph exactly and waste nothing, but make position and spacing
depend on the data: the same template renders at different heights depending on which glyphs arrive.
That is jitter across a batch, and it is what ADR-0045 rejected.

## Consequences

- `top`/`bottom`-aligned text moves by ~0.24em, and height-bound items of that kind fit at a slightly
  smaller size. Centered text does not move at all: the bundled tape templates render byte-for-byte
  identically before and after, verified by MD5.
- The guarantee is scoped, and SPEC says so: ink within the font's ascender/descender band is
  contained; the outliers above are not, and `center` is not inset.
- ADR-0041 delegated placement to `#align` rather than metric-derived offsets. This inset is such an
  offset, deliberately: #123's failure was *guessing* an offset from a box Typst was not using,
  whereas this one is computed from the metrics Typst itself reads and is checked against a compiled
  page.
- The ink-conservation test compares an aligned render in a tight slot against a **centered** control,
  not against a taller slot at the same alignment — bottom alignment puts the baseline on the slot
  floor however tall the slot is, so an aligned control clips exactly as much as the subject and the
  comparison is blind. That mistake was made and caught during implementation; the note is here so it
  is not made again.
