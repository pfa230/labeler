TREE_SHA256: 94580acd9e15fcb859a7bfb6edee5b49734ed7e1a04468bfa08d8acd3cae6392

# Diff review — issue-318 (list parameter form control)

**What I verified as correct.** The editor in `ParamInput.tsx:278-403` matches the delta requirement clause for clause: rows derived from the prop with no draft state, `aria-disabled="true"` plus an early-returning handler for the boundary move controls rather than `disabled` (`:322`, `:349`), focus placement after move and removal via a layout-effect ref map (`:66-79`), accessible names carrying `name` and 1-based position, and every control natively disabled while deferred. The `PrintForm.tsx` edits (`:31`, `:54`, `:61`, deletion of the `:115` exemption) are behaviourally right — I proved both seeding paths necessary by mutation, below. All 49 test files / 454 tests pass, `npm run lint` and `npm run build` are clean, and all 15 new tests fail against the pre-change components (no whole-test rubber stamps). The delta correctly supersedes the stale published scenario at `openspec/specs/template-inputs/spec.md:238`.

Three findings block.

---

### 1. BLOCKING — `ui/src/app/toast.tsx` is an out-of-scope source change

`ui/src/app/toast.tsx:1,11-16,22-27,40-41` rewrites the toast provider's timer handling (a `timeouts` ref map, an unmount cleanup effect, clearing on manual dismiss). It is named by no task in `tasks.md`, by no requirement in the delta, and by no line of `proposal.md`'s Impact section — which enumerates the touched files and says of the editor "This file is the only place the editor lands". No test was added for it.

It is also not needed. [verified] I copied the `ui/` tree to a scratch dir, restored `toast.tsx` from `HEAD`, and left every other change in place: **49 test files / 454 tests passed**. Nothing in this change depends on it.

It may well be a real leak fix, but it lands here as a behaviour change to a shared app-level provider that no reviewer was asked to judge and no test protects. Revert it from this change; file it as its own issue if it is wanted.

### 2. BLOCKING — both print-form seeding edits ship with no test that can detect their removal

`tasks.md` 4.1 and 4.6 are checked and each states, in its own words, that it covers one of these edits. Neither does. Two mutations, each run against the shipped test suite:

**Mutation A — revert the `withArrivals` return guard** (`PrintForm.tsx:61` back to `return deferred === value.deferred ? value : …`, task 2.2's edit): **all 454 tests pass.** [verified]

Task 4.1 says "This is the case 2.2's guard would silently break, so assert the submitted body, not just that the button is enabled." Its template puts `tags` in `detail.inputs.default`, so `initialFieldState` (`:31`) seeds it and `withArrivals`'s new branch never runs. The guard only matters for a `list` entry arriving in a **later** list for the first time — the shape the existing test at `PrintForm.test.tsx:627` ("brings a later entry named for an Object.prototype member in deferred") already uses for defaulted entries. I wrote that probe (branch `standard`→`pro` brings in `tags`): it fails against the mutation and passes against the shipped code. `design.md` names this exact risk ("The `withArrivals` return guard is easy to miss → … the form would look right until submission") and asserts "A test that submits without touching the editor catches it" — it does not.

**Mutation B — delete `initialFieldState`'s list seeding** (`PrintForm.tsx:31-32`, task 2.1's edit): **all tests pass, 4.6 included.** [verified]

4.6 asserts through `lastInputsData()`, which reads the *last* `/inputs` request — by then `withArrivals` has already seeded `tags`. The claim in `design.md` is about the *first* request ("the very first request already carries `tags: []`"), which is what `initialFieldState` buys and what the extra round trip costs without it. A probe reading the **first** `/inputs` body fails against the mutation (`expected undefined to deeply equal []`) and passes against the shipped code.

Neither delta scenario reaches these paths either: `specs/template-inputs/spec.md:762` ("An untouched list entry submits the empty list") describes the first-paint entry, and there is no scenario for a `list` entry appearing in a later list, though `:732` has exactly that scenario for a defaulted entry.

Both edits are correct. Both are unprotected, in the shape `AGENTS.md` calls the loop's #1 defect. Add the two cases and re-check the boxes honestly.

### 3. BLOCKING — neither cut item was filed, and one leaves a published spec knowingly false

`proposal.md:30-39` cuts two items and says "**each needs its own issue before it is done anywhere**". The approving plan review conditioned on it: "AGENTS.md tracker rule is satisfied once those issues are filed before implementation" (`review.md`). [verified] `gh issue list --state all --limit 300` has no issue for either; the newest issues are #347/#348 (already-known out-of-scope screens) and #350.

The second cut is worse than a lost backlog note. `openspec/specs/datetime-params/spec.md:311` publishes, in its **UI form control** column:

> `list` control (#318 builds the editor; until it lands a screen reports the entry and draws nothing)

That sentence is false the moment this commit lands, and `archive-merge-check.sh:141` means a published spec can only be corrected by a delta — i.e. by another change through the loop. Shipping the falsehood with nothing recording that it must be corrected is a published contract that contradicts the code and has no owner. File both issues (and reference the `datetime-params` one from `proposal.md`) before this lands.

---

### Non-blocking

**4.** `ui/src/pages/print/FieldForm.test.tsx:277-286` is edited, correctly and necessarily — the old case asserted the removed behaviour. But no task names it (`tasks.md` §3 names `ParamInput.test.tsx`, §4 names `PrintForm.test.tsx`) and `proposal.md`'s Impact says `FieldForm` is untouched without distinguishing source from test. The plan should say so, not the diff.

**5.** Task 5.2 requires "say in the task record which of 5.1's cases were already covered and which this added." `tasks.md` carries `[x]` and no record. The coverage does exist (`Import.test.tsx:751-753` and `:806-810` both assert `data.tags` is undefined in the batch body; `LabelGrid.test.tsx:329`; `Connect.test.tsx:519`), so nothing needed adding — but the box claims a record that was never written.

**6.** Nit: `ParamInput.tsx:66` has no dependency array and clears `pendingFocusRef` only when it runs. A consumer whose `onChange` does not re-render `ParamInput` leaves the pending focus armed until some later unrelated render, which then steals focus. Unreachable through `FieldForm` today, which always updates state; worth a guard if `ParamInput` gains a second consumer.

VERDICT: REVISE
