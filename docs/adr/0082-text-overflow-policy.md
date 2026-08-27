# 82. Text overflow is an authored policy

Date: 2026-08-26

## Status

Accepted. Issue [#226](https://github.com/pfa230/labeler/issues/226). Complements [ADR-0080](0080-unify-size-resolution.md) and [ADR-0081](0081-size-vocabulary-content-and-fill.md).

## Context

Previously, text formatting handled overflow implicitly:
- Text with font size ranges stepped down until fitting, but fixed-size text would either be clipped by Typst boxes or silently truncated without clear control.
- In certain contexts (e.g., shipping asset tags or regulatory labels), silent truncation or ellipsis may be unacceptable, requiring an explicit failure instead of printing incomplete data.
- Layout measurement had conditional blank line edge trimming on dynamic labels that caused discrepancy between measured and rendered lines.

## Decision

1. **Explicit `overflow` Field on Text**:
   Add an optional `overflow` field to `text` layout items with two supported policies:
   - `ellipsis` (default): Text that exceeds the allocated box after reaching minimum font size is shortened and given a trailing ellipsis (`...`). Every emitted line is shortened, not only the last, because a glyph wider than the box lands on a line of its own wherever the break falls, and a line left over-wide would be clipped. Two cases cannot be shortened into fitting, and both raise `text_does_not_fit` under `ellipsis` as they would under `fail`: a box narrower than the marker itself, and one shorter than a single line at the chosen size.
   - `fail`: If the text cannot fit within the box bounds at the specified font size (or range minimum), rendering aborts immediately with error code `422` and reason `text_does_not_fit`.
2. **Unified 4-Step Text Layout Pipeline**:
   Unify text measurement and rendering across all formats into a single deterministic 4-step pipeline, run unconditionally for every active `text`:
   - Step 1: Take the input lines (one line unless `multiline`).
   - Step 2: Choose the size — a fixed `font_size`, or the largest 0.5 pt candidate in `[min, max]` that fits, re-breaking the text at each candidate.
   - Step 3: Break at the chosen size, then apply the overflow policy against the width and the line budget.
   - Step 4: Trim blank edge lines at emission.
   The order matters: the size is chosen before the emitted breaks exist, so the breaks come from the selected size and not from a provisional one.
3. **Blank Edge Trimming at Emission**:
   Blank edge lines are counted while determining the font size, and trimmed when emitted to Typst.

## Consequences

- Template authors can enforce strict data integrity guarantees (preventing partially printed text) by specifying `overflow: fail`.
- Text rendering behavior is consistent across fixed, sheet, and dynamic tape label formats.
