## Context

See proposal.md - Why. What matters here is the shape the grid already has.

`ui/src/components/LabelGrid.tsx` registers one `CellEditor` component under two names and lets the
column's `editor` predicate choose between them (`LabelGrid.tsx:60-126, 257-263`). Editors and cells
are module-level components on purpose: a fresh identity per render would remount every cell
(`LabelGrid.tsx:49-51`), so they read the grid's props from `GridPropsContext` rather than a closure.
An edit never mutates the grid's own rows: the `update-cell` intercept converts it into the caller's
`onRowsChange` and returns `false` (`LabelGrid.tsx:274-287`), casting the applied value to `string`.
`cellInput(row, field)` is the single resolution of an entry for a cell, used by both `DataCell` and
the `editor` predicate, and both `Import.tsx:142-147` and `Connect.tsx:222-226` supply it from the
per-row list.

Two facts about the vendor were read out of `@svar-ui/grid-store`'s sourcemap rather than assumed
[verified, `src/DataStore.ts:162-201` and `src/types.ts:178-186` as extracted from
`node_modules/@svar-ui/grid-store/dist/index.js.map`]:

- The object handed to a registered inline editor is
  `{ id, column, value, renderedValue, config?, options?, type }`, where **`id` is the row id**.
- A column's `editor` may be a handler returning `TEditorType | IColumnEditor | null`, and
  `IColumnEditor.config` carries only `template`, `cell`, `options`, `buttons`, `dropdown` and
  `clear`.

## Goals / Non-Goals

**Goals:**

- One decision point that maps a reported `control` to an editor and to a resting presentation.
- The unset state reachable and expressible for `checkbox` and `select`.
- Byte-identical submitted `data` for any cell the operator does not edit.

**Non-Goals:**

