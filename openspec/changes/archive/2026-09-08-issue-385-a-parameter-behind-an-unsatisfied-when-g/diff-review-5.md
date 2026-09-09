TREE_SHA256: e8f9dfac4ef272a74c380d5d7594175172e6a944733ccd8b74c88bcbe07db040
SPECS_SHA256: 7c43ef29253f78f6653830c06e74ef9ce11112e935ddcc6c44d365d588f18772

## Diff review: issue-385, grid columns from `inputs.all`

**Scope.** Working-tree diff (`ui/src/pages/{Connect,Import}.{tsx,test.tsx}`, +870/−40) against `proposal.md`, `design.md`, `tasks.md`, the delta at `specs/template-inputs/spec.md`, the published `openspec/specs/template-inputs/spec.md`, and AGENTS.md. No `ANSWERS.md`. Branch sits at `7b424f4` (`origin/main`), so the post-#271 rebase diff-review-4 demanded has happened. I edited nothing in the worktree; every experiment ran in `/tmp` scratch copies, since removed.

### What holds

- Both column sets are row-independent and match D1/D2: `Connect.tsx:203` aliases `templateFields` (`:176`), `Import.tsx:111-128` unions CSV headers with `inputs.all` and filters `listNames`; `cellInput` tries the row's list first, then `inputs.all` (`Connect.tsx:205-208`, `Import.tsx:130-135`). [verified]
- D3 is inherited, not asserted: `validateRow` and all four `pruneDataForSubmit` sites still read `getRowInputs` (`Connect.tsx:213,251,300`; `Import.tsx:140,181,247`). [verified]
- `inputs.all ⊇ any per-row list` is structural, so no column can vanish: with `resolved_data: None` the walk skips no subtree and enters each `repeat:` once under the same `repeated_names` filter (`src/templates.rs:377-405`), and both derivations emit in `self.params` order. [verified]
- Typing garbage into a newly reachable cell cannot break row resolution: `template_inputs` (`src/api.rs:1302-1339`) never coerces or rejects `data`. [verified]
- **Red before green**, run myself: `HEAD`'s two pages restored into a scratch `ui/`, both suites run. 8 of 10 new tests fail (3.1, 3.2, 3.3, 3.5, 3.6, 3.7, 3.8, 3.10), pre-existing ones pass. 3.4 and 3.9 pass either way, which `tasks.md:29-31` now declares up front as regression guards. [verified]
- Gates: `npm run lint`, `npm run test` (51 files, 590 tests), `npm run build` all green; `cargo fmt --check`, `clippy --all-targets --all-features`, `cargo test` all exit 0; `git diff --stat` touches the four files only (tasks 4.1-4.3, 5.1-5.2 earned). `openspec validate --strict` passes and `.workflow/loop review-gate --probe .` exits 0. [verified]
- The three `MODIFIED` blocks drop no scenario and alter no unrelated normative text (diffed block by block against the published spec). [verified]

### Findings

**1. [BLOCKING] Test 3.1 proves its "refuses no row" claim only because its fixture publishes a response the service cannot produce.**

`Connect.test.tsx:2181-2189` stubs `orientation` in both `inputs.all` and every per-row list without `required`, while the scenario it implements (`specs/template-inputs/spec.md:744-750`) declares `orientation` as "an `enum` with no `default:`" and requirement 1 states that such an entry is `required: true` ("a `boolean`, an `enum` and a `datetime` declaring no default are each `required: true`"). The rows are materialized with `data: {}`, so under the real contract every row carries a required, empty `orientation`.

I patched only that fixture in a scratch copy, adding `required: true` to the four `orientation` entries, and re-ran: the column and editability assertions still pass, and `Connect.test.tsx:2253` (`download` enabled) fails with the button `disabled`. So the one assertion standing for "no row is refused" is disproved as soon as the fixture matches the contract. [verified by run]

The spec scenario is right, and 3.5 already models the same premise correctly with `required: true` and asserts the row *is* refused for `orientation`. It is 3.1 that overreaches: the scenario's THEN scopes non-refusal to `tags` and `location`, so the evidence should be the absence of an error against those two cells (as 3.5 does for `tags`), not an unblocked run. As written, `tasks.md:36-40`'s checked box claims a scenario nobody proved.

**2. [non-blocking] The re-authored precedence clause mis-scopes which cells are inert.**

`specs/template-inputs/spec.md:578-579` now reads "a cell for a column the template does not declare is inert". The published sentence it replaces said "a cell for a name absent from the row's list", which this change correctly had to re-scope, but the replacement is narrower than the behavior. A CSV header naming a parameter the template *declares and never reads* is reported by neither list, so `Import.tsx:130-135` returns `undefined` and `LabelGrid.tsx:471-474` renders the inert `—`. The template does declare it. The accurate scope is a name neither the row's list nor `inputs.all` reports.

**3. [non-blocking] The grid column rule omits the list carve-out its own scenario states.**

`specs/template-inputs/spec.md:320` and `:493-495` say a grid SHALL offer a column for every name `inputs.all` reports, with no exception, while the scenario at `:829-834` in the same requirement says the CSV import grid shows no `tags` column, and `Import.tsx:127` implements exactly that. The carve-out is stated in requirement 1 and in the CSV Import requirement, so the contract is determinate read whole, but the sentence a reader lands on first contradicts the scenario below it. One clause naming the tolerate rule at the point of the column rule closes it.

**4. [nit] Cosmetics in the new test block.** `Connect.test.tsx` now ends with a blank line after the final `});` (`\n\n` at EOF). The new top-level `describe` at `:2166` keeps the deeper indentation it had while nested, so its `beforeEach`/`it` bodies sit one level in from the file's other top-level describes. And `Connect.tsx:203`, `const displayedFields = templateFields;`, is a pure alias for one use at `:392`.

**5. [note, driver's call] The plan verdict's digest was re-recorded after the contract grew.** `review.md` records `SPECS_SHA256: 7c43ef29…`, which matches the delta today, so the gate passes. But diff-review-1 through -4 record `bfd8d60a`, `865c877b`, `7698ec75` and `7698ec75`, so `specs/` changed at least three times after that round-2 verdict, most recently for the post-rebase re-authoring of #271's block. `specs-digest.sh:13-16` says in its own header that `--write` can launder a stale verdict and that the visible edit to `review.md` is the only check. I bounded the exposure: everything in the delta beyond the plan review's own three required changes is #271's block copied forward unchanged, apart from the one sentence that is finding 2. Nothing here needs a re-review on content grounds; it needs a decision recorded rather than a digest quietly refreshed.

Finding 1 forbids landing. Findings 2 and 3 are cheap while the delta is open, and 2 is the sentence this change authored.

VERDICT: REVISE
