TREE_SHA256: 877c13c50f3f004763e987dd0e2be523b808a6311ed849cd2682e48840c67978
SPECS_SHA256: 7698ec7595c1f76a0d1c7a2c95a0ec102ffb7e97e7a038419220f7ac3acd83b1

# Diff review: issue-385, batch grid columns from `inputs.all`

**Scope.** Working-tree diff (`ui/src/pages/{Connect,Import}.{tsx,test.tsx}`, +867/−40) against `proposal.md`, `design.md`, `tasks.md`, the delta at `specs/template-inputs/spec.md`, the published `openspec/specs/template-inputs/spec.md`, and AGENTS.md. No `ANSWERS.md` at the worktree root. I edited nothing; every experiment ran in scratch copies under `/tmp`, since removed.

## What holds

- Both column sets are row-independent and match D1/D2 exactly: `Connect.tsx:203` reuses `templateFields` (`:176`), `Import.tsx:111-128` unions CSV headers with `inputs.all` and filters `listNames`; `cellInput` tries the row's list first, then `inputs.all` (`Connect.tsx:205-208`, `Import.tsx:129-134`). [verified]
- D3 is inherited, not asserted: `validateRow` and all four `pruneDataForSubmit` call sites still read `getRowInputs` (`Connect.tsx:213,251,300`; `Import.tsx:141,181,247`). [verified]
- **Red-run earned.** I copied `ui/` to a scratch dir, restored `HEAD`'s two pages, and ran both suites: 8 of the 10 new tests fail (3.1, 3.2, 3.3, 3.5, 3.6, 3.7, 3.8, 3.10), 75 pre-existing pass. 3.4 and 3.9 pass either way, which `tasks.md:31-33` now declares as regression guards. [verified]
- Gates on the branch as it stands: `npm run lint`, `npm run test` (51 files, 576 tests), `npm run build` all exit 0. `git diff --stat` touches the four files only (task 4.3). [verified]
- Every prior diff-review finding I could check is closed: `proposal.md:39` now reads "three requirements change"; `requiredUnion` is renamed on both pages; the `Import.tsx:154` test comment is refreshed to `:142`; the file-wide `HTMLAnchorElement` spy is gone; `review-gate-check.sh --probe .` exits 0. [verified]
- D7's premise is true, so the "historical heading" exception is proven, not asserted: a scratch openspec root shows `--strict` erroring with *"MODIFIED … omits scenario(s) the current spec still has"*. [verified]
- diff-review-3's finding 4 is **narrowed** by this diff rather than left open: `getRowInputs` falls back to `detail.inputs.default` while a row's list is in flight (`labelInputs.ts:233`), so `cellInput`'s `!inputs` branch fires only when `inputs.default` is empty; otherwise the new `inputs.all` fallback returns the `list` entry and `LabelGrid.tsx:261` withholds the editor. [verified]

## Findings

**1. [BLOCKING] `main` has moved and the branch is not rebased, so the delta restates a superseded requirement and would delete 13 published scenarios on archive.**

`HEAD` is `866078c`; `main` and `origin/main` are `7b424f4` ("Respect reported controls for inline editing in the batch grid", #271), and `HEAD` is its ancestor. That commit rewrote the very requirement this delta restates: `openspec/specs/template-inputs/spec.md:890` gained a grid-editor block at `:1068-1123` plus 13 scenarios. The delta was written against the pre-`7b424f4` text.

Proven mechanically, not inferred: with `main`'s `openspec/specs/` and this change folder in a scratch openspec root,

```
✗ [ERROR] template-inputs/spec.md: MODIFIED "A screen renders the reported inputs and
  decides nothing else" omits scenario(s) the current spec still has: "A select cell
  offers the values the entry declares and nothing else", … 13 names … Copy them into
  the MODIFIED block (a MODIFIED requirement replaces the whole block, so archive
  refuses to drop them).
```

It is not a copy-forward either. `main`'s `spec.md:1112` reads *"Three rules already stated take precedence, in this order: **a cell for a name absent from the row's list is inert**; …"* — that is the exact rule this change removes, so the #271 prose needs re-authoring against the new contract, not just re-pasting. Regenerating the delta will also void `review.md`'s `SPECS_SHA256`, which is the plan-review gate diff-review-2 already caught once.

AGENTS.md is explicit: *"Rebase before the diff review wherever `main` has already moved, so the tree the reviewer approved is the tree that lands."*

The code side is clean, which bounds the work: I extracted `main`'s `ui/` and dropped in this change's four page files. 51 files, **590/590 tests pass**, including `main`'s new 618-line `LabelGrid.test.tsx`. So the rebase is a spec-delta job, not a code job. [verified]

**2. [non-blocking] A rewritten scenario publishes a citation pointing at unrelated code.** Delta `specs/template-inputs/spec.md:221` anchors "The Connect grid preserves input-list order" at `ui/src/pages/Connect.tsx:153`, which is `schema={schema}` inside the `<ConnectorBrowser>` JSX; the column derivation is `:176`/`:203`. Its Import twin at `:216` was deliberately refreshed to `Import.tsx:124` and is correct. Raised as diff-review-2 finding 2 and diff-review-3 finding 2, and still unaddressed while the delta is open anyway.

**3. [non-blocking] Both new test blocks are filed under unrelated describes.** `Connect.test.tsx:2165`'s `describe("issue-385: …")` is nested inside `describe("8.6 Field transforms scenarios at their new timing")` (`:1863`), and the four new Import tests (`Import.test.tsx:817,878,929,1006`) sit inside `describe("CSV Import screen: datetime parameters")` (`:494`). Vitest prints the full path, so a failure reports this change's tests under field transforms or datetime parameters, which is where the next reader will not look.

Finding 1 alone forbids landing; findings 2 and 3 are cheap to fix in the same pass.

VERDICT: REVISE
