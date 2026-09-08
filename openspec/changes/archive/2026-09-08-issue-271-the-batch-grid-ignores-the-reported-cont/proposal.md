## Why

The batch grid renders one text editor for every column and discards the `control` the service
reports for it (`ui/src/components/LabelGrid.tsx:257-263`), so an `integer`, `number`, `select`,
`checkbox`, `date`, `datetime` or `image` entry is edited as free text. `textarea` is the single
exception, fixed by #237 while the rest was declared out of scope. That contradicts the requirement
the grid is meant to satisfy, "A screen that collects label data SHALL render exactly the entries the
service reports for the label it is about to submit, **using each entry's `control`**"
(`openspec/specs/template-inputs/spec.md:890-893`), and it makes the two screens editing the same
template disagree: the print form does honour `control` (`ui/src/components/ParamInput.tsx`).

The cost is per control. A `select` cell accepts a value outside its declared `values` and says
nothing, so the row dies late as an allowed-values rejection in the middle of a batch. A `checkbox`
cell shows the literal text `true` and invites the operator to retype it. `integer` and `number`
cells carry neither step nor the `min`/`max` the entry already publishes. `date` and `datetime` are
typed by hand while the print form offers a picker. And an `image` cell offers free text over a data
URI, which was never an editor.

Nothing new is needed on the wire: `InputSpec` already carries `control`, `values`, `min`, `max`,
`slider` and `unit` (`ui/src/api/types.ts:43-57`), and since #237 the grid receives the whole entry
per cell through `cellInput`. Everything but `control === "textarea"` is thrown away.

Implements #271.

## What Changes

- The grid's column `editor` predicate picks the editor from the reported `control`, the way it
  already picks a textarea: a `<select>` over `values` for `select`, a tick box for `checkbox`, a
  number input carrying `min`, `max` and a step for `integer` and `number`, a date or date-and-time
  input for `date` and `datetime`, the text editor for `text`, the textarea for `textarea`, and no
  editor at all for `image` and `list`.
- The resting cell follows the control too: a `checkbox` column draws a non-interactive tick box, a
  `select` cell shows the chosen option and an unset one shows that nothing is chosen, and an `image`
  cell shows a marker rather than its data URI. Every existing resting-cell behavior survives: the
  `⚠ <error>` marker for an empty invalid cell, the `first line +N` multiline truncation with its
  `title`, the `list` column's `displayCellText`, and the `—` for a name the row's list does not
  carry.
- An unset `checkbox` and an unset `select` are visibly unset rather than `false` and rather than the
  first option, and the operator can return a cell to that state. This is what
  `openspec/specs/template-inputs/spec.md` already requires of a screen and what the grid cannot do
  today.
- **BREAKING** for nobody on the wire: a cell already holding a valid value submits exactly what it
  submits today, and `validateRow` is untouched.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `template-inputs`: "A screen renders the reported inputs and decides nothing else" gains the
  per-control contract for a grid cell's editor and for its resting presentation, in place of the
  single `textarea` clause it carries today.

## Impact

- `ui/src/components/LabelGrid.tsx`: the registered inline editors and the `DataCell` renderer.
- `ui/src/components/LabelGrid.test.tsx`: new coverage per control; the four Enter/Escape/Tab tests
  at lines 151-329 must keep passing unchanged.
- No Rust, no API and no template change. `pruneDataForSubmit` is already conformant
  (`ui/src/lib/labelInputs.ts:240-262`) and is not touched.
- `validateRow` in `ui/src/pages/Import.tsx` and `ui/src/pages/Connect.tsx` is not touched: a wrong
  value arriving by CSV or paste still reaches the wire and fails at render, exactly as today.
  Widening validation is #388.

### Assumptions recorded

- The `image` resting cell shows a muted marker instead of the data URI it holds today. The issue
  settles that an `image` cell gets no editor but does not say what it displays, and a data URI in a
  table cell is unreadable and would land 50KB of base64 in the multiline `title` tooltip. Struck
  easily if a reviewer disagrees: the cell reverts to rendering the held value as text.
