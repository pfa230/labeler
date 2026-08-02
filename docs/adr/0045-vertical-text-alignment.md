# 45. Vertical text alignment

Date: 2026-08-02

## Status

Accepted. Consolidates and supersedes [ADR-0041](0041-vertical-alignment-delegated-to-typst.md),
[ADR-0043](0043-ink-based-vertical-alignment.md) and
[ADR-0044](0044-baseline-relative-vertical-alignment.md), which were written across two days while
one bug was fixed and the model was then changed twice. This is the only alignment ADR that needs
reading; the three it replaces are kept for history and marked superseded.

Also corrects how [ADR-0030](0030-multiline-auto-length-tape.md)'s "`alignment.vertical` is honored
literally" is implemented. Issues [#123](https://github.com/pfa230/labeler/issues/123),
[#127](https://github.com/pfa230/labeler/issues/127),
[#133](https://github.com/pfa230/labeler/issues/133); open trade on
[#124](https://github.com/pfa230/labeler/issues/124).

## Context

`alignment.vertical` places a text item inside its slot. Two questions had to be answered: **who
computes the placement**, and **what box gets placed**.

The original auto-length code answered both badly. It computed placement itself, sizing the text
block with `fontdue`'s full line height (~1.21 em) and offsetting it with a hand-computed `dy`. But
Typst lays a line out cap-height-to-baseline (~0.73 em) at the *top* of that block, so the glyphs
floated about 0.24 em high — measured as -1.41 mm on `brother_12mm` and -1.90 mm on two-line
`brother_24mm_multiline` (#123). The fixed-size path never had the bug: it boxed the slot and called
`#align`.

With that fixed, a second question surfaced (#127): the cap-height box is *string-independent*, so a
lowercase word with a descender sits low. On `brother_12mm` at 180 dpi in a 56 px slot, ink gap above
vs below: `MESSAGE` 11/12, `test` 14/12, `typogy` 14/3, `message` **20/3**.

Centring each string's own **ink** box was tried and shipped briefly. It made every label perfect in
isolation and broke the relationship between labels: the box height depends on the glyphs, so `j`
raised `testj` relative to `test`, and `t` lowered `test` relative to `es`. Labels off one roll no
longer shared a baseline.

Research into how other engines solve this — run late, which is the root of the churn — was
unanimous. Pango documents *logical* extents as what to position with, and Cairo notes ascent/descent
are the designer's alignment metrics rather than glyph extrema. Pillow's `anchor="mm"` is the
ascender/descender midpoint. TeX's `\strut` exists precisely to force constant height and depth so
baselines cannot wander, and Flutter's `StrutStyle` re-invents it. ImageMagick's centre gravity
computes from `metrics.ascent + metrics.descent`, not from the `metrics.bounds` it also exposes. Skia
positions runs on a baseline and treats blob bounds as culling data. ZPL, Brother b-PAC and DYMO are
baseline- or template-object based with no ink-centring API at all. CSS `text-box-trim` and Figma's
vertical trim trim to cap-height/baseline — still string-independent. Typst's own default is that box.

## Decision

**Typst computes the placement.** Both text paths emit a box the height of the item's slot and
delegate to `#align(<vertical> + <horizontal>)`. Neither derives a vertical offset from font metrics;
the `line_height_units` helper that did is deleted. Font metrics stay where they belong — auto-shrink,
wrapping and line-count budgeting.

**The placed box is Typst's default: cap-height to baseline.** We set no `top-edge`/`bottom-edge`
override at all, because the engine default already is the industry box and not overriding it states
that plainly. Alignment is therefore baseline-relative: the baseline lands in the same place whatever
glyphs a string contains.

Measured on `brother_12mm` at 180 dpi, ink rows in the 56 px slot:

| string | ink | baseline |
|---|---|---|
| `test` | 21..51 | 51 |
| `es` | 27..51 | 51 |
| `MESSAGE` | 18..51 | 51 |
| `testj` | 18..60 | 51 (descender to 60) |
| `message` | 27..60 | 51 (descender to 60) |

**Blank first and last lines are dropped before emission.** They carry no ink but still take a line
box, shifting visible text by a full line advance; `fit_text_auto_length` preserves them
(`"\nmessage"` measures as `["", "message"]`). Interior blanks are kept as real spacing.

### Alternatives measured and rejected

- **`ascender`/`descender` box** — places the baseline within 0.001 em of the default for Inter
  (0.969 + 0.240 vs 0.727), so it changes nothing. No string-independent box can fix a skew caused by
  the string's own glyphs.
- **`bounds`/`bounds` (per-string ink)** — centres every label perfectly and moves the baseline
  between labels. Rejected on that; see #133.
- **Hand-computed correction via `fontdue` + `#move`** — ~90 lines, a sign error on the first pass,
  and 1-2 px residual where Typst's own edges land within 0.5 px. Rejected as reinventing the engine.
- **`cap-height`/`descender`** — keeps the stable baseline *and* stops descenders overhanging, but
  pushes caps ~0.12 em high. Still open as the candidate fix for #124.

## Consequences

- Descenders hang below the box, so `vertical: bottom` clips `g j p q y` at the slot edge (#124).
  Templates must reserve room until that is decided.
- Lowercase-only text sits lower in its slot than all-caps, because the box reserves cap-height space
  whether the string uses it or not. Inherent to baseline alignment; every other renderer does this.
- Regression coverage is pixel-level, not source-level: `baseline_is_stable_across_glyph_classes`
  reads the baseline directly (strings without descenders end their ink on it) and asserts every
  glyph class agrees within 1 px. It is verified to fail if per-string ink centring returns.
  `autolength_text_centers_vertically` and `autolength_text_top_and_bottom_pin_to_slot_edges` guard
  the #123 arithmetic bug at two font sizes, since that error scaled with the em.

## Note on process

Three ADRs in two days for one setting. The first fixed a genuine bug; the next two changed the model
on intuition, shipped, and were reverted after the research that would have settled it was finally
run. For rendering decisions: establish what established engines do **before** choosing a model.
