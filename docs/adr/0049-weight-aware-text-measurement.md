# 49. Text measurement tracks the font instance Typst renders

Date: 2026-08-12

## Status

Accepted. Issues [#96](https://github.com/pfa230/labeler/issues/96) and
[#97](https://github.com/pfa230/labeler/issues/97). Builds on
[ADR-0035](0035-font-weight-via-variable-font.md), which is referenced, not edited.

## Context

Auto-shrink and ellipsis fitting measured text with `fontdue`, reading the bundled
`InterVariable.ttf` at whatever instance the file defaults to. Typst renders something else. It sets
the `wght` axis from the text's weight and the `opsz` axis from the font size in points
(`typst-library/src/text/font/variations.rs`), and it stacks lines as cap-height-to-baseline boxes
separated by `leading` (0.65em by default), not as a run of full font line boxes.

Three measurement errors followed, all of them "measuring something Typst does not lay out". Measured
against the bundled font:

| error | direction | magnitude |
| --- | --- | --- |
| `wght` ignored | under-measures bold | +2.5-4% wider at 700 for realistic strings, +12% for narrow-glyph runs, +22% at 900 |
| `opsz` ignored | over-measures large text | −3.7% at 24pt, −6.7% at 32pt |
| line height flat at 1.21em | over-measures short blocks, under-measures long ones | +66% at 1 line, +15% at 2, +4% at 3, −0.4% at 4, approaching −12% |

Under-measurement is the direction that overflows a clip box; over-measurement prints text smaller
than its box allows. Both were live. `fontdue` could not be made to do better: version 0.9.4 has no
variation API of any kind, so it can only ever report the default instance.

Adding `font_weight` (#97) would have made the first error visible and routine, which is why the two
issues ship together.

## Decision

**`fontdue` is replaced by `ttf-parser`** (0.25, `variable-fonts` feature), already present in the
dependency tree via Typst. It was used for nothing but advance widths and line height, so this is a
swap rather than an addition.

**Both axes are set per measurement**: `wght` from the item's `font_weight` (400 when unset) and
`opsz` from the candidate font size. Out-of-range values normalise against the axis, clamping as
Typst's do. The face is parsed once per fit and mutated per candidate size, since the size loop runs
up to ~76 iterations.

**Line stacking is modelled as `n * cap_height + (n - 1) * leading`**, and the fitter carries the two
constants separately rather than fusing them into a per-line height. `typst-layout`'s collector emits
leading only between lines, so a fused constant overshoots by one leading per block.

**The model is calibrated against the real engine, not derived on paper.** Tests compile probe pages
with auto width and auto height and compare Typst's laid-out frame against the fitter's prediction:
block heights at one, two and three lines, and line widths at two font sizes. All four match exactly
(0.00% drift), and the tests assert a 1% bound so a future font revision cannot silently drift.

**The axes are verified when the font loads.** `set_variation` returns success for any variable face
even when no axis matches the tag, so it cannot serve as the check; the loader confirms `wght` and
`opsz` are present and fails with a clear error naming the font otherwise. The loader is split into a
byte-taking `load_face` and a cached `inter_face` so this is testable without depending on which font
some earlier test happened to load first.

**A character the font lacks measures as `.notdef`**, matching what `fontdue` did, and never errors:
Typst renders such characters from a fallback face, so the label is valid and only its measurement is
approximate.

## Consequences

- **Templates with a `font_size` range may render at a different size than before.** Mostly larger,
  since the dominant pre-existing error over-measured. Fixed `font_size` with an explicit width is
  untouched; fixed `font_size` with `size: auto` still measures, so it can move too. This is recorded
  in the SPEC changelog because someone comparing a reprint against an older label deserves to find
  it written down rather than infer it.
- **The fitter matches Typst's instance but not its shaping.** Measurement remains a per-character
  advance sum, so kerning, ligatures and fallback glyphs can still make Typst's width differ. The
  calibration shows the residual is nil for unkerned strings; closing the gap entirely would mean
  shaping with rustybuzz, which is a separate and larger decision.
- The `weight` reaches the fitter at all three call sites — fixed-size, auto-length, and the measure
  pre-pass that decides an auto width. Missing the pre-pass would size a tape label for text that
  renders wider, so a test pins the behavior rather than the wiring.
- `"Inter Variable"` is dropped from the requested font family list. Typst 0.15 strips the suffix from
  *stored* family names only, so requesting it logged `unknown font family` on every compile and
  resolved through the `Inter` fallback.
- One dependency out, one in, and the new one was already being compiled. `typst-assets` is added as a
  dev-dependency so the axis-verification test can use a real static face rather than corrupt bytes.
