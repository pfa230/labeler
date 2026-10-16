## Why

Current pixel probes reproduce horizontal clipping from three causes: shaped text can advance farther than the character-width sum, glyphs such as `j` extend left of their advance box, and Typst's optical overhang pushes centered ellipses past the clip. Give horizontal fitting and intrinsic sizing the same measured-ink basis as vertical fitting so content-sized labels preserve the text they emit.

Source issue: https://github.com/pfa230/labeler/issues/257

Fixes #257

## What Changes

- **BREAKING** Measure horizontal text advances and ink from the shaped lines that are emitted, using the selected Inter instance.
- Reserve measured left/right overhang according to horizontal alignment and use that width consistently for fitting, overflow and intrinsic sizing; centered content can need more reservation than left/right alignment.
- Emit text without optical margin protrusion, retaining the existing clipping boundaries.
- Add current-renderer regressions for dynamic labels, capped content containers and centered overflowing text, plus narrow/wide ending-glyph controls.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `text`: Define horizontal shaped advances, ink reservation and alignment alongside the existing vertical rules, and make content-width alignment account for the reservation.

## Impact

Implementation touches the existing text/shaping helpers in `src/render/helpers.rs`, text emission in `src/render/mod.rs`, their existing tests and the affected text-sizing explanation in `docs/AUTHORING.md`. The current `layout` contract already consumes text intrinsic extents, so no layout schema or separate sizing framework is added. Label widths, selected sizes and ellipsis cutoffs can change where the previous width approximation or clipping affected output. No API field, template key, dependency or printer behavior changes.

## Non-Goals

No fixed padding constant, DPI-based margin, clipping removal, new font family, fallback-glyph policy, shaping backend, measurement cache or unrelated layout refactor. Existing vertical ink rules, line-breaking policy and supported-font boundary remain. The complete request in #257 is included; no follow-up issue is needed.
