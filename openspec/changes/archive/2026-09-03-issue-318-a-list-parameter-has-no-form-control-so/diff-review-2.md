TREE_SHA256: f236b51ef74836c27a83014132e4af6552804cd60b902da85deb7de02f1a5603

I reviewed the diff, the artifacts, and ran the gates and mutation probes myself. Here is what I found.

## What I verified as correct

- **Both round-1 seeding findings are now genuinely protected.** I copied `ui/` to a scratch tree and mutated each edit:
  - **Mutation A** — revert `PrintForm.tsx:61` to `return deferred === value.deferred ? value : …`: `submits empty array for list entry arriving in a later list without defaults without touching editor` **fails** (1 failed / 25 passed). [verified]
  - **Mutation B** — delete `initialFieldState`'s list seeding (`PrintForm.tsx:31-32`): `carries empty array in the very first list request for untouched undefaulted entry` **fails** (1 failed / 25 passed). [verified]
- **`ui/src/app/toast.tsx` is reverted** — `git status --porcelain` lists only the five in-scope files plus the untracked change folder. [verified]
- **Both cut items are filed** — `gh issue view 351/352` both return OPEN with the recorded titles. `proposal.md:32-39` references them. [verified]
- **Gates**: `npm run test` 49 files / 456 tests pass, `npm run lint` clean, `npm run build` clean. No Rust file is touched. [verified]
- **The behaviour the delta's untested scenarios describe actually holds.** I wrote four integration probes against the real `PrintForm` (focus after a move, move-then-submit ordering, `[""]` submission, focus after a removal); all four pass. [verified]
- The `withArrivals` return-guard extension, the deletion of `PrintForm.tsx:115`, and the editor's inert/focus/naming/deferral rules match `spec.md:301-345` clause for clause.

## Findings

### 1. BLOCKING — `specs/` was edited after the approving plan review, and the review gate refuses the commit

`.workflow/specs-digest.sh` on the change folder returns `fe2c1c9d2002…`; `review.md:22` records `SPECS_SHA256: 31704571a79b…`. Running the gate the hook and CI both call:

```
$ .workflow/review-gate-check.sh --plan-only "$PWD" ui/src/components/ParamInput.tsx ui/src/pages/print/PrintForm.tsx
review gate: change 'issue-318-…': specs/ has changed since the verdict (recorded 31704571a79b,
now fe2c1c9d2002). A change to the contract voids the review; re-run it in a fresh context.
EXIT=1
```
[verified]

The cause is in the implement stage's own record: `.agent-runs/implement-agy.log` — "Added corresponding scenarios to `specs/template-inputs/spec.md`". Those are `specs/template-inputs/spec.md:831` (`An undefaulted list entry appearing later arrives holding the empty list`) and `:837` (`The initial list request carries the empty list for an undefaulted list entry`). File mtimes agree: `spec.md` 05:23:14, `review.md` 04:55:16, `diff-review-1.md` 05:19:51.

AGENTS.md is explicit — "Editing `specs/` afterwards voids the verdict, and the gate detects it" — and `run-change.sh:420,436` writes the digest only at the plan-review APPROVE point, so nothing downstream will reconcile it. The two scenarios are good additions; the problem is that they are contract, added after the contract was signed off, by the agent whose code they describe. The remedy is a fresh full plan review over the amended `specs/`, not re-running `specs-digest.sh --write`, which AGENTS.md names as laundering a stale verdict. This alone forbids APPROVE.

### 2. Non-blocking — three delta scenarios are asserted one layer below what they state

`spec.md:775`, `:793` and `:799` each say what the **submitted `data`** carries. The only tests are `ParamInput.test.tsx:348` (`moves and removes elements in row order`) and the `[""]` assertion in the append test, both of which assert `onChange` arguments; no `PrintForm.test.tsx` case submits after a move, a removal, or with an empty row. My probes confirm the behaviour is right, so this is coverage, not a defect — but it is the shape AGENTS.md's apply guidance calls out ("a task saying to add an HTTP test is not satisfied by a unit test one layer below").

### 3. Non-blocking — the one-element editor's two inert move controls have no test

`spec.md:311` states "A one-element editor therefore carries two inert move controls rather than none." `ParamInput.tsx:292-293` gets this right (`isFirst` and `isLast` are both true at length 1), but test 3.4 uses three elements and 3.8's single-row case is `disabled`, so nothing exercises the stated consequence.

### 4. Non-blocking — a test name claims keyboard where the event is a click

`ParamInput.test.tsx:441` — "…after moving second element earlier **by keyboard**" — drives the move with `fireEvent.click`. It does call `.focus()` first, so it tests the thing that matters; the name overstates what happened.

### 5. Non-blocking — `withArrivals`'s defaulted branch was restructured, not just extended

The old shape ran `if (hasOwnKey(deferred, name)) continue;` before touching `data`; `PrintForm.tsx:45-53` now seeds `data` even when `deferred` already holds the key. I looked for a state where `deferred` holds a key `data` lacks and could not reach one — nothing deletes from `data`, and `initialFieldState`, `withArrivals` and `toggleDeferred` all write both — so it is behaviour-preserving today. But it is a widening of an existing path that no task names and no test pins, inside the one function this change was warned about.

VERDICT: REVISE
