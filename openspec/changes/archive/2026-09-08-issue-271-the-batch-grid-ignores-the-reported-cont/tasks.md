## 1. Failing tests first

Every test in this group goes into `ui/src/components/LabelGrid.test.tsx` and is run before any
change to `LabelGrid.tsx`. Each must fail against the current tree, and the failure must be the one
the issue names: the cell falls back to the text editor, or the resting cell renders the raw value.
A test that passes here is testing nothing (design.md - decision 10).

- [x] 1.1 A test per control asserting which editor the cell opens: `select`, `checkbox`, `integer`,
      `number`, `date` and `datetime`. Run them and confirm each fails on today's `TEXT_EDITOR`
      fallback.
- [x] 1.2 `select`: an unset cell's editor offers exactly the three declared values plus the choice
      standing for nothing chosen; a cell holding `enormous` also offers `enormous` and nothing else;
      opening and committing without choosing leaves `enormous` in the cell and in the submitted row;
      Escape abandons a chosen option and the grid does not act on the key.
- [x] 1.3 `checkbox`: one activation from an unset cell yields a true value and a ticked resting box
      rather than the text `true`; three activations from unset read checked, unchecked, then unset,
      and the row submits no key for that name once unset.
- [x] 1.4 `integer` and `number`: the editor carries the published `min` and `max`, reports a typed
      `20` invalid under `max: 9` and cannot be stepped past `9`; an `integer` steps by 1 while a
      `number` accepts `2.5`; an entry carrying `slider: true` still opens the plain numeric control.
- [x] 1.5 `date` and `datetime`: the editors are a date control and a date-and-time control; a `date`
      cell holding `2026-09-01T12:00:00Z` still holds it after the editor is opened and committed
      without typing.
- [x] 1.6 `image`: double-clicking the cell opens no editor, the cell shows that it holds an image
      rather than its data URI, and the row still submits the held value.
- [x] 1.7 An unset `checkbox` cell reads as neither checked nor unchecked and an unset `select` cell
      does not read as its first declared value, with neither name submitted; an empty cell its row
      flags keeps the `⚠ <error>` marker whatever its control.
- [x] 1.8 Run `npm test` in `ui/` and record which of 1.1-1.7 fail and how, so the green run at the
      end is a change in the same assertions rather than in the tests (recorded in `red-run.md`).

## 2. Editor selection

- [x] 2.1 Add the per-open resolution of an entry inside `LabelGrid.tsx`: from `editor.id` and
      `editor.column`, find the row in `rows` and call `cellInput`, both read from
      `GridPropsContext`, answering undefined for a row that is gone (design.md - decision 1).
- [x] 2.2 Register `labeler-select`, `labeler-checkbox`, `labeler-number`, `labeler-date` and
      `labeler-datetime` beside the two existing names, served by module-level components that branch
      on `editor.type` (design.md - decision 2).
- [x] 2.3 Replace the column `editor` predicate's `textarea`-or-text ternary with a total map from the
      reported `control` to a registered name, returning null for `image` and for `list`, and keeping
      the existing `!row || disabled`, missing-`cellInput` and missing-spec guards exactly as they
      are.

## 3. The editors

- [x] 3.1 `select`: a control whose choices are the entry's `values`, one standing for nothing chosen,
      and the value the cell currently holds where it is neither of those, and nothing else.
- [x] 3.2 `checkbox`: a tick box over three states, drawing the unset one with the DOM's
      `indeterminate`; `true`/`"true"`/`"1"` read as checked, `false`/`"false"`/`"0"` as unchecked,
      `""` as unset; activation cycles unset, checked, unchecked, unset, applying `"true"`, `"false"`
      and `""` (design.md - decisions 3 and 4).
- [x] 3.3 `integer` and `number`: a numeric control carrying `min` and `max` where the entry publishes
      them, stepping by 1 for `integer` and unconstrained for `number`, mirroring the control's own
      validity into `aria-invalid`, and ignoring `slider` (design.md - decision 6).
- [x] 3.4 `date` and `datetime`: a date control and a date-and-time control.
- [x] 3.5 Every editor added here answers Enter by committing and Escape by abandoning, both without
      letting the key bubble, and commits when focus moves away. Apply only a string, so the
      `update-cell` intercept's cast stays as it is, and apply only from a change event, so a value
      the control cannot display is committed unaltered (design.md - decisions 3 and 8).
- [x] 3.6 Leave the `text` and `textarea` branches behaving exactly as they do today, Shift+Enter
      included.

## 4. The resting cell

- [x] 4.1 Order `DataCell`'s branches: a name absent from the row's list, then a `list` column, then
      the `⚠ <error>` marker for an empty flagged cell, then the multiline `first line +N` treatment
      for any control, then the control (design.md - decision 7). The first four keep their current
      behavior.
- [x] 4.2 `checkbox`: a tick box that is not itself operable, checked, unchecked or `indeterminate`,
      carrying an accessible name naming the field and the state it shows.
- [x] 4.3 `select`: the chosen option, and for an unset cell a presentation showing that nothing is
      chosen rather than the first declared value.
- [x] 4.4 `image`: a marker saying the cell holds an image, in place of the data URI.
- [x] 4.5 Every other control, `date` and `datetime` included, shows the value as held; a value the
      control has no form for falls through to that same plain text rather than being drawn as a state
      the control can hold (design.md - decision 9).

## 5. Gates

- [x] 5.1 Run `npm test` in `ui/` and confirm every test from group 1 now passes, and that the four
      Enter/Escape/Tab tests at `LabelGrid.test.tsx:151-329` pass with no edit to them.
- [x] 5.2 Run `npm run lint` and `npm run build` in `ui/`.
- [x] 5.3 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features` and `cargo test`.
- [x] 5.4 Confirm with `git status --short` that the diff touches only
      `ui/src/components/LabelGrid.tsx`, `ui/src/components/LabelGrid.test.tsx` and this change
      folder: `validateRow`, `pruneDataForSubmit`, `ParamInput`, `Import.tsx` and `Connect.tsx` are
      out of scope.
