# 85. Text `wrap` flag, segmentation, and field-level shortening

Date: 2026-08-28

## Status

Accepted. Issue [#251](https://github.com/pfa230/labeler/issues/251). Supersedes the blank-edge trimming rule of [ADR-0045](0045-vertical-text-alignment.md) and updates the text layout pipeline of [ADR-0082](0082-text-overflow-policy.md). Complements [ADR-0050](0050-ink-reservation-at-slot-edges.md), [ADR-0080](0080-unify-size-resolution.md), and [ADR-0081](0081-size-vocabulary-content-and-fill.md).

## Context

Previous text formatting implementations coupled two distinct concepts under the `multiline` key on text layout items:
1. Hard newlines in data (`\n`) were only preserved when `multiline: true` was authored on the layout item; otherwise, step 1 silently discarded all input lines after the first.
2. Soft wrapping of long lines to fit the container box width was enabled by the same `multiline` flag.
3. Form controls in the web UI inspected layout item `multiline` flags to synthesize parameter types when `params` declarations were missing or partial, creating confusing truncation warnings across option variants.
4. Step 4 of the layout pipeline trimmed blank edge lines at emission (ADR-0045), causing discrepancies where a leading blank line shrunk the font during fitting but vanished during rendering.
5. Under the `ellipsis` overflow policy (ADR-0082), dropped trailing lines (such as a trailing newline) could fail to display an overflow marker when the retained block fit the budget.

## Decision

1. **Rename Layout Flag to `wrap` and Refuse `multiline`**:
   The layout item property is renamed to `wrap: bool` (default: `false`). Authoring `multiline` on a `text` layout item is refused at template parse/validation with a migration error pointing to `wrap`.
2. **Step 1 Always Segments**:
   Every hard newline (`\n`) in text data splits the value into segments that are all preserved and laid out, regardless of whether `wrap` is `true` or `false`. `\r\n` is normalised to `\n` to prevent unmapped `\r` glyphs from charging `.notdef` advances.
   - `wrap: true`: Each segment is softly wrapped to the box width at word boundaries (or character boundaries for over-wide words).
   - `wrap: false`: Segments pass through without soft wrapping.
   An empty string value produces one empty line box.
3. **Step 4 Removes Blank-Edge Trimming**:
   Every segment produced by Step 1 reaches emission as its own line box. Blank edge lines (leading or trailing) are rendered, and trailing blank lines emit a trailing `#linebreak()` so Typst allocates their line box at the fitted font size and weight. Intrinsic height is reported as the block height of lines emitted after the overflow policy has been applied.
4. **Field-Level Shortening Marker**:
   Under `Overflow::Ellipsis`, when any line is dropped due to height constraints (even a blank line), the overflow marker (`...`) is appended to the last retained line, shortening characters as needed to fit the box width.
5. **The input list's controls are deliberately left alone.**
   Removing the layout-derived control and the `truncated_elsewhere` flag from the service-built input
   list is issue #269, which depends on this decision: those rules are only wrong once a `text` item
   renders every line of its value. Until #269 lands the flag is still computed and the print form
   still shows its note, which is a stale warning rather than a wrong render.

## Consequences

- Templates authored with `multiline` on text items must be migrated to `wrap: true` (for soft wrapping) or `wrap: false` (to disable soft wrapping).
- Hard line breaks in user data always survive and render as separate lines.
- Blank lines are faithfully rendered and preserve vertical spacing.
- The print form keeps showing a truncation note for a truncation that no longer happens, until #269 removes it.
