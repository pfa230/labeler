## Context

See proposal.md for motivation. The issue's older `text_width` pointer is now a test-only function; production measurement uses `shape_run` and retains drawable zero-glyph clusters as Inter's `.notdef`. `draws_nothing` already identifies LF, tabs and default ignorables. `a_character_the_font_does_not_map_is_outside_the_guarantee` explicitly expects `H中` to render and checks `.notdef`-based geometry. Neither that test nor the actual fallback discrepancy has been run in this scoping session.

Typst documents that fallback may select another available family when the chosen family lacks a glyph: https://typst.app/docs/reference/text/text/#parameters-fallback. Do not treat that documentation as proof that a particular installed fallback supplies the selected reproduction character.

## Goals / Non-Goals

Refuse unmapped drawable input at the common text-layout boundary. Do not rewrite the shaper, disable fallback globally, add fonts or alter QR/image handling.

## Decisions

- **Routine default: validate once before fitting.** In `layout_text`, after newline normalization/segmentation and before font-size search or shortening, inspect character mappings using the loaded Inter face. Reuse the existing no-drawing classification for line breaks, tabs and default ignorables. Font variation changes metrics, not the character map. Checking only emitted lines would let ellipsis hide an unsupported input character; checking only the final chosen face after size search would still measure unsupported input during the search.
- **Routine default: one layout error.** Add `Reason::TextCharacterUnmapped` and an `AppError` constructor using `reasoned`, carrying the affected text-value path as `details.field` and the unmapped scalar as `details.character`. A human-readable message can identify its Unicode code point. Do not add a list of every offending character or infer parameter provenance from an interpolated string; the layout path identifies the affected field even when its value mixes literals and parameters.
- Replace the existing fallback-success render assertion with the requested refusal and supported-text contrast. Retain low-level shaping tests that describe `.notdef` metrics: production input refusal makes rewriting the entire shaper unnecessary.
- Scope newline handling after #259's normalization change. This proposal modifies only Fonts and Reason slugs, so it does not overwrite #259's Line breaking delta. When #187 later adds font selection, its selected measurement face must become the face this same check uses.

## Risks / Trade-offs

- This intentionally refuses text previously permitted by the Fonts contract, including a template literal when rendered. Template-load text measurement is not introduced.
- Before implementation, reproduce a drawable character measured with Inter's `.notdef` but drawn from an available fallback face. The existing `H中` case proves the code's missing-map path by inspection, but may not select a real CJK fallback on this installation; use a character actually supplied by an existing embedded face. Record measured and rendered face evidence before changing the refusal behavior.
