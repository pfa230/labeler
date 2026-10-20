## Context

See proposal.md for motivation. `layout_text` in `src/render/helpers.rs` normalizes CRLF immediately before splitting on LF. The existing `crlf_normalisation_matches_lf` test includes a guard asserting that bare CR has nonzero measured width in Inter, but exercises only CRLF normalization. This session inspected that code; it has not rerun the guard or reproduced the historical tape-length effect.

## Goals / Non-Goals

Change the existing text segmentation entry point. No input-wide normalizer, parser change or new newline option is needed.

## Decisions

- **Routine default:** Replace CRLF with LF first, then remaining CR with LF, at the existing normalization site. Reversing the substitutions would turn a CRLF pair into two breaks. Replacing only CRLF leaves the issue's lone-CR input unchanged.
- Extend the existing normalization regression, retaining its nonzero-CR-width guard and comparing emitted lines, chosen font size and fitted dimensions for LF, CRLF and CR. Include mixed separators with edge/interior blank lines in the same test, under both wrapping settings. Removing lone-CR normalization must break the comparisons.
- Reuse the existing `autolength_tape`/`render_tape` test helpers for a rendered CR-versus-LF comparison. Compare decoded image dimensions and pixels rather than adding a new rendering harness; helper-only measurements cannot establish the requested auto-length output equivalence.

## Risks / Trade-offs

Text containing lone CR intentionally becomes multiline. Normalize before fitting so wrapping, size selection and emission all consume those same segments. Coordinate #254 at this boundary: line terminators must be segmented before testing text glyph support.
