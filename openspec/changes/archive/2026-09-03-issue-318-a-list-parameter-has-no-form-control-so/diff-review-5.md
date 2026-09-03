TREE_SHA256: 94a35b5737e90804f89a9abb6eebff405a7099aa9067358a9944d532d9bf9521

# Diff review — issue-318 (list parameter form control), round 5

**Scope.** The five modified files against `proposal.md`, `design.md`, `tasks.md`, `specs/template-inputs/spec.md`, issue #318's acceptance criteria, and AGENTS.md. No repo file was edited; the mutation trees live in `/tmp/repro318b` and `/tmp/repro318c`, outside the repository. The branch has no commits beyond `origin/main` (`git log origin/main..HEAD` is empty), so the working tree is the whole change.

## What I verified as correct

- **Round 4's blocking finding is genuinely fixed, not merely re-tested.** The `queueMicrotask` clear is gone; `ParamInput.tsx:66-79` is a dependency-less layout effect that clears the ref inside its own body. I rebuilt round 4's repro in `/tmp/repro318c` — a test file that imports no RTL, sets `IS_REACT_ACT_ENVIRONMENT = false`, renders with `createRoot` and drives a native `dispatchEvent(new MouseEvent("click"))` — and all three focus rules hold: move-earlier lands on `move tags 1 earlier`, removal lands on `remove tags 2`, removing the only row lands on `add tags`. [verified]
- **The focus tests can now fail.** Six mutations against a scratch copy, each run over `ParamInput.test.tsx` + `PrintForm.test.tsx` + `FieldForm.test.tsx` (77 tests, all green unmutated) [verified]: no-op the layout effect → 4 failures; move-earlier focus to `idx` instead of `idx - 1` → 2; collapse the three removal branches to `index: idx` → 2; drop `aria-disabled` → 3; natively `disabled` the inert controls → 4; filter empty strings on append → 6; ignore the `value` prop → 17. Round 1's two seeding mutations still bite exactly one test each: deleting `initialFieldState`'s list branch (`PrintForm.tsx:32-33`) fails *carries empty array in the very first list request*, reverting the return guard (`:63`) fails *submits empty array for list entry arriving in a later list*.
- **Gates**: `npm run test` 49 files / 463 tests pass, `npm run lint` clean, `npm run build` (tsc + vite) clean, `cargo fmt --check` clean. No Rust file is in the diff, so `clippy` and `cargo test` cannot regress. [verified]
- **Scope holds.** `git status --porcelain` lists only the five in-scope files plus the untracked change folder. `Import.tsx`, `LabelGrid.tsx`, `Connect.tsx` and `FieldForm.tsx` source are unedited, and their tests are unedited. [verified]
- **The plan gate passes.** `.workflow/specs-digest.sh` returns `fe2c1c9d2002…`, matching `review.md:24`; `review-gate-check.sh --plan-only` exits 0. Both cut issues, #351 and #352, are OPEN. Both `MODIFIED` requirement names match `openspec/specs/template-inputs/spec.md:12` and `:787`. [verified]
- **The delta drops nothing.** I diffed both delta requirement bodies against the published ones: every change is an addition or a stated correction (the tolerate paragraph, the `422 BatchInvalid` envelope, the `text`-narrowing of *A broken default is shown as a diagnostic*). No published prose or scenario is silently lost, which `archive-merge-check.sh` would not have caught. [verified]
- Rounds 1–4's non-blocking items are addressed: `disabled:opacity-50` is on all four button classes; `expect(screen.getByText("CONSUMABLE"))` pins the checkbox naming its default; the inert controls are now proved focusable through `.focus()` + `document.activeElement`, not through `not.toBeDisabled()`; `tasks.md` 3.9 names `FieldForm.test.tsx` and 5.2 carries its record.
- Reordering, removal and append arithmetic match the delta clause for clause, no array is mutated in place (`input.default` reaches `data` by reference through `FieldForm.tsx:51`, so this matters), and every issue #318 acceptance criterion has a test.

## Findings — none blocking

**1. Non-blocking. A 96-line test block is deleted with no task authorizing it, and one assertion has no replacement.**

`describe("PrintForm list parameter tolerance")` (`HEAD:ui/src/pages/print/PrintForm.test.tsx:828-926`) is removed outright. It had to change — it asserted `queryByLabelText("tags")` is null and `body.data` equals `{ title }`, both false now — but `tasks.md` §4 and `proposal.md`'s Impact both say the file *gains* cases, and neither names a deletion. Its one assertion with no successor is that Print stays **disabled** while a required `text` entry is empty and a `list` entry is present. No new test pairs a list entry with an unfilled required entry: the closest, *sends empty array in list request … retains value across branch switches*, fills `tier` before submitting. The general required-gating rule is covered by list-free tests, so this is a thin loss, but it is a deletion the plan does not record.

**2. Non-blocking. The exemption deletion at `PrintForm.tsx:127` is unobservable, which the proposal calls BREAKING.**

Re-adding `if (input.control === "list") return true;` above `:127` passes all 77 tests. [verified] That is correct and expected — the entry now always holds `[]`, so the exemption and its absence agree on every reachable state — but `proposal.md:20-22` labels it "**BREAKING (UI behaviour)**". Nothing about the shipped behaviour breaks; the deletion is a simplification. Worth knowing when reading the archived proposal, not worth an edit.

**3. Non-blocking. Two list entries sharing a description get identically-named groups.**

`ParamInput.tsx:284` names the editor group `spec.description || name`, so `tags` and `codes` both described `"Values"` produce two groups called `Values`. `spec.md:329-332` requires the *controls* to carry the entry's `name` and says only that the editor "SHALL be a group named for the entry"; every control does carry it, and `tasks.md` 1.5 prescribes exactly this spelling, so the contract is met. The scenario at `spec.md:818-823` asserts controls and does not reach the group.

**4. Non-blocking, recorded not re-raised. `pendingFocusRef` stays armed if a consumer's `onChange` leaves the value unchanged.**

`ParamInput.tsx:66-79` clears the ref only when the effect body runs. A consumer that drops an `onChange` leaves the pending target set until some later unrelated render, which then places focus. Unreachable through `FieldForm`, the only consumer. Raised as a nit in round 1 and round 3; round 4's attempted fix for it was the regression, and reverting it was right.

**5. Nit.** `ParamInput.tsx:320` and `:346` read `if (isEarlierInert || isFirst) return;` where `isEarlierInert` is `!disabled && isFirst`, so the disjunction is `isFirst`. Harmless; the second clause is the one doing the work.

VERDICT: APPROVE
