TREE_SHA256: bf373925b8a74c7b82961bebd4a6b613d4eac9939e88dd5d210639e42b72c697
SPECS_SHA256: 7698ec7595c1f76a0d1c7a2c95a0ec102ffb7e97e7a038419220f7ac3acd83b1

## Review: issue-385, grid columns from `inputs.all`

**Scope.** Working-tree diff (`ui/src/pages/{Connect,Import}.{tsx,test.tsx}`, +864/-37) against `proposal.md`, `design.md`, `tasks.md`, the delta at `specs/template-inputs/spec.md`, the published `openspec/specs/template-inputs/spec.md`, and AGENTS.md. Branch sits at `origin/main`; the change is uncommitted. No ANSWERS.md at the worktree root.

### What holds

- Both grids' column sets are row-independent: `Connect.tsx:203` reuses `templateFields` (`:176`, a memo over `detail.inputs.all`), `Import.tsx:111-128` unions CSV headers with `inputs.all` and filters `listNames`. `cellInput` tries the row's own list first and falls back to `inputs.all` (`Connect.tsx:205-208`, `Import.tsx:129-134`). That is D1 and D2 as written. [verified]
- `validateRow` and every `pruneDataForSubmit` call site still read `getRowInputs` (`Connect.tsx:210-234,251,300`; `Import.tsx:136-162,181,247`), so D3's "no row valid today becomes invalid, no request changes" is inherited rather than asserted. [verified]
- The `inputs.all ⊇ per-row list` assumption the whole change rests on is structural: with `resolved_data: None` the walk skips no subtree and enters each `repeat:` once with the same `repeated_names` filter (`src/templates.rs:377-405`). No column can vanish. [verified]
- Only `Connect.tsx` and `Import.tsx` import `LabelGrid`, so no third grid is left on the old rule. [verified]
- The three `MODIFIED` blocks restate the published requirements without dropping a scenario or altering unrelated normative text (diffed block by block against `openspec/specs/template-inputs/spec.md`). `openspec validate --changes --strict` passes, and `review-gate-check.sh --probe .` now exits 0, so diff-review-2's blocking finding is resolved. [verified]
- Gates I ran myself: `ui` lint and test green (51 files, 576 tests), `npm run build` green in a scratch copy, `cargo fmt --check` green, and `git diff --stat` limited to the four files. [verified]
- diff-review-2's finding 4 does not survive: I probed the grid editor with a synchronous `queryByLabelText` immediately after `fireEvent.doubleClick` and it is already mounted, so 3.5/3.6's synchronous negative queries do discriminate. [verified by probe]

### Findings

**1. [BLOCKING] `proposal.md` understates what the delta changes.** `proposal.md:39` says "`template-inputs`: two requirements change" and enumerates two. The delta modifies three, and the third carries substantive normative rewriting: the batch grid now "shows the column and renders the cell read-only" for a `list` entry (`specs/template-inputs/spec.md:113-116`), a mapped array "SHALL submit it whenever its own input list reports that name, unchanged and unflattened" (`:129-131`), and both ordering scenarios' mechanism clauses change (`:214-222`). `design.md:110` already says "three whole requirements", so the proposal is the stale artifact, and it is the one the archive keeps as the record of scope. The fix touches no file under `specs/`, so it cannot void the plan review (`specs-digest.sh:7-16` digests `specs/` only).

**2. [non-blocking] A rewritten scenario publishes a citation pointing at unrelated code.** `specs/template-inputs/spec.md:221` anchors "The Connect grid preserves input-list order" at `ui/src/pages/Connect.tsx:153`, which is `schema={schema}` inside the `<ConnectorBrowser>` JSX; the column derivation is `:176`/`:203`. Its Import twin was deliberately refreshed to `Import.tsx:124` and is correct. Raised as finding 2 of `diff-review-2.md` and not addressed.

**3. [non-blocking] Two of the ten new tests cannot fail against the pre-change pages.** I restored `HEAD`'s `Connect.tsx` and `Import.tsx` into a scratch copy of `ui/` and ran both suites: 8 of the 10 new tests fail, and the 73 pre-existing ones pass. The two that pass either way are Connect 3.4 (`Connect.test.tsx:2470`) and Import 3.9 (`Import.test.tsx:912`), because their fixtures make `inputs.all` identical to every row's list, so nothing distinguishes the old per-row union from the new column set. They are legitimate regression guards for two restated ordering scenarios, but their boxes sit under `tasks.md:31-33`, which says every test in that section must fail first. The defect is in the task text, which specifies fixtures that make the assertion unfalsifiable, not in the code.

**4. [non-blocking] The newly normative "read-only list cell" has an in-flight hole.** `specs/template-inputs/spec.md:113-116` now states as contract that the batch grid renders a `list` cell read-only, and `:552-555` requires columns to be drawn before any row's list arrives. `Connect.tsx:206` returns `{ name: field, control: "text" }` whenever `getRowInputs(row.id)` is undefined, which it is until the POST resolves (`labelInputs.ts:164,215`), and `LabelGrid.tsx:261-262` then hands that cell a text editor. In that window a mapped array can be overwritten with a string, against "SHALL hold it unaltered while that list does not" (`:130-131`). The code path is pre-existing and untouched by this diff; what is new is the SHALL describing it. [verified by code path; I did not build a timing test]

**5. [nit] `requiredUnion` is now a misnomer on the page that kept it.** `Import.tsx:111` names a plain map over `detail.inputs.all`: no union, nothing required-only. Connect's twin was collapsed into `templateFields`, so one change leaves two spellings of the same idea. Same nit as `diff-review-1.md` finding 2, fixed on one page.

Finding 1 alone forbids landing. Findings 2, 3 and 5 are cheap while the folder is open; finding 4 is a judgment call for the driver, since closing it means editing `cellInput`'s pending fallback, which is outside this change's stated non-goals.

VERDICT: REVISE
