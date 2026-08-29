## Context

See `proposal.md` — Why. The facts this design rests on, all verified against the working tree:

- `react-data-grid` is `7.0.0-beta.61`. Its `EditCell` attaches `handleKeyDown` to the container that
  wraps the editor, so a keydown from inside the editor bubbles to it: `Escape` calls `onClose()`
  (cancel), `Enter` calls `onClose(true)` (commit), and Tab navigates. `commitOnOutsideClick` defaults
  to true, so an outside mousedown commits. Everything #237 asks for except the newline is already the
  library's behavior.
- The grid cell is `overflow: clip`, `white-space: nowrap`, `text-overflow: ellipsis`, declared inside
  `@layer rdg.Cell`. A cell cannot show a second line, and an editor cannot grow past the cell box
  without defeating that rule.
- Both grid pages already resolve a per-row `InputSpec[]` through `getRowInputs(row.id)` and already
  derive a per-cell boolean from it, passed to `LabelGrid` as `isCellEditable`
  (`ui/src/pages/Import.tsx:130`, `ui/src/pages/Connect.tsx:163`).

**ADR-0086, "A grid cell editor follows the reported control".** 0085 is the highest number on `main`
and no worktree holds a higher one. The change alters what an operator can enter and what the UI
submits, so it needs one; the ADR records the Enter/Shift+Enter split and why the editor stays inside
the cell box.

## Goals / Non-Goals

**Goals:**

- A cell whose reported control is `textarea` can hold a newline, entered by hand, and that newline
  reaches the submitted `data` unaltered.
- A cell whose value holds a newline says so, whatever its control and whether or not it was typed
  there.
- One source for a cell's activeness and its control, so the two cannot disagree.

**Non-Goals:**

- Honoring `control` for the grid's other data columns. Every data cell is a text `<input>` today, so
  an `integer`, `select`, `checkbox` or `date` is edited as free text there. That is the same
  requirement's first sentence and a wider gap than #237 accepted; it stays exactly as it is and is
  named in `proposal.md` — Impact for a separate issue.
- A composing surface. The grid is for triage of many rows; the print form is where one label's text
  is written.
- Any change to the service, the response shape, or the print form.

## Decisions

### One accessor returning the entry, not two returning facts about it

`LabelGrid` takes `cellInput(row, field): InputSpec | undefined` in place of
`isCellEditable(row, field): boolean`. Absent means the name is not in that row's list: the cell stays
inert exactly as today. Present carries the control the editor and the cell need.

**The two "editable anyway" escapes must survive the swap**, and they are the reason this decision is
not a rename. Both pages today return `true` before consulting any list: Import when no template has
been chosen (`ui/src/pages/Import.tsx:131`, so a CSV can be pasted and edited before a template exists)
and both pages when a row's list has been requested and not yet received (`Import.tsx:133`,
`Connect.tsx:165`, so the grid does not freeze mid-flight). A `cellInput` that returned `undefined` in
those states would make every cell inert and silently regress both. So the pages SHALL return a
synthetic `{ name: field, control: "text" }` entry there, preserving exactly today's behavior: active,
editable, single-line. `undefined` keeps its one meaning, "this row's resolved list does not contain
this name", and `LabelGrid` needs no third state.

Omitting the `cellInput` prop entirely continues to mean every column is active, as omitting
`isCellEditable` does today (`ui/src/components/LabelGrid.tsx:109`), which is what the component's own
tests rely on.

Alternative considered: keep `isCellEditable` and add a second `controlFor(row, field)` prop. Rejected
because both would be derived from the same `getRowInputs` lookup in both pages, and nothing would stop
them disagreeing about one row and field — a cell reported inert by one and editable-as-a-textarea by
the other has no defined behavior. Returning the entry once makes that state unrepresentable. The cost
is a changed prop on an existing component and its two callers, which is the same edit either way.

### Shift+Enter by stopping propagation, everything else inherited

