TREE_SHA256: aa47ca231f36a46d699e8a841ce809555cbfb1860f268714ad00469f308d1b6f

## Diff review — issue-318 (list parameter form control)

**Scope of what I checked.** The five modified files against `proposal.md`, `design.md`, `tasks.md`, `specs/template-inputs/spec.md`, issue #318's acceptance criteria, and AGENTS.md.

### What I verified as correct

- **Gates are green** [verified]: `npm run lint` clean, `npx vitest run` 49 files / 460 tests pass, `npm run build` (tsc + vite) clean. No Rust file is touched, so the cargo gates cannot regress.
- **`git status --porcelain`** lists only the five in-scope files plus the untracked change folder. The out-of-scope `ui/src/app/toast.tsx` edit from round 1 is gone. `Import.tsx`, `LabelGrid.tsx`, `Connect.tsx` and `FieldForm.tsx` source are unedited, as `proposal.md` promises. [verified]
- **The plan gate passes**: `.workflow/specs-digest.sh` returns `fe2c1c9d2002…`, matching `review.md:24`, and `review-gate-check.sh --plan-only` exits 0. Round 2's blocking digest finding was resolved the way AGENTS.md requires, by a third full plan review (`review-3.md`, `VERDICT: APPROVE`), not by re-running `--write`. [verified]
- **Round 1's two unprotected seeding edits are now genuinely protected.** I re-ran both mutations in a scratch copy: reverting `PrintForm.tsx:58` to `deferred === value.deferred ? value : …` fails `submits empty array for list entry arriving in a later list without defaults`; deleting `initialFieldState`'s `else if (input.control === "list")` branch fails `carries empty array in the very first list request`. Each is 1 failed / 56 passed. [verified]
- **The editor's tests are load-bearing.** Four further mutations each fail 1 to 3 tests: dropping the inert early return (`:320`), sending post-move focus to the old index (`:321`), dropping `aria-disabled` (`:317`), and natively disabling the inert controls. [verified]
- Every scenario in the delta at `spec.md:762-846` has a test, and the arrays are never mutated in place (`:322-325`, `:376`, `:398`), which matters because `items` can be `input.default` by reference after `toggleDeferred` re-seeds it (`FieldForm.tsx:51`). [verified]
- Round 2's finding 5 was wrong: `withArrivals`'s defaulted branch still runs `if (hasOwnKey(deferred, …)) continue;` before touching `data` (`PrintForm.tsx:46`). Nothing widened. [verified]

### Findings

**1. BLOCKING. Two invariant comments in `PrintForm.tsx` now state the opposite of the code directly beneath them.**

`ui/src/pages/print/PrintForm.tsx:21-23` reads "An entry publishing none is absent from both maps, which is **not** the same as holding an empty value or a `false` deferral." Lines 31-33, eight lines below, now put an undefaulted `list` entry into `data` holding `[]`, which is exactly "holding an empty value".

`ui/src/pages/print/PrintForm.tsx:38-40` reads "An entry a later list brings in for the first time is seeded **and deferred** here, exactly as one present at first paint." Lines 52-56 seed such a `list` entry into `data` and deliberately leave `deferred` untouched, which `tasks.md:2.1` states as a requirement ("Do not put such an entry into `deferred`").

Failure scenario: a maintainer editing `withArrivals` reads line 39, concludes every arriving entry is deferred, and reintroduces the deferral for a list entry that has no default to defer to; or reads line 22 and concludes `data` never holds a value for an undefaulted entry, which is the precise belief the `valid` deletion at `:119-125` now depends on being false. Both comments are load-bearing statements of exactly the invariant this change bends, and AGENTS.md is explicit that "a surviving exception lives next to the rule it bends, with its proof attached, in the same contract, spec, or comment. Never in a footnote." Today the exception lives only in `design.md` and the delta.

Related, same fix: `design.md:70-72` calls the `withArrivals` return-guard widening at `:58` "the one non-obvious edit in the change", and it carries no comment, in a function whose every other non-obvious step has one (`:21-23`, `:38-40`, `:68-71`, `:86-88`, `:95-98`, `:102-104`).

**2. Non-blocking. A deferred list editor renders four full-contrast buttons that do nothing.**

`ParamInput.tsx:328`, `:354`, `:381` and `:397` set `disabled={disabled}` but omit `disabled:opacity-50`, and each pairs it with an inline `style={{ color: "var(--ink)" }}` (`:331`, `:357`, `:382`, `:398`). An author inline declaration outranks the UA `button:disabled { color: GrayText }` rule, so the UA's only disabled affordance is suppressed and nothing replaces it. [verified for the cascade; the rendered result is an assumption, since nothing here renders a browser.] `disabled:opacity-50` is the convention at 20-plus call sites including `PrintForm.tsx:14-15`'s own `buttonBase`; this change introduces the first `<button>` elements into `ParamInput`, whose other branches are native form controls the browser greys itself.

Spec conformance is unaffected: the controls are natively disabled and assistive technology is told. What an operator sees while `Use default` is checked is an editor that looks operable. Note the inert controls at `:332` and `:358` do carry `opacity: 0.4`, so the weaker state is signalled and the stronger one is not.

**3. Non-blocking. `pendingFocusRef` stays armed if a consumer's `onChange` leaves the `value` prop identical.**

`ParamInput.tsx:66-80` clears the ref only when the effect body runs, and the effect keys on `[value]`. A consumer that drops an `onChange` leaves the pending target set until some later unrelated `value` change, which then steals focus. Unreachable through `FieldForm`, the only consumer (`setData` always builds a new array), so this is latent, not a defect in the shipped path. Round 1 raised it as a nit and it was not addressed; recording it rather than re-raising it as new.

**4. Note, not a finding.** `spec.md:781-786` requires the deferral checkbox to name the published default, and the test at `PrintForm.test.tsx` asserts only `checkbox.checked`. That clause is the subject of the deliberate cut to #351 (`String(["A","B"])` renders `A,B`), so leaving it unpinned is consistent with the scope. Both cut issues, #351 and #352, are filed and OPEN. [verified]

Finding 1 is the only one that must be fixed before this lands, and it is a two-comment edit.

VERDICT: REVISE
