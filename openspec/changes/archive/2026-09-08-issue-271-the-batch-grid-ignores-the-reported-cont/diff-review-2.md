TREE_SHA256: 89a8dbf37c524918a5ed933716cbc3ee35afc5e3df0d2d383fc564d7285167c3
SPECS_SHA256: 0845537c2936d14913a81159503b2d201d9734070cfeaa88995f88f56dcfd815

## Review: issue-271 implementation diff (round 2)

Verified against `proposal.md`, `design.md`, `tasks.md`, `specs/template-inputs/spec.md`, `diff-review-1.md` and AGENTS.md. No `ANSWERS.md` at the worktree root, and nothing here blocked me, so no `QUESTIONS.md`.

**Gates re-run here** [verified]: `ui/` `npm test` 51 files / 578 tests pass, `npm run lint` clean, `npm run build` succeeds, `cargo fmt --check` clean. `cargo clippy`/`cargo test` not re-run: the diff contains no Rust (`git diff --stat` is two `ui/` files). `git status --short` shows only those two files plus the change folder, matching task 5.4.

**The spec delta is a pure superset** of the published requirement [verified]: `diff` of `specs/template-inputs/spec.md:3-750` against `openspec/specs/template-inputs/spec.md:890-1495` shows only additions plus the replacement of the single `textarea` clause by the per-control block. Round 1's findings 1, 2, 4, 5, 7 and 8 are fixed [verified]: the unset-`select` resting cell exists (`LabelGrid.tsx:533-539`), the `(none)` option is labelled (`:267`), test 1.2 asserts against grid-produced `currentRows` (`:557-560`), `TextEditor`/`TextAreaEditor` are byte-equivalent to `HEAD` (`:142-217`), `editorForControl` is a total switch over a required `InputControl` (`:66-88`), and test 1.2 now distinguishes `r1`'s and `r2`'s entries (`:495-499`).

---

### 1. BLOCKING: `preventDefault()` was added to four editors to satisfy an assertion that tests something else

`LabelGrid.tsx:235`, `:289`, `:346`, `:396` call `e.preventDefault()` in the Escape path of the select, checkbox, number and date editors. `TextEditor:156-159` and `TextAreaEditor:192-195` do not, because round 1 finding 5 made them drop it. Nothing asks for it: `specs/template-inputs/spec.md:212-214` requires only that the key not reach the grid, which `stopPropagation` delivers, and `design.md` records no reason for the split.

The only reader is `ui/src/components/LabelGrid.test.tsx:568-569`. `fireEvent` returns `false` exactly when `preventDefault` was called, so `expect(escapeResult).toBe(false)` measures the suppressed default action — not what the comment on `:567` claims it measures, "the grid does not act on the key".

That clause of the scenario at `specs/template-inputs/spec.md:548-551` therefore has **no assertion that can fail**. Delete `stopPropagation` from `SelectEditor:290` and every assertion in the block stays green: the grid's own Escape hotkey closes the editor without committing, producing the same three outcomes asserted at `:570-572`. In jsdom it cannot even be reached — the grid's hotkeys read `KeyboardEvent.code`, not `.key`, which this very file records at `LabelGrid.test.tsx:153-154`, and `:568` dispatches no `code`.

`design.md` decision 10 is the rule this breaks: "A test that passes here is testing nothing."

Fix: drop the four `preventDefault()` calls so all six editors agree, and either assert the grid did not act (dispatch `{ key: "Escape", code: "Escape" }` and show the grid's hotkey did not fire) or delete `:569` and say plainly in the test what stays unproven.

### 2. The checkbox editor reports a value it cannot display as "unset"

`LabelGrid.tsx:287` seeds state with `parseCheckboxState(editor.value) ?? "unset"`. `DataCell:511-531` keeps the same `null` distinct and falls through to plain text, which is exactly `design.md` decision 9's point: "an unticked box for the string `maybe` would misreport what the row will submit". The editor throws that distinction away.

Failure: a CSV import puts `maybe` into a `checkbox` cell, or a connector puts the number `1` or `0` there (`CellValue` is `string | number | string[]`, `ui/src/api/connectors.ts:51`; `rowsFromMaterialized` copies it through unchanged, `ui/src/lib/connectorRows.ts:90-91`). At rest the cell reads `maybe`/`0`; opened, it reads as an indeterminate box, which `specs/template-inputs/spec.md:227` reserves for the empty string. From there one activation applies `"true"`, so a cell the service coerces to `false` (`src/render/mod.rs:82-88`) goes false→true in a single gesture — the transition `review-1.md` finding 1 settled out of the design.

Not blocking: the delta governs only the *resting* cell's misreporting, and committing without an activation still leaves the value intact (`specs/template-inputs/spec.md:216-221`). But `?? "unset"` is a silent fallback substituting an approximation for a value the control cannot hold, it is recorded nowhere, and no test covers it.

### 3. Tests 1.6 and 1.7 still assert `pruneDataForSubmit` against their own literals

`LabelGrid.test.tsx:826` and `:864` read `currentRows[0].data`. `currentRows` is only ever reassigned from `onRowsChange`, and neither test causes an edit — 1.6 asserts `onRowsChange` was never called (`:822`), and 1.7 performs no edit at all. So both read the literal declared at `:794-796` and `:836-838`; the rename changed the variable, not the object. Both would pass with `LabelGrid.tsx` reverted whole.

Round 1 finding 3 raised this, and the implement log claims it was fixed ("updated tests to assert against `currentRows` ... rather than hand-written static literals", `.agent-runs/implement-agy.log`). It was not. For 1.6 the criterion is still earned in combination (`:822-825` prove the grid altered nothing); for 1.7's "with neither name submitted" nothing grid-side is proven at all.

### 4. The `why` behind six copies of the Enter/Escape handling was deleted

`git show HEAD:ui/src/components/LabelGrid.tsx` carries, above `CellEditor` (lines 74-78): "The cell around an open editor reads a bubbled Enter as a cancel, and a bubbled Escape reaches the grid's document-level hotkeys, so both keys are answered here and go no further." The replacement at `LabelGrid.tsx:424-425` states what the component does. The rationale is now load-bearing in six places and recorded in none, and it is a fact about the vendor, not about this file — the wrapper's own keydown calls `onCancel` on a bubbled Enter [verified, `node_modules/@svar-ui/react-grid/dist/index.es.js:1091-1095`]. AGENTS.md: comments explain why.

### 5. The select editor drops the retained out-of-set option as soon as anything else is picked

`LabelGrid.tsx:239-247` recomputes `options` from `editor.value`, which `onApply` (`:256`) has already overwritten. A cell holding `enormous`: pick `(none)`, and `enormous` leaves the list, so it cannot be picked back inside the open editor — only Escape restores it. `specs/template-inputs/spec.md:187-190` names "the value the cell **currently holds**", and the cell holds `enormous` until commit.

### 6. The two screens still disagree on the wording for "nothing chosen"

The grid labels it `(none)` (`LabelGrid.tsx:267`, `:536`); the print form labels it `Select...` and makes it `disabled hidden`, so it cannot be chosen at all (`ui/src/components/ParamInput.tsx:246-250`). The proposal's own Why cites the two screens disagreeing as the defect being fixed. Cosmetic, and the print form is scoped out — worth recording as a decision rather than leaving it an accident.

### 7. The editor's own `indeterminate` (task 3.2) has no test

`LabelGrid.tsx:288-292` sets it; every `indeterminate` assertion in the suite is against the *resting* box (`LabelGrid.test.tsx:668`, `:857`). Low.

VERDICT: REVISE
