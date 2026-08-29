## Why

Implements [#237](https://github.com/pfa230/labeler/issues/237).

The batch grid cannot represent a newline. Its data cell editor is a single-line `<input>`
(`ui/src/components/LabelGrid.tsx:29`), so no gesture puts a line break into a cell, and its read-only
cell renders the value into a `<span>` (`ui/src/components/LabelGrid.tsx:125`), where HTML collapses
`\n` to a space, so a two-line value looks exactly like a one-line one.

Both sources feeding that grid already carry newlines through: CSV import preserves one inside a quoted
field (`ui/src/lib/csv.ts:32`, papaparse), and a connector value passes verbatim
(`ui/src/lib/connectorRows.ts:30`). Since #251 the renderer draws every `\n` segment of a value rather
than keeping only the first line. So the grid is now the single place in the path that loses, or hides,
what the rest of the system carries.

The service already says which cells are affected. `template-inputs` reports a `control` per name per
row, and `textarea` means exactly "the operator may type a newline into this". Both grid pages already
resolve a per-row `InputSpec[]` and already thread a per-cell decision into the grid through
`isCellEditable` (`ui/src/pages/Import.tsx:117`, `ui/src/pages/Connect.tsx:154`). Nothing needs
deriving; the control needs honoring.

## What Changes

- The grid's data cell editor is chosen by the row's reported `control`: a `<textarea>` when that
  control is `textarea`, the existing `<input>` otherwise. The control reaches `LabelGrid` per-row and
  per-field, the way editability already does, because a name can be in one row's list and out of
  another's.
- The `textarea` cell editor's keys are fixed: **Enter commits, Shift+Enter inserts a newline, Escape
  cancels, blur commits.** This is react-data-grid's own `EditCell` contract with the one addition, and
  matches Airtable, Notion and AG Grid's large-text editor. Plain Enter must not insert a newline in a
  grid cell.
- A read-only cell whose value holds a newline shows that it does, rather than rendering the value as
  one collapsed line.
- No change to the print form, to the service, or to any response shape.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `template-inputs`: the requirement "A screen renders the reported inputs and decides nothing else"
  already binds a screen to render each entry "using each entry's `control`", and already governs the
  grid's union columns and per-row inert cells. It says nothing about what a grid cell editor does with
  a control that admits a newline, which is a question a form control does not raise: a grid owns Enter
  and Escape, so a `textarea` cell has to state which gesture commits. The delta adds the cell editor
  and the multiline display rules to that requirement.

## Impact

- `ui/src/components/LabelGrid.tsx`: `DataEditCell`, the data column's `renderCell`, and the props by
  which a caller reports a cell's control.
- `ui/src/pages/Import.tsx` and `ui/src/pages/Connect.tsx`: supply the per-cell control from the row
  inputs they already resolve.
- `ui/src/components/LabelGrid.test.tsx`: the editor and display assertions.
- No Rust, no API, no template change. `openapi.rs` is untouched.

**Out of scope, and a finding this change does not act on.** The grid honors `control` for no data
column today: every data cell gets the same text `<input>`, so an `integer`, `select`, `checkbox` or
`date` control is edited as free text there while the print form renders a real control for each. That
is a wider conformance gap against the same requirement's first sentence, it is not what #237 accepted,
and per the project's scope rule it belongs in its own issue rather than as an extra task here. This
change fixes the `textarea` case only and leaves the others exactly as they are.
