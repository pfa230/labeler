TREE_SHA256: 605f266fdae5f0328b9d197f8e09b3483d5c4a25e8005a320b8f8c687d380eb3
SPECS_SHA256: 0845537c2936d14913a81159503b2d201d9734070cfeaa88995f88f56dcfd815

## Diff review: `issue-271-the-batch-grid-ignores-the-reported-cont` (round 4)

Reviewed `ui/src/components/LabelGrid.tsx` and `ui/src/components/LabelGrid.test.tsx` against `proposal.md`, `design.md`, `tasks.md`, `specs/template-inputs/spec.md`, the three prior diff reviews and AGENTS.md. No `ANSWERS.md` at the worktree root. Nothing blocked me, so no `QUESTIONS.md`. I edited no file.

### What I verified independently

**Gates, all re-run here [verified]:** `ui/` `npm test` 51 files / 580 tests pass, `npm run lint` exit 0, `npm run build` exit 0, `cargo fmt --check` clean, `cargo clippy --all-targets --all-features` exit 0, `cargo test` exit 0. `openspec validate ... --strict` reports the change valid. `git status --short` shows only the two `ui/` files plus the change folder, matching task 5.4; `.agent-runs/` is gitignored (`.gitignore:37`).

**The spec delta is a superset [verified]:** diffing the delta body against `openspec/specs/template-inputs/spec.md:890` onward, the only removal is the six-line `textarea` clause, and each of its obligations reappears in the per-control block. `MODIFIED` is valid: the requirement already exists in `openspec/specs/`.

**Task 5.1 [verified]:** the four Enter/Escape/Tab tests at `LabelGrid.test.tsx:151-329` are untouched. The whole diff to the test file is two import lines plus an append at `:456`.

**Vendor facts the design rests on [verified]:** `onApply` only stages into the editor store (`@svar-ui/react-grid/dist/index.es.js:1085-1089`), `onCancel` closes with `ignore: true` (`:1082-1084`), and `onSave` commits only when `isSame(rowValue, editorValue)` is false. `isSame` compares primitives with `===`, not `==` (`@svar-ui/lib-state/dist/index.js`), so a checkbox going `false` → `""` is not swallowed as a no-op. The editor host renders only for the cell being edited (`:1758`), so `editor.id` is the open row's id.

**Prior rounds' findings are fixed [verified]:** round 2's `preventDefault` split is gone (all six editors now do `stopPropagation` only, `LabelGrid.tsx:160-163, 196-199, 238-241, 293-296, 349-352, 398-401`); `CheckboxEditor` no longer collapses an undisplayable value to `unset` (`:292`); `SelectEditor` pins the retained option with a `useState` initializer so it survives picking another option (`:242-252`); design decision 11 records the `(none)` wording. Round 3's finding 1 is answered by `red-run.md`; finding 3 by the new decision-4 paragraph plus the second regression guard (`:974`); finding 4 by `aria-disabled="true"` (`:528`).

I found no correctness defect in `LabelGrid.tsx`.

---

### 1. `red-run.md`'s totals do not describe the file it records

`red-run.md` states "**Total outcome**: 12 failed, 21 passed (33 total)". The file holds 35 tests [verified: `npx vitest run src/components/LabelGrid.test.tsx` → 35 passed]. The two missing ones are the `Review regression guards` at `LabelGrid.test.tsx:933` and `:974`, and both would in fact fail against `origin/main` — each asserts `toHaveAttribute("type", "checkbox")` on the opened editor, and main's `editor` predicate returns `TEXT_EDITOR` for a `checkbox` control, yielding a bare `<input>` with no `type` attribute. So the substance is right and task 1.8's real claim (12 of 12 group-1 tests red, with the failure mode named per test) is earned. What is stale is the arithmetic, in the one document whose job is to be trustworthy about a red run. One line: correct the total, and say the two guards are red on main too rather than leaving their status unstated.

### 2. Tests 1.6 and 1.7 still read `pruneDataForSubmit` against their own literals

`LabelGrid.test.tsx:837` and `:891` call `pruneDataForSubmit(currentRows[0].data, …)`. `currentRows` is only reassigned from `onRowsChange`, and both tests assert it was never called (`:833`, `:888`), so both read the literals declared at `:794-796` and `:869-871`. Those two assertions alone would pass with `LabelGrid.tsx` reverted whole. This is diff-review-1 finding 3, diff-review-2 finding 3 and diff-review-3 finding 2.

It is now honest rather than fixed: the tests carry explicit comments naming it a combination proof, and the neighbouring `expect(onRowsChange).not.toHaveBeenCalled()` is what carries the criterion (any commit would reassign `currentRows` and flip the value assertions). Test 1.5 shows the assertion that does bite on its own — `:783` would go red if the date editor committed `""`. Not blocking; the annotation is the deliverable now, not the assertion.

### 3. An `image` cell renders the literal `image` for any non-empty value, not only a data URI

`LabelGrid.tsx:547-553` branches on `spec.control === "image" && strValue !== ""`. A CSV column or a connector field mapped to an image parameter and holding `logo.png`, a URL, or anything else reads `image` in every row, so the operator cannot see the value, cannot tell two rows apart, and cannot spot the mistake before the run fails at render. The delta's own precedence rule says "a cell holding a value its control has no form for shows that value as text, rather than misreporting it as a state the control can hold" (`specs/template-inputs/spec.md:220-222`). The `image` clause at `:224` is unqualified, so this is within the letter of the contract, and the marker was recorded in `proposal.md` as a struck-easily assumption. Guarding the marker on `strValue.startsWith("data:")` would satisfy both clauses at the cost of one condition.

### 4. `parseCheckboxState` accepts numeric `1`/`0`, which nothing in the plan records

`LabelGrid.tsx:94-95` reads `1` and `0` as checked and unchecked. `design.md` decision 4 and `tasks.md` 3.2 both enumerate exactly six spellings — `true`/`"true"`/`"1"` and `false`/`"false"`/`"0"` — and neither names the numbers. The widening is correct against the service, which coerces `JsonValue::Number` 1 and 0 (`src/render/mod.rs:91-98`), and a connector can deliver a number into a cell (`CellValue` is `string | number | string[]`), so a resting tick box over a numeric `1` does not misreport what the row submits. It is an unrecorded decision, not a defect: one clause in decision 4.

VERDICT: APPROVE
