# Diff review

AUTHORS: agy
REVIEWER: claude
VERDICT: APPROVE
ROUNDS: 6
TREE_SHA256: 80a1593450f60e20f02bc0d507183e574cfacf2195eb7771237a250528677f88
SPECS_SHA256: 293519ff909fdb11683ed5290171f67402782a3cfd089d23d58db0ca4578e267

# Diff review: issue-385, grid columns from `inputs.all`

**Scope.** Working-tree diff (`ui/src/pages/{Connect,Import}.{tsx,test.tsx}`, +878/−42) against `proposal.md`, `design.md`, `tasks.md`, the delta at `specs/template-inputs/spec.md`, the published `openspec/specs/template-inputs/spec.md`, and AGENTS.md. No `ANSWERS.md`. Branch is at `7b424f4` = `origin/main`, so no rebase is outstanding. I edited nothing in the worktree; every experiment ran in `/tmp` copies, since removed. This is round 6; diff-review-5's blocking finding 1 and non-blocking 2 and 3 are fixed (see below).

## What holds

- **Column sets are row-independent and match D1.** `Connect.tsx:390` feeds the grid the existing `templateFields` memo (`Connect.tsx:176`, `detail.inputs.all` names); `Import.tsx:111-128` unions CSV headers with `inputs.all` and filters `listNames`. Neither depends on `rows` or `getRowInputs`, so no column is added or dropped when a row's list arrives, which is what the delta requires at `specs/template-inputs/spec.md:493-495,303-305`. [verified]
- **D2's precedence is implemented in the stated order:** row's list first, `inputs.all` second, `{name, control:"text"}` fallback kept, `undefined` for a name in neither (`Connect.tsx:205-208`, `Import.tsx:130-135`). [verified]
- **D3 is inherited, not asserted.** `validateRow` and every `pruneDataForSubmit` site still read `getRowInputs` (`Connect.tsx:213,251,300`; `Import.tsx:140,181,247`), so no row valid today becomes invalid and no request changes. [verified]
- **`LabelGrid.tsx` and `labelInputs.ts` are untouched**, and `LabelGrid.test.tsx` passes unchanged (task 4.1). Connect and Import are the only `LabelGrid` consumers in `ui/src`, so no third grid was missed. [verified]
- **Red before green, run myself.** `HEAD`'s two pages restored into a scratch `ui/`: 8 of the 10 new tests fail (3.1, 3.2, 3.3, 3.5, 3.6, 3.7, 3.8, 3.10) and all 75 pre-existing tests in those two files pass. 3.4 and 3.9 pass either way, which `tasks.md:29-31` declares up front as regression guards. [verified by run]
- **Gates.** `npm run lint && npm run test && npm run build` green (51 files, 590 tests); `cargo fmt --check && cargo clippy --all-targets --all-features && cargo test` exit 0; `git diff --stat` touches the four files only (tasks 4.3, 5.1, 5.2 earned). `openspec validate --strict` passes and `.workflow/loop review-gate --probe .` exits 0. [verified]
- **The delta is faithful.** I diffed all three `MODIFIED` blocks line-by-line against the published spec: no scenario dropped, no unrelated normative text altered. Both occurrences of the old "union of the names across the rows present" rule (`openspec/specs/template-inputs/spec.md:1061,1143`) are inside the rewritten blocks, and no other capability states a grid column rule, so `conditional-visibility` correctly needs no delta. [verified]
- **D7's justification is true, not assumed.** In a sandbox copy I deleted the `A grid cell for a name inactive on its row is inert` scenario from the delta and re-ran strict validation: it errors with "MODIFIED ... omits scenario(s) the current spec still has". The historical heading with the inverted outcome plus its italic note is the only shape the tooling permits. [verified by run]
- **`review.md`'s digest is honest.** `SPECS_SHA256: 293519ff…` recomputes to exactly that value against today's delta, and the added "Decision on post-verdict delta updates" section records the extension rather than laundering it, which is what diff-review-5's finding 5 left to the driver. [verified]

## Findings

**1. [nit] `Connect.test.tsx` gains a stray blank line at EOF.** `HEAD`'s copy ends `});\n`; the working copy ends `});\n\n`. Raised as diff-review-5 finding 4 alongside three other cosmetics (the `displayedFields` alias, the over-indented `describe`, both now fixed) and left standing. Lint does not catch it.

**2. [nit] `tasks.md:71-75` (4.2) cites a correction that no test needed.** It names "`Connect.test.tsx`'s mapped-list test asserting the row is not refused" as inverted by the new contract, but the pre-existing mapped-list test (`Connect.test.tsx:1807`) declares no gate: its `inputs.all` equals its `inputs.default`, `validateRow` skips `list` controls, and the row is still not refused. Nothing was corrected, and nothing needed to be. The box is honest about the re-read; the example points at a test that does not exist in that form, and a later reader chasing it finds nothing.

**3. [nit] `Import.tsx:120-121` keeps a loop that can add nothing.** `listNames` unions `list` names from `inputs.all` and then from `inputs.default`. Since `default ⊆ all` structurally (`src/templates.rs:193-405`: the `resolved_data: None` walk skips no subtree), the second loop can never contribute a name. It is pre-existing, and the diff edited the line above it, so removing it is optional rather than owed.

None of the three changes what the code does or what the contract says. The blocking finding from the previous round is fixed: `Connect.test.tsx:2181-2189` now declares `orientation` `required: true` in both `inputs.all` and every per-row list, and asserts the rows *are* refused and `Download` disabled, scoping the non-refusal claim to `tags` and `location` exactly as the scenario at `specs/template-inputs/spec.md:744-750` does.

