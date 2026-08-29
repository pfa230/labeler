# Diff review

AUTHOR: agy
REVIEWER: claude-subagent
VERDICT: REVISE

## What I checked

Read in full: `proposal.md`, `specs/template-inputs/spec.md`, `design.md`, `tasks.md`, `review.md`,
`AGENTS.md`, issue #237 (via `gh`), `docs/adr/0086-a-grid-cell-editor-follows-the-reported-control.md`,
and the whole diff (`git diff` plus the untracked ADR).

Read as reference: `ui/node_modules/react-data-grid/lib/index.js:1258-1345` (`EditCell` and its
container `handleKeyDown`), `ui/src/api/types.ts:20-48` (`InputControl`/`InputSpec`),
`ui/src/lib/labelInputs.ts:189-212` (`getRowInputs`, `pruneDataForSubmit`), `ui/src/lib/csv.ts`,
`git show HEAD:` versions of `LabelGrid.tsx`, `Import.tsx`, `Connect.tsx`, `Import.test.tsx`.

**Gates, run by me, not taken on trust:**

- `ui/`: `npm run lint` clean (no output). `npm run test` **406 passed / 406, 49 files**.
  `npm run build` green (`tsc -b && vite build`, 147 modules, built in 1.08s).
- root: `cargo fmt --check` clean. `cargo clippy --all-targets --all-features` finished with no
  warnings. `cargo test` **671 passed, 0 failed, 2 ignored** (incl. `adr_readme_indexes_every_record_and_invents_none`).
- `ui/dist` is gitignored (`ui/.gitignore:11`), so my `npm run build` left the worktree clean:
  `git status --porcelain` still shows only the six modified files plus the ADR and the change folder.

**Red-first and mutation testing.** I copied `ui/` to a scratch dir with `node_modules` symlinked
(the worktree was never mutated) and ran the new tests against `git show HEAD:` sources, then ran the
new sources with single mutations:

| what I ran | result |
| --- | --- |
| new `LabelGrid.test.tsx` vs HEAD source | 4 failed / 9 passed — the two multiline tests, the inert-cell test and the Shift+Enter test go red |
| new `Import.test.tsx` vs HEAD source | 1 failed / 23 passed — the new CSV multiline test goes red |
| HEAD `Import.test.tsx` vs new source | 23 passed — the diff breaks nothing existing |
| drop `e.stopPropagation()` (LabelGrid.tsx:49) | 1 failed — Shift+Enter is genuinely covered |
| drop `e.preventDefault()` (LabelGrid.tsx:51) | **406 passed — survives** |
| `control === "textarea"` → always true (LabelGrid.tsx:38) | **406 passed — survives** |
| `split(/\r\n\|\n/)` → `split(/\n/)` (LabelGrid.tsx:161) | **406 passed — survives** |

**Design decisions, each verified against source:**

- (a) `cellInput` replaces `isCellEditable`; no `isCellEditable` remains anywhere in `ui/src`. All
  three "editable anyway" states survive the swap and return a synthetic spec:
  `Import.tsx:131` (no template chosen), `Import.tsx:133` and `Connect.tsx:165` (list in flight).
  Compared line by line against `git show HEAD:ui/src/pages/Import.tsx:130-135` and
  `HEAD:ui/src/pages/Connect.tsx:163-167`; `inputs.some(...)` → `inputs.find(...)` is equivalent.
  **No inert regression.**
- (b) Both halves present at `LabelGrid.tsx:47-53`. `EditCell`'s handler (`index.js:1315-1316`) tests
  `event.key === "Enter"` with no `shiftKey` check, so `stopPropagation` is genuinely load-bearing,
  and `preventDefault` does not stop bubbling, so plain Enter still commits.
- (c) `LabelGrid.tsx:166-168` combines error and full value with a blank line, and either alone.
  Asserted exactly at `LabelGrid.test.tsx:263-265`.
- (d) The stored value is never rewritten: `renderCell` only reads, and `pruneDataForSubmit`
  (`labelInputs.ts:197-212`) does no trimming. `Import.test.tsx:487` proves the POST body carries
  `"first line\nsecond line\nthird line"` byte for byte.

**Scope**: no other control is honored — the only branch is `textarea` vs `<input>`
(`LabelGrid.tsx:38`). Nothing in scope was dropped. **Unchanged behavior**: the `—` inert marker,
option columns, the preview radio column, duplicate/remove and `disabled` are all untouched in the
diff and covered by the still-green existing tests. **ADR**: 0086 is free — no `0086*` on `main`, and
none in the six other worktrees — and its row is at `docs/adr/README.md:95`.

