# 44. Vertical alignment is baseline-relative, using a fixed metric box

Date: 2026-08-02

## Status

**Superseded by [ADR-0045](0045-vertical-text-alignment.md)**, which merges this decision with
ADR-0041 and ADR-0043 into a single document. The decision itself is unchanged — alignment is
baseline-relative, using Typst's default cap-height→baseline box. Kept for history; read 0045
instead.

Originally: Accepted, superseding ADR-0043. Issue
[#133](https://github.com/pfa230/labeler/issues/133); reopens
[#124](https://github.com/pfa230/labeler/issues/124).

## Context

ADR-0043 made `alignment.vertical` centre the bounding box of the glyphs actually drawn. Each label
then centred perfectly on its own marks. In use, that turned out to be the wrong property to
optimise: the box height depends on the string, so the baseline moved between labels.

- `test` vs `testj` — the `j` descender makes the box taller at the bottom, pushing the baseline up.
- `test` vs `es` — the `t` ascender makes the box taller at the top, pushing the baseline down.

Two labels from the same template no longer agreed on a baseline. ADR-0043 recorded this as an
accepted trade; seeing it printed, it is not acceptable.

The research behind ADR-0043 had already said so, and was overruled at the time. Every server-side
and print engine surveyed centres a **string-independent** box and keeps the baseline fixed: Pango
documents logical (not ink) extents as what to position with, and Cairo notes ascent/descent are the
designer's alignment metrics rather than glyph extrema; Pillow's `anchor="mm"` is the ascender/
descender midpoint; TeX's `\strut` exists precisely to force constant height and depth so baselines
cannot wander, and Flutter's `StrutStyle` re-invents it; ImageMagick's centre gravity computes from
`metrics.ascent + metrics.descent`, not from the available `metrics.bounds`; Skia positions runs on a
baseline and treats blob bounds as culling data; ZPL, Brother b-PAC and DYMO are baseline- or
template-object based with no ink-centring API at all. CSS `text-box-trim` and Figma's vertical trim
trim to cap-height/baseline — still string-independent. Typst's own default is that box.

## Decision

**Use Typst's default box: cap-height to baseline.** Concretely, drop the `top-edge`/`bottom-edge`
override entirely rather than setting it to anything — the engine default already is the standard,
and not overriding it is the clearest statement of intent.

Measured on `brother_12mm` at 180 dpi, ink rows in the 56 px slot:

| string | ink | baseline |
|---|---|---|
| `test` | 21..51 | 51 |
| `es` | 27..51 | 51 |
| `MESSAGE` | 18..51 | 51 |
| `testj` | 18..60 | 51 (descender to 60) |
| `message` | 27..60 | 51 (descender to 60) |

The baseline is identical for every string; descenders hang below it instead of moving it.

The alternative fixed boxes were measured too. `ascender`/`descender` places the baseline within
0.001 em of the default for Inter, so it buys nothing. `cap-height`/`descender` also keeps a stable
baseline and additionally stops descenders overhanging, but pushes caps about 0.12 em high — that
trade belongs to #124, not here.

## Consequences

- **#124 reopens.** With the baseline at the box bottom, `vertical: bottom` puts descenders outside
  the clipped box and they are cut. Templates must reserve room, or we adopt `bottom-edge:
  "descender"` — decided on #124 rather than bundled here.
- Lowercase-only text sits lower in its slot than all-caps, because the box reserves cap-height space
  above the baseline whether the string uses it or not. Inherent to baseline alignment; it is what
  every other renderer produces, and it is the appearance that prompted #127.
- The blank-edge-line trim from ADR-0043 stays. It is a separate correctness fix: a leading blank
  line still takes a real line box and would shift the visible text by a full line advance.
- #123 stands unchanged. Text floating ~0.24 em high was an arithmetic bug in our own placement code,
  independent of which box is centred.
- Regression coverage is `render::tests::baseline_is_stable_across_glyph_classes`, which reads the
  baseline directly (strings without descenders end their ink on it) and asserts every glyph class
  agrees within 1 px. Verified to fail if per-string ink centring is reintroduced.

## Note on process

This is the third alignment ADR in two days (0041 → 0043 → 0044). The churn came from fixing a real
bug (#123) and then continuing to tune past it on intuition. The research that would have settled the
question was available before ADR-0043 and was run only afterwards, at which point it contradicted
the design already shipped. For future rendering decisions: establish what established engines do
*before* choosing a model, not after.
