## Why

Implements [#318](https://github.com/pfa230/labeler/issues/318).

A template may declare `type: list` and the service reports an ordinary entry for it with the `list`
control, but no screen can draw that control. On the print form `ParamInput` returns `null` for
`control === "list"` (`ui/src/components/ParamInput.tsx`), so `FieldForm` shows the entry's label with
no editor beside it, and `PrintForm.tsx:115` exempts the entry from the completeness check, so the form
submits looking valid and the render answers `422 MissingField` naming a parameter the operator was
never offered. Today only an API caller sending `data: {"tags": ["A", "B"]}` can supply one.

## What Changes

- `ParamInput` draws an editor for `control === "list"`: one row per element, each a text control with
  a remove control and move-earlier and move-later controls, plus a control appending an empty element.
  The value handed to `onChange` is `string[]` in row order. It reaches every screen built on
  `ParamInput`, which today is `FieldForm` and so the print form alone.
- The print form holds the empty list for a `list` entry that publishes no default, so an entry the
  operator never touches submits `tags: []` rather than being dropped by `pruneDataForSubmit`.
- **BREAKING (UI behaviour):** `PrintForm.tsx:115`'s blanket `if (input.control === "list") return true`
  exemption goes. A `list` entry is decided by the same completeness rule as every other entry, which
  it always satisfies, because `[]` is a value.
- The two published statements the issue names, both saying no screen draws a `list` control, are
  corrected for the print form and kept for the three screens that still draw none.

Out of scope, unchanged, and each owned by an open issue: the batch grid's non-editable `—` cell
(#271), the CSV import grid's dropped column (#320), the connect screen's excluded mapping (#348), and
replacing the move controls with dragging (#347).

**Cut from this change, and each owned by its own open issue:**

- The `Use default` checkbox renders a list default through `String(value)`, so `["A", "B"]` reads as
  `A,B`, indistinguishable from one element holding a comma. Real, and outside #318, which does not
  name the checkbox (#351).
- `openspec/specs/datetime-params/spec.md`'s parameter-type table publishes, in its **UI form control**
  column, "`list` control (#318 builds the editor; until it lands a screen reports the entry and draws
  nothing)". That cell is false once this lands, and `openspec/specs/` is written by archive and never
  by hand, so correcting it needs a delta of its own (#352). #318 says its delta lands in `template-inputs`,
  and it does.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `template-inputs`: **`An input list describes the controls one label needs`** — the
  "tolerate a `control` it cannot draw" paragraph says `list` is that control *today* and names #318 as
  what changes it. The general obligation survives, because the batch grid, the CSV import grid and the
  connector mapping screen still draw none; the claim that no screen draws one does not. The
  consequence paragraph is kept for those three screens and corrected: they submit through
  `POST /api/batch`, so the failure is `422 BatchInvalid` carrying a `MissingField` entry in
  `details.failures`, not a top-level `MissingField`.
- `template-inputs`: **`A screen renders the reported inputs and decides nothing else`** — states the
  print form's list editor, decides what "no value" and "a visibly unset state" mean for a `list` entry
  **on that screen**, and says what the pruning rule does with an empty list. Every rule it adds is
  scoped to the print form; a screen that draws no `list` control is left to the tolerate rule above.

## Impact

- `ui/src/components/ParamInput.tsx`: the `control === "list"` branch that returns `null` becomes the
  editor. This file is the only place the editor lands; `ParamInput` has exactly one consumer.
- `ui/src/pages/print/PrintForm.tsx`: the completeness exemption goes, and the initial-state and
  arrival paths hold `[]` for a `list` entry publishing no default.
- Tests: `ui/src/components/ParamInput.test.tsx` asserts today that a `list` renders no control and is
  replaced; `ui/src/pages/print/PrintForm.test.tsx` gains the submission, ordering, deferral and
  accessibility cases; `ui/src/pages/print/FieldForm.test.tsx` updates its list rendering case to assert the editor.
- No server change. `InputControl::List` (`src/models.rs:122`) is already reported, `ParamValue`
  already admits `string[]` (`ui/src/api/types.ts:7`), and `pruneDataForSubmit` already passes an array
  through (`ui/src/lib/labelInputs.ts:251-256`). No Rust file is touched.
- Unchanged and asserted so: `ui/src/pages/Import.tsx`, `ui/src/components/LabelGrid.tsx` and
  `ui/src/pages/Connect.tsx` keep their list exclusions, `ui/src/pages/print/FieldForm.tsx` source is not
  edited, and none of their existing tests are.
