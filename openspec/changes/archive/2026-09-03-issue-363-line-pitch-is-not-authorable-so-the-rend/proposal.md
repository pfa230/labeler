# Proposal: Authorable line pitch (`line_spacing`) on text items

## Why

Implements [#363](https://github.com/pfa230/labeler/issues/363). A template cannot set the line pitch of a multi-line text item, so on a height-bound item the renderer's inherited Typst default decides the fitted font size, and that default is a property of whichever font file is loaded rather than a number in the spec.

## What Changes

- A new optional field `line_spacing` on `text` layout items only: a bare number giving the baseline-to-baseline distance as a multiple of the font size. Anything that is not a bare number, including an explicit null, is refused before serving with an error naming the file and the key.
- The default is 1.2, and it is the only default. The metric-derived pitch (`cap_height + 0.65em`, i.e. 1.3775em on the bundled Inter) is deleted, not kept as a fallback.
- **BREAKING.** Every existing multi-line text item tightens from 1.3775em to 1.2em pitch with the bundled font, and every height-bound item picks up a larger fitted font size from the gained slack. Templates change appearance without being edited. No migration, no desugaring, no deprecation window, per the pre-1.0 rule.
- The fitter reserves and the emitter emits against the same pitch: the renderer sets each text block's Typst `par` leading to whatever produces the authored pitch. The derived leading inherits the box model the existing Typst-layout agreement proof covers; the issue's 1.326em figure measured ink tops, which move with each line's glyphs rather than with the baselines, so pitch acceptance is measured on content-controlled values (identical repeated lines) and not on arbitrary text.
- The field is legal on a single-line item and has no effect there. Static only: no `{{ param }}` interpolation (a follow-up issue).
- `docs/AUTHORING.md` shows the field on a worked example.
- One first-touch `ADDED` requirement under a new `text-line-spacing` capability carries the complete post-change `text` field list and supersedes the frozen `docs/SPEC.md` §4.1 `text` bullet (`docs/SPEC.md:488-500`) in full.
- One `MODIFIED` requirement under the existing `layout-sizing` capability updates "Vertical fitting reserves the ink each alignment can expose" to compute the metric block and line budget from the authored pitch instead of the Typst-default leading, narrows the two byte-identity scenarios to the single-line cases the pitch cannot move, and re-derives the auto-shrink scenario's sizes for the 1.2 default.

## Capabilities

### New Capabilities

- `text-line-spacing`: the complete post-change `text` field list, the `line_spacing` schema/validation/quarantine/read-back, what the number means, the 1.2 default as the only default, the single-line no-op, text-only scope, and the breaking-change statement. First migration of line-pitch behaviour out of the frozen spec, so an `ADDED` requirement carrying the complete post-change contract.

### Modified Capabilities

- `layout-sizing`: the "Vertical fitting reserves the ink each alignment can expose" requirement is updated in full: `metric_block(n, s)` becomes `cap_height(s) + (n − 1) × pitch(s)` with `pitch(s) = line_spacing × s`, the Typst leading becomes the derived `pitch(s) − cap_height(s)` the renderer emits, and the line-budget divisor becomes the pitch. Reservation, tolerance, placement and intrinsic-height rules are unchanged.

## Impact

- **Code**: `src/raw.rs` (`TextRaw` gains the field with presence-tracking so an explicit null is refused rather than read as absent; `deny_unknown_fields` keeps every other item closed), `src/models.rs` (`LayoutItem::Text` gains it, with read-back omitting an absent key), `src/convert.rs` (value validation with path, mirroring `font_weight`), `src/templates.rs` (`validate`, mirroring `font_weight`), `src/render/helpers.rs` (`leading()` and the `cap_height + 0.65em` stacking deleted; pitch-parameterised block height, budget and `TextLayoutItem`), `src/render/mod.rs` (emit a block-scoped `par(leading:)` per text block so the setting cannot leak across items sharing one Typst source; thread the field through `TextRenderArgs`). Many existing render-measurement tests keyed to the old pitch will move and must be re-baselined by the implementer.
- **Specs**: one new delta under `text-line-spacing`, one modified delta under `layout-sizing`. `docs/SPEC.md` and `docs/adr/` are not edited (frozen).
- **Clients**: breaking for any multi-line template served before. The template API is otherwise additive: template-detail responses gain an optional `line_spacing` member on text items (omitted when absent), which the OpenAPI schema picks up through the existing derive; no endpoint, status or error shape changes.
- **Docs**: `docs/AUTHORING.md` gains the field on a worked example (part of this change, acceptance criterion).
