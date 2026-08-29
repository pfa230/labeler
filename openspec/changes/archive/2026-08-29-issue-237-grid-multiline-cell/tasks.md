## 1. Prove the gap red

- [x] 1.1 In `ui/src/components/LabelGrid.test.tsx`, add a failing test: a data column whose cell input
      reports control `textarea` is edited, Shift+Enter is pressed between two words, and the value
      committed through `onRowsChange` carries a `\n` at that point. It must fail against the current
      `<input>` editor before any source change.
- [x] 1.2 Add a failing test that a read-only cell holding `"line one\nline two"` renders differently
      from one holding `"line one line two"`. It must fail against the current `<span>`.

## 2. The accessor

- [x] 2.1 Replace `isCellEditable?: (row, field) => boolean` on `LabelGridProps` with
      `cellInput?: (row, field) => InputSpec | undefined`. `undefined` means inert: the cell keeps
      today's `—` marker, is not editable, and is not validated. Omitting the prop keeps meaning every
      column is active, which `LabelGrid.test.tsx` relies on.
- [x] 2.2 Update `ui/src/pages/Import.tsx` to supply `cellInput` from the row inputs it already
      resolves, returning a synthetic `{ name: field, control: "text" }` for the two states that
      return `true` today: no template chosen (`Import.tsx:131`) and a row's list still in flight
      (`Import.tsx:133`).
- [x] 2.3 Update `ui/src/pages/Connect.tsx` the same way, covering its in-flight state
      (`Connect.tsx:165`).
- [x] 2.4 Confirm the existing inert-cell test (`LabelGrid.test.tsx:91`) still passes, rewritten only
      as far as the new prop shape requires. Inert behavior must not change.

## 3. The editor

- [x] 3.1 In `DataEditCell`, render a `<textarea>` filling the cell when the cell's control is
      `textarea`, and the existing `<input>` otherwise. Keep `onBlur={() => onClose(true)}` on both.
- [x] 3.2 In the textarea's `onKeyDown`: `Enter` with `shiftKey` calls `stopPropagation()` and nothing
      else; `Enter` without `shiftKey` calls `preventDefault()` and is left to bubble. `Escape` and
      `Tab` are untouched.
- [x] 3.3 Make 1.1 pass, and add a test that plain Enter commits the edit without inserting a newline,
      and one that Escape leaves the cell's prior value intact.

## 4. The display

- [x] 4.1 In the data column's `renderCell`, a value holding a line break renders its first line
      followed by a muted marker giving how many lines follow. Split on `\r\n` and `\n` alike so a CRLF
      import does not put a stray `\r` into the rendered line. Do not alter the stored value.
- [x] 4.2 Combine the cell's `title`: the validation error first, then the full value, separated by a
      blank line, and either alone when the other is absent. The error tooltip at
      `LabelGrid.tsx:126` must not be suppressed.
- [x] 4.3 Make 1.2 pass, and add a test that a cell that is both invalid and multiline still exposes
      its error message.

## 5. Record the decision

- [x] 5.1 Write `docs/adr/0086-a-grid-cell-editor-follows-the-reported-control.md`: the Enter versus
      Shift+Enter split and why plain Enter needs `preventDefault()`, one accessor rather than two, and
      why the editor stays inside the cell box rather than defeating `@layer rdg.Cell`'s
      `overflow: clip`.
- [x] 5.2 Add its row to `docs/adr/README.md`.

## 6. Gates

- [x] 6.1 From `ui/`: `npm run lint`, `npm run test`, `npm run build`. All three green.
- [x] 6.2 From the repo root: `cargo fmt`, `cargo clippy --all-targets --all-features`, `cargo test`.
      No Rust changes, so this is a no-regression check, and it must still be run and pass.

A by-hand browser-and-rendered-label check (import a CSV whose quoted field holds a newline, confirm the cell shows the line-count marker, edit it with Shift+Enter, and confirm the submitted label renders both lines) is expected of whoever implements this change. Per AGENTS.md ("Templates are visual artifacts"), it carries no checkbox because its only evidence is a transient browser session and rendered label that no automated check can verify.
