# 35. Font weight via the bundled variable font (deferred to Typst 0.15 / typst-as-lib 0.16)

Date: 2026-06-29

## Status

Accepted

## Context

Templates cannot set type weight: every text item renders at Inter's default ~400, so a label
cannot make a name or tag heavier than its surrounding text (#97). The reference `homebox_labels`
design relies on weight contrast, so no template-only change can match it.

The bundled font is the single-face variable `fonts/InterVariable.ttf` (family name `Inter Variable`,
`usWeightClass` 400, axes `wght 100-900 / opsz 14-32` [verified by inspecting the binary]). The
project compiles against Typst 0.14.2 via `typst-as-lib` 0.15.5 [verified Cargo.lock]. A font-weight
research run (2026-06-29, three web-research agents plus primary-source verification) established:

- **Typst 0.14 cannot vary a single-face variable font.** It renders the variable face at its default
  weight regardless of any requested weight, so weighted text comes out as ~400 (the original
  motivation behind #99 and typst/typst#751).
- **Typst groups faces by OpenType nameID 1 with style-suffix stripping, not nameID 16.** Verified
  from the exact source the project builds against (`typst-library-0.14.2/src/text/font/book.rs`,
  `typographic_family()`): the suffix list includes `bold/medium/light/thin/black/heavy/...` and the
  modifiers `semi/demi/extra/ultra`. Inter's legacy weight names (`Inter SemiBold`, `Inter Medium`,
  `Inter ExtraBold`, ...) therefore all collapse to the family key `inter`. This contradicts #99's
  claim that static instances would need nameID 16 surgery to avoid faux-bold; they would not.
  Typst has no faux-bold synthesis (typst/typst#394) and picks the nearest available `usWeightClass`.
- **Typst 0.15 (released 2026-06-15) adds native variable-font support.** It auto-drives the `wght`
  axis from `text.weight`, strips the `Variable`/`Var`/`VF` suffix so `Inter Variable` unifies with
  `Inter`, and *prefers* the variable face over static instances when weight distance ties
  (typst/typst#8425, verified in `font/book.rs` scoring). So on 0.15 the existing bundled variable
  font honors a requested weight with no new font assets.
- **The 0.15 upgrade is blocked at the wrapper, not the engine.** `typst-render`/`typst-pdf` 0.15.0
  exist, but `typst-as-lib` (latest 0.15.5 and `main`) still depends on `typst ^0.14`. The
  maintainer's upgrade PR is open with green CI: Relacibo/typst-as-lib#57 ("Update to typst/typst-kit
  0.15, bump version to 0.16.0"), non-draft, MERGEABLE as of 2026-06-23. The font-feature names this
  project uses (`typst-kit-embed-fonts`, `typst-kit-fonts`) are preserved as forwarding aliases in
  that PR.

The alternative the prior issue assumed was to bundle real static Inter weight faces (built from the
fork at `~/projects/inter` via `make static_ttf`, family `Inter`, distinct from `Inter Variable`).
This works on 0.14, but it is throwaway: once `typst-as-lib` 0.16 lands, the variable font is
preferred and the static faces become dead weight that must be removed to avoid silently shadowing.

## Decision

1. **Render font weight from the existing bundled variable font under Typst 0.15.** Do not bundle
   static Inter weight instances and do not instance weights at build/runtime via `fontTools`. The
   single `InterVariable.ttf` already covers `wght 100-900`; Typst 0.15 will honor a requested weight
   from it directly.

2. **Defer font-weight rendering until `typst-as-lib` 0.16 (typst 0.15) is released.** Tracked by the
   upgrade issue (#101), which is blocked on Relacibo/typst-as-lib#57. This is the only label-fidelity
   capability that waits on the upgrade.

3. **Font weight remains a real feature beyond the engine upgrade.** The upgrade only makes the engine
   *capable* of honoring a weight; nothing requests one today. After the upgrade, #97 still adds the
   `font_weight` field (`raw.rs` -> `convert.rs` -> `models.rs`) and emits `weight:` on both renderer
   paths, and #96 still fixes fontdue (`inter_font()`, separate from Typst, no variable-axis support)
   to measure at the requested weight so heavier text does not overflow the clip box.

4. **Everything else that affects label correctness proceeds now, independent of the upgrade.** The
   weight deferral is narrow. Container rotation for the rotated-portrait variant (#98), the
   `avery5163` rebuild geometry/sizing/QR/alignment (#95, every part except the final weight contrast),
   deterministic font search (#100), and any other rendering-fidelity fix are NOT gated on Typst 0.15
   and should be done as soon as ready. `avery5163` should be built correct in both orientations now,
   with weight contrast layered in once the upgrade lands.

## Consequences

- #99 (bundle static Inter weight faces) is closed as won't-do; its nameID-16 framing was incorrect
  and its approach is superseded here.
- No new font binaries enter the repo. `fonts/InterVariable.ttf` stays the single bundled face, also
  used by fontdue for measurement.
- A short-lived external dependency: weighted labels are unavailable until `typst-as-lib` 0.16 ships
  (the maintainer's PR is ready with green CI, so the wait is expected to be days-to-weeks, not
  months). If the wait runs long and weighted labels become urgent, the rejected static-faces path
  remains available as a temporary bridge, to be removed on upgrade.
- On upgrade, only `InterVariable.ttf` is kept in Typst's search directory, so the 0.15
  "variable preferred over static" behavior cannot shadow anything.
- Relationships: closes/supersedes #99; gated by #101 (Typst 0.15 upgrade); enables #97 (`font_weight`)
  and #96 (weight-aware measurement); independent of #98, #95, and #100, which proceed now.
