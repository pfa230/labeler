# 86. A grid cell editor follows the reported control

Date: 2026-08-28

## Status

Accepted. Issue [#237](https://github.com/pfa230/labeler/issues/237). Complements [ADR-0014](0014-csv-import-grid.md), [ADR-0070](0070-service-derives-the-input-list.md), and [ADR-0085](0085-text-wrap-flag.md).

## Context

The batch import and connector grids rendered single-line `<input>` editors for all data cells and collapsed read-only values into single lines in `<span>` elements. As a result, hard newlines (`\n`) present in CSV fields or connector data were visually collapsed, and operators had no way to enter multiline text in the grid.

The service reports parameter input specifications per row via `template-inputs`, designating controls such as `textarea`. While the print form renders dedicated multi-line controls, the grid previously ignored the control type for data cells.

## Decision

1. **One accessor returning the input spec (`cellInput`)**:
   `LabelGrid` accepts `cellInput?: (row: LabelGridRow, field: string) => InputSpec | undefined` replacing `isCellEditable`. A return value of `undefined` marks a cell as inert (`—`), disabled, and unvalidated. Omitting the prop defaults all cells to active single-line text inputs. Both `Import.tsx` and `Connect.tsx` return synthetic `{ name: field, control: "text" }` specs during pre-template selection or when row input lists are pending in flight.

2. **Shift+Enter versus Enter in `<textarea>` editor**:
   When `control` is `textarea`, `DataEditCell` renders a `<textarea>`.
   - `Shift+Enter`: calls `e.stopPropagation()` and nothing else, allowing the browser to natively insert a newline character without triggering `react-data-grid`'s container commit handler.
   - `Enter` (without Shift): calls `e.preventDefault()` and lets the event bubble to `react-data-grid`'s container handler to commit the cell. Calling `preventDefault()` is required because the browser's native default key action for Enter in a `<textarea>` is to insert a newline, which would otherwise mutate the committed value.
   - `Escape` and `Tab` bubble unchanged, preserving standard grid cancel and navigation behavior.
   - Blur commits the edit.

3. **Editor sized to cell box**:
   The `<textarea>` editor fills the grid cell (`resize: none`, internal scroll) rather than attempting to bypass `@layer rdg.Cell`'s `overflow: clip` styling. This keeps styling maintainable and avoids coupling to library internals.

4. **Multiline read-only display with line-count marker**:
   A read-only cell holding a newline renders its first line followed by a muted line-count marker (e.g. `+1`) indicating how many lines follow. Line endings (`\r\n` and `\n`) are normalized for display and line counting without altering the stored data.

5. **Combined tooltip title**:
   The cell `title` attribute combines any validation error followed by the full multiline text separated by a blank line (or either alone when the other is absent), ensuring error messages are never obscured by multiline text previews.

## Consequences

- Operators can enter and edit multiline values in `textarea` columns using Shift+Enter.
- Hard newlines in data are visually indicated by line-count badges in read-only grid cells.
- Validation error tooltips and multiline content coexist without collision.
- The grid editor stays within its cell boundary without introducing stylesheet layering hacks.
