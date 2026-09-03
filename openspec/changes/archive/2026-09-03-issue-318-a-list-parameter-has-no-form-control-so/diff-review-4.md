TREE_SHA256: ca43d377328bf9567af8030c2145e7aad2061ca3095df617df1c6d8aa1fb2c01

# Diff review — issue-318 (list parameter form control), round 4

**Scope.** The five modified files against `proposal.md`, `design.md`, `tasks.md`, `specs/template-inputs/spec.md`, issue #318's criteria, and AGENTS.md. No repo file was edited; the reproduction below lives in `/tmp/repro318` (a copy of `ui/`), outside the repository.

## What I verified as correct

- **Gates are green** [verified]: `npx vitest run` → 49 files / 460 tests passed; `npm run lint` clean; `npm run build` clean. The `window is not defined` uncaught error is from `src/app/toast.tsx`'s timer under `Import.test.tsx` and is untouched by this diff [assumption: pre-existing; none of the five changed files is on that path].
- **`git status --porcelain`** lists only the five in-scope files plus the untracked change folder. `Import.tsx`, `LabelGrid.tsx`, `Connect.tsx` and `FieldForm.tsx` source are unedited, as `proposal.md:64-78` promises. [verified]
- **Round 3's blocking finding is fixed.** `PrintForm.tsx:21-25` and `:37-40` now state the seeding exception where the code makes it, and `:57-59` comments the return-guard widening `design.md:70-72` called the change's one non-obvious edit. [verified]
- **The seeding is correct and load-bearing.** `initialFieldState` (`PrintForm.tsx:32-33`) and `withArrivals` (`:54-57`) seed `[]` only for an undefaulted `list`, and never into `deferred`; `hasOwnKey` is `hasOwnProperty` (`labelInputs.ts:8`), so `[]` counts as present and no render loop follows. The `valid` deletion at `:127` is safe because every path that puts a `list` entry into `inputs` also seeds it, and `inputsPending` gates the rest. [verified]
- **Reordering, removal and append arithmetic are right** (`ParamInput.tsx:335-340`, `:361-366`, `:391`, `:399`), no array is mutated in place, and the boundary handlers return before touching the array (`:333`, `:359`). Task 5.1's four citations check out (`LabelGrid.test.tsx:329`, `Import.test.tsx:688`, `:755`, `:751-753`, `Connect.test.tsx:509`). [verified]

## Findings

### 1. BLOCKING. The focus rules do not work in a browser; only `act()` makes them appear to.

`ui/src/components/ParamInput.tsx:72-77` clears `pendingFocusRef` from a `queueMicrotask` callback queued inside the click handler, *before* `onChange` is called. React 19 with `createRoot` schedules sync-lane work in its own microtask, queued after that one. So the clear runs first, and the layout effect at `:79-82` sees `null` and places no focus.

Failure scenario: the operator tabs to the second row's move-earlier control and presses Enter. The element moves to row 1, and focus is left on the button that is now "move tags 2 earlier" — a *different* element's control. Activating again moves the wrong element, which is precisely the failure `design.md:104-108` says the focus rule exists to prevent. Same for both removal cases: focus is on nothing at all (`document.activeElement` is `body`).

Verified, not reasoned. In `/tmp/repro318` I rendered the current component with `createRoot`, `IS_REACT_ACT_ENVIRONMENT = false`, and a native `dispatchEvent(new MouseEvent("click"))` — the browser's own scheduling:

```
× move-earlier places focus on the moved element's new row
    expected 'move tags 2 earlier' to be 'move tags 1 earlier'
× remove places focus on the removing control of the row that took its place
    expected null to be 'remove tags 2'
× removing the only row places focus on the appending control
    expected null to be 'add tags'
```

The move and the removal themselves land (`input[0].value === "B"`; row count drops to 2) — only the focus is dropped. Deleting the `queueMicrotask` block at `:72-77` and changing nothing else turns all three green. Causation is established, not inferred.

This violates `specs/template-inputs/spec.md:322-330` ("After a move, focus SHALL follow the moved element to the same control on its new row… After a removal, focus SHALL move to the removing control of the row that took the removed row's position, or of the preceding row…, or to the appending control…"), the scenario at `:811-816`, and tasks 1.6, 3.5 and 3.6.

Two things make this worse than an ordinary bug:

- **Every test covering it is act-bound and cannot fail.** `ParamInput.test.tsx`'s three focus cases and `PrintForm.test.tsx`'s reordering cases go through RTL's `fireEvent`, which wraps in `act()`; `act` flushes the commit synchronously, on the stack, before any microtask runs. The suite therefore reports green against the broken path. Task 4.7 asked for exactly this check ("so none of them is a test that cannot fail") and it did not reach this, because the mutation tested was the ref index, not the scheduling.
- **It is a regression introduced by the round-3 fix.** `diff-review-3.md` finding 3 quotes `ParamInput.tsx:66-80` as "the effect keys on `[value]`" and calls the stale-arming risk latent and unreachable through `FieldForm`. The current file has no dependency array and this microtask instead. A non-blocking nit was answered by disabling a shipped requirement.

The stale-arming concern that motivated it is still addressable — clearing the ref when the editor's `items` length or contents are the ones the pending target was computed against, or keying the effect on `value` while clearing unconditionally at the end of the effect — but whatever replaces it must be proven against a non-`act` render, because the existing suite cannot see the difference.

Incidentally, `typeof queueMicrotask === "function"` at `:72` guards an environment that does not exist here; it goes with the block.

### 2. Non-blocking. A scenario clause has no test.

`specs/template-inputs/spec.md:781-786` requires that for a defaulted list entry "the deferral checkbox is checked **and names that published default**". `PrintForm.test.tsx`'s "opens a defaulted list entry with one row…" asserts only `checkbox.checked`. The rendering does satisfy the clause for the scenario's own `["CONSUMABLE"]` (`String(["CONSUMABLE"])` is `CONSUMABLE`, `FieldForm.tsx:93`); it is the assertion that is missing, not the behaviour. The multi-element spelling is the deliberate cut to #351, so only the single-element case is pinnable here.

### 3. Non-blocking. "Reachable by keyboard" is asserted through a proxy.

`ParamInput.test.tsx`'s inert-control case checks `expect(firstEarlier).not.toBeDisabled()`. `spec.md:317-319` says the controls "SHALL NOT be made unfocusable", which `tabindex="-1"` would break while leaving `not.toBeDisabled()` green. Task 4.7 named this assertion as one of the two most likely to pass either way. `.focus()` then `document.activeElement` pins the actual clause. Relatedly, task 3.5 and the scenario at `:811-813` both say "by keyboard" and the test activates with `fireEvent.click`; that is behaviourally the same event a keyboard Enter produces, so it is a wording gap rather than a coverage one.

Finding 1 must be fixed before this lands.

VERDICT: REVISE