The textarea's own `onKeyDown` handles `Enter` in two ways and nothing else. With `shiftKey`, it calls
`stopPropagation()`: the browser inserts the newline natively and `EditCell`'s container handler never
sees the event. Without `shiftKey`, it calls `preventDefault()` and lets the event bubble: the container
handler still commits, and the browser's own default for Enter in a `<textarea>`, which is to insert a
newline, is suppressed. Both halves are needed. `stopPropagation()` alone would leave plain Enter both
committing and inserting a line break into the control it is closing, and which of the two the value
ends up carrying depends on how React batches the resulting `input` event against the commit.

`Escape` and `Tab` are left untouched, so cancel and navigation stay the library's, not ours.

Alternative considered: handling every key ourselves and calling `onClose` directly. Rejected: it
reimplements four behaviors to change one, and each is a chance to drift from what the rest of the grid
does.

Alternative considered: the grid-level `onCellKeyDown` with `preventGridDefault()`, which `EditCell`
checks before its own handling. Rejected as action at a distance — the rule belongs to the one editor
it governs, not to the grid.

### The editor fills the cell and scrolls

The `<textarea>` is sized to the cell, `resize: none`, and scrolls internally. At the default row
height roughly one line is visible while typing.

This is a real limitation, and the alternative is available: the cell's `overflow: clip` sits in
`@layer rdg.Cell`, so an unlayered class passed through `cellClass` would beat it on the cascade and
let the editor overlay the rows below, Airtable-style. Rejected here because it couples this component
to the internals of a beta library's stylesheet layering, and because nothing in the spec delta needs
it: the newline is enterable, correct, and visible in the cell's tooltip either way. If composing in
the grid turns out to matter, it is a contained follow-up with a visible cost, not a decision this
change should make quietly.

### The read-only cell shows the first line and a count

A cell whose value holds a newline renders its first line, then a muted marker giving how many lines
follow, with the full value in the cell's `title`. Constant row height, no reflow, and the marker is
what distinguishes it from the same words written with a space.

**The `title` already carries the validation error** (`ui/src/components/LabelGrid.tsx:126`), so the
full value SHALL be combined with it rather than replacing it: error first, then the value, separated
by a blank line, and either alone when the other is absent. A cell that is both invalid and multiline
must not lose the message that says why it is invalid.

**Line endings are normalized for display and counting, not for the value.** A CSV written on Windows
delivers `\r\n` inside a quoted field and papaparse preserves it verbatim, so the split that finds the
first line and counts the rest SHALL treat `\r\n` and `\n` alike; otherwise a `\r` rides along into the
rendered first line as a stray character. The stored value is left exactly as it arrived, because
rewriting a cell the operator never touched would make the grid a silent editor of imported data.

Alternative considered: a per-row `rowHeight` function rendering every line. Rejected: one 20-line
value would dominate the grid, virtualization is off (`enableVirtualization={false}`), and the cell
would still need `white-space` overridden. Alternative considered: a return glyph alone, with no count.
Rejected because "there is more" is the thing the operator needs, and a count is what says how much.

## Risks / Trade-offs

- **A one-line editing viewport is unsatisfying for a long value.** → The value is still correct and
  fully typed; the cell tooltip shows all of it; the print form remains the screen for composing one
  label. Named above as a contained follow-up rather than hidden.
- **`stopPropagation` depends on `EditCell` handling keys on the container.** → Verified against the
  installed `7.0.0-beta.61`. A test asserts Shift+Enter inserts a newline *and* that Enter commits, so
  a library change that moved the handler fails the suite rather than silently swallowing newlines.
- **Changing `isCellEditable` to `cellInput` touches both grid pages at once.** → The two call sites
  are three lines each and already do the lookup; the existing inert-cell tests
  (`LabelGrid.test.tsx:91`) cover the behavior that must not change.
- **The line-count marker consumes cell width.** → It renders after the first line's text and inherits
  the cell's ellipsis, so a long first line loses characters to it rather than the marker being pushed
  out. Accepted: knowing the value continues matters more than the last few visible characters.
