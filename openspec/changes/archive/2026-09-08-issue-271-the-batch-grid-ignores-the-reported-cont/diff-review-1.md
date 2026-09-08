TREE_SHA256: 83336abdb824291c1f0f4748b009634a05383adaad8ef2abfa617ecbe4fadde0
SPECS_SHA256: 0845537c2936d14913a81159503b2d201d9734070cfeaa88995f88f56dcfd815

## Review: issue-271 implementation diff

Verified against `proposal.md`, `design.md`, `tasks.md`, `specs/template-inputs/spec.md`, and AGENTS.md. Gates run here [verified]: `ui/` `npm run lint` clean, `npm run test` 51 files / 578 tests pass, `npm run build` succeeds. No Rust in the diff. `git status --short` shows only the two `ui/` files plus the change folder, matching task 5.4. The spec delta is a pure addition to the existing requirement (`diff` of `specs/template-inputs/spec.md:3-750` against `openspec/specs/template-inputs/spec.md:890-1495` drops nothing).

### 1. BLOCKING: the unset `select` resting cell is not implemented, and task 4.3 is checked anyway

`DataCell` gained a `checkbox` branch (`ui/src/components/LabelGrid.tsx:514-535`) and an `image` branch (`:537-543`), but no `select` branch. An unset `select` cell falls through to `ui/src/components/LabelGrid.tsx:545` and renders `<span></span>`, byte-identical to an unset `text` cell.

The delta says: "**An unset `checkbox` or `select` cell SHALL show that nothing is chosen**, distinguishably from `false` and from the first option" (`specs/template-inputs/spec.md:225`). `tasks.md:4.3` says "for an unset cell a presentation showing that nothing is chosen rather than the first declared value", and it is marked `[x]`. `design.md` decision 9 states "Only `checkbox`, `select` and `image` change appearance" -- select's appearance did not change. The change built an explicit `indeterminate` box for the checkbox rather than relying on emptiness, then relied on emptiness for the select. Showing nothing is not the same as showing that nothing is chosen.

Note for the fix: the inert `—` is explicitly ruled out by `specs/template-inputs/spec.md:229-231` ("the inert marker is not available to express it"), so it needs its own presentation.

### 2. BLOCKING: the `select` "still submits `enormous`" criterion is asserted by a test that cannot fail

`ui/src/components/LabelGrid.test.tsx:541-543` wraps the only grid-derived assertion in `if (onRowsChange.mock.calls.length > 0)`. When Enter commits without a change and no `update-cell` fires, the block is skipped and nothing is checked, which is exactly the path the scenario is about.

The line that follows, `ui/src/components/LabelGrid.test.tsx:545-546`, does not close the gap: `selectRows[1].data` is the test's own literal `{ size: "enormous" }`. The `update-cell` intercept builds new objects (`ui/src/components/LabelGrid.tsx:645`), so the grid can never touch that object. `expect(pruned.size).toBe("enormous")` therefore tests `pruneDataForSubmit` against a hand-written input and would pass with the grid deleted. `tasks.md:1.2` requires proof that the value survives "in the cell **and in the submitted row**"; only the in-cell half is proven (`:539`). Test 1.3 shows the right pattern: it asserts against `currentRows`, the array the grid produced.

### 3. Same tautology in tests 1.6 and 1.7

`ui/src/components/LabelGrid.test.tsx:805` (`pruneDataForSubmit(imageRows[0].data, ...)`) and `:839` (`pruneDataForSubmit(rowsUnset[0].data, ...)`) read the tests' own literals. They assert `pruneDataForSubmit`'s behavior, which this change does not touch, not the grid's. Less severe than finding 2 because nothing in those two cases could have edited the row, but they are recorded as evidence for criteria they do not test.

### 4. No red-before-green exists for the select resting requirement

`ui/src/components/LabelGrid.test.tsx:836` is `expect(screen.queryByText("small")).toBeNull()`. The pre-change tree rendered `String(data[field] ?? "")` for a `select` cell too, so `small` never appeared there either: this assertion passes against `HEAD` unmodified. `tasks.md:1.8` claims each group-1 test was confirmed failing for the reason the issue names. Test 1.7 as a whole does go red on the checkbox half, which is how this passed unnoticed, but the select half of criterion 1.7 was never red and finding 1 is the result.

### 5. `e.preventDefault()` was added to the text and textarea Escape path

`ui/src/components/LabelGrid.tsx:159` and `:196` call `e.preventDefault()` in `cancel`; the pre-change `CellEditor` called only `stopPropagation()`. `tasks.md:3.6` says "Leave the `text` and `textarea` branches behaving exactly as they do today". Suppressing Escape's default action in a text control is small but real (it is what an IME candidate window and a browser's field-revert read), it is undocumented in `design.md`, and it is load-bearing only for a self-authored assertion, `expect(escapeResult).toBe(false)` at `ui/src/components/LabelGrid.test.tsx:568`. The spec requires the key not to reach the grid (`specs/template-inputs/spec.md:212-214`), which `stopPropagation` already delivers. Either drop the `preventDefault` from the two pre-existing editors, or state in `design.md` why the behavior changed.

### 6. Low: the "nothing chosen" option carries no label

`ui/src/components/LabelGrid.tsx:245` seeds `options` with `""` and `:270-272` renders `<option value="">{""}</option>`. The dropdown's first row is blank rather than saying anything. `specs/template-inputs/spec.md:187` asks for "one standing for nothing chosen", which a blank row meets only by convention, and it reads as an accidental empty entry beside three labelled ones.

### 7. Low: `editorForControl`'s fallback is unreachable

`InputSpec.control` is required and `InputControl` is a closed union (`ui/src/api/types.ts:28-43`), so the optional parameter and the `default: return TEXT_EDITOR` at `ui/src/components/LabelGrid.tsx:66,85-86` cannot be reached. It also silently converts a future control into a free-text cell, which is the failure this change exists to remove. A total switch with no default would make the compiler name the next added control.

### 8. Low: the row-resolution risk `design.md` says is covered by a test is not

`design.md` Risks says "a test opening a `select` editor on a row whose entry differs from another row's fails if the wrong row is resolved". Every new test's `cellInput` ignores its row argument (`ui/src/components/LabelGrid.test.tsx:471`, `:496`, `:601`, `:665`, `:786`, `:815`), so nothing distinguishes row `r1`'s entry from `r2`'s. The lookup at `ui/src/components/LabelGrid.tsx:435` is correct and `editor.id` is confirmed to be the row id [verified, `node_modules/@svar-ui/grid-store/dist/types/types.d.ts:178-186`], so this is a missing mitigation rather than a defect.

VERDICT: REVISE