## Critical (blocking)

None.

## Moderate

**M1. Two of the four new editor tests cannot fail.**
`ui/src/components/LabelGrid.test.tsx:162` ("commits a textarea edit on plain Enter without inserting
a newline") and `:189` ("leaves prior value intact when Escape is pressed in textarea edit") both pass
against the pre-change `<input>`-only source (4 failed / 9 passed run above; these two are in the
passing set). Neither asserts `tagName`, unlike the Shift+Enter test at `:151`, so both are testing
react-data-grid's own `EditCell` behavior (`index.js:1315-1316`), which needed no change. The
preventDefault mutation confirms it from the other side: deleting `LabelGrid.tsx:51` leaves the whole
406-test suite green. Consequence: the spec's "Enter commits a grid cell rather than breaking the
line" and "Escape abandons a grid cell edit" scenarios have no coverage that could go red, while
`tasks.md` 3.3 claims tests for both. Minimum fix: assert `textarea.tagName === "TEXTAREA"` in both,
so they at least fail when the textarea editor is absent, and say plainly in the task or a comment
that "Enter inserts no newline" is not assertable under jsdom, which never applies the native default.

**M2. Nothing distinguishes the textarea branch from the input branch.**
Mutating `LabelGrid.tsx:38` from `control === "textarea"` to always-true — every data cell in both
grids becomes a `<textarea>` — leaves all 49 files / 406 tests green. Issue #237's third acceptance
criterion ("A cell whose control is not `textarea` is unchanged: still an `<input>`, still commits on
blur") is therefore unverified, and that regression would ship. The existing edit test
(`LabelGrid.test.tsx:79-89`) queries by `aria-label` and casts to `HTMLInputElement` without asserting
`tagName`, so the cast is a compile-time fiction. Fix: assert `tagName === "INPUT"` for a cell whose
`cellInput` reports `control: "text"`.

**M3. `tasks.md` 6.3 is checked and is not honestly claimable.**
6.3 is "Run the grid by hand against a running server: import a CSV… confirm the submitted label
renders both lines." Its evidence is a browser session and a rendered label that no later reader can
retrieve, and `AGENTS.md` ("Templates are visual artifacts") rules on exactly this: "Nothing checks
this, and no task should claim it… A checked box over it would be a claim nobody can verify and no
gate can refuse, which is worse than an honest gap, so the box is gone (#220)." Nothing in the change
folder or the diff records a server run, a CSV, or a rendered label. The box should not have been in
`tasks.md`, and checking it asserts what the project explicitly says must not be asserted. Uncheck it
with a one-line note saying why, or attach retrievable evidence.

## Suggestions

**S1. The CRLF decision is implemented but untested.** `LabelGrid.tsx:161` splits on `/\r\n|\n/` per
design decision (d) and `tasks.md` 4.1, but narrowing it to `/\n/` keeps all 406 tests green. One case
rendering `"line one\r\nline two"` and asserting `getByText("line one")` (no stray `\r`) would lock a
decision the design argued for at length.

**S2. `Connect.tsx`'s new `cellInput` has no test.** `Import` got a page-level test
(`Import.test.tsx:412-489`); `Connect.tsx:163-167` is the same shape and is exercised by nothing new.
`Connect.test.tsx` passes, but it would pass against a broken `cellInput` too, since it never opens a
textarea cell.

**S3. A vacuous assertion.** `LabelGrid.test.tsx:234` asserts
`queryByText("line one\nline two")` is null. testing-library normalizes an element's text before
matching, so a matcher string containing a literal `\n` can never match anything; the assertion holds
regardless of the implementation. The two assertions above it already carry the test.

**S4. A trailing blank line** was added after the final `});` of `LabelGrid.test.tsx`. Harmless, lint
does not flag it, but it is noise in the diff.

**S5. A trailing newline reads as a second line.** `"abc\n"` splits to `["abc", ""]`, so the cell
renders `abc +1` for a value with no visible second line. Arguably correct (the value does hold a line
break) but it is an unstated edge; worth one sentence in the ADR or a deliberate `filter`.

**S6. No IME guard.** The Enter handler (`LabelGrid.tsx:47`) does not check
`e.nativeEvent.isComposing`, so confirming an IME candidate commits the cell. This is pre-existing for
the `<input>` path via `EditCell`, so it is not a regression from this diff, and I raise it only
because the textarea is the control where composing text is most likely.