- Widening `validateRow` (#388), which stays required-ness and datetime format only.
- Deferral in a grid (#242), list editing in a grid, and image editing in a grid.
- Any change to `pruneDataForSubmit`, `ParamInput`, the API or the service.

## Decisions

### 1. The editor resolves its own entry from `editor.id`, not from the vendor's editor config

A `select` editor needs `values`; a numeric one needs `min` and `max`. Only `values` could ride the
vendor's `config.options`, so routing it that way would need a second mechanism for the numeric
bounds. Instead each editor reads `editor.id` and `editor.column`, finds the row in `rows` from
`GridPropsContext`, and calls the same `cellInput` the cell and the predicate call. One resolution
path, three consumers, and no per-control plumbing in the column config.

Rejected: a module-level ref written by the `editor` predicate when it resolves a spec. The predicate
also runs from `isCellEditable`, which is what makes Tab skip inert cells (`LabelGrid.tsx:253-256`),
so the ref would be written by calls that are not opening an editor and read by one that is.

### 2. One registered name per control, one component branching on `editor.type`

This is what the file does today for two names and why the branch on `editor.type` exists at
`LabelGrid.tsx:92`. Add `labeler-select`, `labeler-checkbox`, `labeler-number`, `labeler-date` and
`labeler-datetime` beside the two. The predicate becomes a total map from `control` to a name, with
`null` for `image` and `list`.

### 3. Every editor applies a string

`update-cell` casts the applied value to `string` (`LabelGrid.tsx:281`) and both grids hold string
cell values. Keeping that means a checkbox applies `"true"`, `"false"` or `""`, and a numeric editor
applies the input's raw string. The submitted `data` therefore keeps exactly the shape it has today,
which is what the acceptance criterion asks, and the backend already coerces both spellings
(`src/render/mod.rs:79-105`).

Rejected: applying a boolean or a number and widening the intercept. It would change the JSON type a
toggled or retyped cell submits, for a service that accepts both, and it would put a type discussion
in the middle of a change about controls.

### 4. The checkbox is tri-state, and unset is reached by a third activation

The held value maps to a state: `true`, `"true"`, `"1"` read as checked; `false`, `"false"`, `"0"` as
unchecked; `""` as unset, drawn with the DOM's `indeterminate`, which is the only native affordance
for "neither". Anything else the control has no form for, and the cell shows it as text (decision 7).

Activation cycles unset, checked, unchecked, unset. A separate clearing control beside the tick box
would be unreachable: Tab is the grid's own commit-and-move, pinned by the test at
`LabelGrid.test.tsx:129-158`, so a second focusable element inside an editor cannot be tabbed to. The
cycle costs discoverability and buys a gesture that works from the keyboard, which the spec's "return
to unset" needs.

When opened on an unrecognized value (where `parseCheckboxState` returns `null`), the editor is
neither checked nor indeterminate. On first activation, `nextCheckboxState(null)` transitions to
`checked` (`"true"`), entering the cycle at `checked`. Entering at `checked` rather than `unset`
ensures an intentional operator activation asserts a definite state without silently clearing
operator data to unset on first click. Subsequent activations cycle to `unchecked` (`"false"`), then
`unset` (`""`).

### 5. The select offers unset, its values, and whatever the cell already holds

Options are: one standing for nothing chosen (value `""`), then each of `values`, then the held value
when it is neither of those. The third exists so the operator sees what the cell holds instead of a
blank control. It does not weaken the "cannot be typed" rule: a `<select>` admits no typed value, and
the retained option is a value the row already carried in from CSV or a connector.

The value is safe either way. An editor only calls `onApply` from a change event, so committing
without choosing commits the held value, never the control's display. Decision 8 makes that a rule
rather than a coincidence.

### 6. Numeric bounds are the control's constraint, not new validation

The numeric editor carries `min`, `max`, `step={1}` for `integer` and `step="any"` for `number`, and
mirrors the control's own validity into `aria-invalid`. A value outside the bounds that is committed
anyway reaches the wire and fails at render, exactly as a CSV-supplied one does today; widening
`validateRow` is #388 and is settled out of this change.

Rejected, clamping on commit: it substitutes an approximation for what the operator asked for, which
this repo forbids outright. Rejected, refusing the commit: it leaves an editor that Escape is the only
exit from, or discards typing on blur. Rejected, rejecting per keystroke: under `min: 5` the
intermediate `1` of `10` would be refused, making `10` untypeable.

### 7. The resting cell is decided in a fixed order

Inert (`—`) first, then a `list` column, then the `⚠ <error>` marker for an empty cell the row flags,
then the multiline `first line +N` treatment, then the control. That order keeps every behavior the
issue requires preserved, and it keeps the multiline rule applying to a cell of any control, which the
spec requires because a CSV or a connector can deliver a newline into a cell nobody opens.

The last step is where a value the control has no form for falls through to plain text: an unticked
box for the string `maybe` would misreport what the row will submit.

### 8. An editor never rewrites a value it cannot display

Stated as a rule so it is tested rather than inherited from the vendor's change-event behavior. It
covers the `select` holding a value outside `values` and the `date` holding an offset-bearing RFC 3339
instant, which `datetimeCellError` accepts for both date controls
(`ui/src/lib/templateFields.ts:27-60`) and no native `date` input can display.

### 9. `date`, `datetime` and the numeric cells look the same as they do today

Their held value already is the control's own form, so "the resting cell follows the control" is a
no-op for them, deliberately. Reformatting an RFC 3339 instant into the control's shorter form for
display would show something other than what the row submits. Only `checkbox`, `select` and `image`
change appearance.

### 10. Tests prove red before green

Each control gets a test that fails against today's tree, which is how the gap survived: the current
suite stays green when the `textarea` branch is deleted. The four Enter/Escape/Tab tests at
`LabelGrid.test.tsx:151-329` are not edited.

### 11. The unset select option wording

The batch grid uses `(none)` for both the resting presentation of an unset select cell and the choice
standing for nothing chosen in the editor. Unlike `ParamInput` (the single-label print form), which
renders a `disabled hidden` `Select...` placeholder because an entry cannot be unset once chosen, the
batch grid must allow returning a cell to the unset state (`specs/template-inputs/spec.md:230-238`), so
its option must be selectable and legible. Aligning `ParamInput` is a Non-Goal (#37) of this change.

## Risks / Trade-offs

- **jsdom may not implement every constraint-validation flag for `type="number"`, `date` and
  `datetime-local`** → assert on what it does implement (`type`, `min`, `max`, `step`, the applied
  value, `aria-invalid`) and say in the test what is being asserted. Never weaken an assertion
  silently to make it pass.
- **The tri-state cycle is not discoverable** → accepted; the alternative is unreachable by keyboard
  (decision 4), and the resting tick box shows the state so the operator can see what a click did.
- **`editor.id` is documented only as `TID`** → verified in the store source, and a test opening a
  `select` editor on a row whose entry differs from another row's fails if the wrong row is resolved.
- **The `image` marker is an assumption, not a settled decision** → recorded in proposal.md; reverting
  it means rendering the held value as text and nothing else.
