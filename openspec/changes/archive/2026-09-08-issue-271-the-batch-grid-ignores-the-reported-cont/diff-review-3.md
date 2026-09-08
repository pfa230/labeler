TREE_SHA256: eb3c6a6048a9674da8a0efa293c30e9f188fbcb6eac742407c7c3aaa853edd1e
SPECS_SHA256: 0845537c2936d14913a81159503b2d201d9734070cfeaa88995f88f56dcfd815

## Diff review: `issue-271-the-batch-grid-ignores-the-reported-cont` (round 3)

Reviewed `ui/src/components/LabelGrid.tsx` and `ui/src/components/LabelGrid.test.tsx` against `proposal.md`, `design.md`, `tasks.md`, `specs/template-inputs/spec.md`, the two prior diff reviews and AGENTS.md. No `ANSWERS.md` at the worktree root. Nothing blocked me, so no `QUESTIONS.md`. I edited no file in the worktree; all experiments ran on a copy under `/tmp/redcheck`.

### What I verified independently

**Gates, all re-run here [verified]:** `ui/` `npm test` 51 files / 579 tests pass, `npm run lint` exit 0, `npm run build` exit 0, `cargo fmt --check` clean, `cargo clippy --all-targets --all-features` exit 0, `cargo test` exit 0. `openspec validate ... --strict` reports the change valid. `git status --short` shows only the two `ui/` files plus the change folder, matching task 5.4.

**The spec delta is a superset [verified]:** diffing the delta body against `openspec/specs/template-inputs/spec.md:890` onwards, the only removal is the six-line `textarea` clause, and every one of its obligations (line-break control, Enter commits, Shift+Enter inserts, Escape abandons, blur commits, Enter never inserts, newline reaches `data` unaltered) reappears in the new per-control block at `specs/template-inputs/spec.md:181-213`.

**Red before green [verified]:** running the new test file against `origin/main`'s `LabelGrid.tsx` gives 12 failed / 22 passed. Every test 1.1-1.7 fails on the `TEXT_EDITOR` fallback, as tasks 1.1-1.7 require.

**The assertions are load-bearing [verified]:** I ran 12 mutations against the shipped implementation. Every one was caught: pinning `aria-invalid` false, pinning `step` to `any`, dropping `min`/`max`, dropping the retained out-of-set select option, dropping the empty select option, pinning the editor's `indeterminate` false, breaking the unchecked→unset leg of the cycle, deleting the resting `(none)` branch, deleting the image marker, routing `image` to the text editor, and stripping the state from the resting box's accessible name. Round 2's findings 1, 2, 4, 5, 6 and 7 are fixed; `TextEditor`/`TextAreaEditor` remain behaviourally identical to `origin/main`'s `CellEditor`; the four Enter/Escape/Tab tests are untouched.

I found no correctness defect in `LabelGrid.tsx`.

---

### 1. BLOCKING: `tasks.md` claims a red run it did not record, and its preamble is false for test 1.9

`tasks.md:1-5` states of group 1: "Every test in this group ... is run before any change to `LabelGrid.tsx`. **Each must fail against the current tree.**" `tasks.md:29` checks off 1.8: "Run `npm test` in `ui/` and **record** which of 1.1-1.7 fail and how, so the green run at the end is a change in the same assertions rather than in the tests."

Both claims are unearned as they stand:

- **Test 1.9 passes against the pre-change tree** [verified]: 22 of 34 pass on `origin/main`, and 21 of those are the pre-existing suite, so the extra passer is `LabelGrid.test.tsx:920` ("1.9 checkbox with unrecognized value"). It asserts `getByText("maybe")`, `editor.indeterminate === false` and `editor.checked === false` — all true of the text `<input>` main opens — and never asserts the editor is a checkbox. It is in group 1's `describe` and is covered by the preamble.
- **The record does not exist** [verified]: nothing in the change folder or `.agent-runs/` holds it. `.agent-runs/implement-agy.log` and `apply-stage.out` are round 3's fix log and describe only the six review findings; `.agent-runs/` is gitignored regardless, so nothing the commit carries can show 1.8 was performed.

AGENTS.md: "A claim you did not earn is worse than an admitted gap. A checked box ... is a claim the next reader trusts instead of redoing the work. Make one only after performing the thing, and never write a step whose completion nothing can show." `design.md` decision 10 is the same rule stated for this change.

The substance is fine — I reproduced the red run and it is exactly what 1.8 asked for. What must change before this lands is the claim: put the red-run result in the change folder (12 of 13 red, naming 1.9 as the exception and why it is a regression guard rather than a red-first test), or reword the preamble and 1.8 so neither asserts something the tree cannot show. Mutation M1 confirms 1.9 is not worthless: removing the `checkboxState !== null` guard at `LabelGrid.tsx:518` makes it fail. It just is not a red-first test, and tasks.md says it is.

### 2. Tests 1.6 and 1.7 still assert `pruneDataForSubmit` against their own literals

`onRowsChange` is called **zero** times in tests 1.2, 1.6 and 1.7 [verified by instrumenting each with the mock's call count]. `currentRows` is only ever reassigned from `onRowsChange`, so `LabelGrid.test.tsx:837` and `:891` read the object literals declared at `:794` and `:869`, not anything the grid produced. For 1.6 that reduces to a direct test of `pruneDataForSubmit`.

This is diff-review-1 finding 3 and diff-review-2 finding 3, reported fixed both times and still present. Not blocking: `expect(onRowsChange).not.toHaveBeenCalled()` at `:833` earns 1.6's criterion in combination, and in 1.7 any commit that wrote a value would fire `onRowsChange` and flip `:888-889`. But the "fixed" claim about it is false for the third time.

### 3. The checkbox editor's first activation from an undisplayable value is an undocumented decision with no test

`nextCheckboxState(null)` returns `"checked"` (`LabelGrid.tsx:108-109`), so one activation on a cell holding `maybe` applies `"true"`. `design.md` decision 4 enumerates unset, checked and unchecked and says only that anything else "the control has no form for ... the cell shows as text"; it does not say what an activation from that state does. Test 1.9 covers commit-without-activation only. The choice is defensible (the alternative, cycling to unset first, silently discards the operator's data), but it is recorded nowhere and nothing pins it.

### 4. The resting tick box is non-operable by three mechanisms but tells assistive tech it is operable

`LabelGrid.tsx:521-531` renders `role="checkbox"` with `tabIndex={-1}`, `readOnly`, `pointerEvents: "none"` and a controlled `checked` with a no-op `onChange`. That is genuinely not operable — `readOnly` is inert on checkboxes, but the controlled `checked` reverts any programmatic toggle. What is missing is `aria-readonly` or `aria-disabled`, so a screen reader announces an interactive checkbox that will not respond. `specs/template-inputs/spec.md:222-223` asks for "a tick box that is not itself operable"; nothing there requires the ARIA state, so this is a nit, not a violation.

VERDICT: REVISE
