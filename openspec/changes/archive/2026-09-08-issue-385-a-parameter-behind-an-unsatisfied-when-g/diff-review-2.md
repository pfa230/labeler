TREE_SHA256: 637c65ba91d50853bb8ca77f78516030a5e806645dcc6894cb4f94a0e2baf367
SPECS_SHA256: 865c877bb72cb948c95257b50fcc78deffc2f4078e03289d466f963e1ea4525f

## Review: issue-385, batch grid columns from `inputs.all`

**Scope reviewed.** Working-tree diff (`ui/src/pages/{Connect,Import}.{tsx,test.tsx}`, +676/-36) against `proposal.md`, `design.md`, `tasks.md`, `specs/template-inputs/spec.md`, the published `openspec/specs/template-inputs/spec.md`, and AGENTS.md. Branch is at `origin/main`; the change is uncommitted.

### What holds up

- `displayedFields` on both pages is now row-independent (`Connect.tsx:203` reusing `templateFields` at `:176`; `Import.tsx:111-128`), and `cellInput` falls back row-list-first then `inputs.all` (`Connect.tsx:206-208`, `Import.tsx:130-135`). That is D1 and D2 exactly, and it satisfies the delta's "SHALL draw its columns before any row's list has arrived, and SHALL NOT add or drop one when a list does arrive". [verified]
- `validateRow` and both `pruneDataForSubmit` call sites still read `getRowInputs` (`Connect.tsx:211-216,251,300`; `Import.tsx:140-141,181,247`), so D3's "no row valid today becomes invalid, no request changes" is inherited rather than asserted. [verified]
- The `all ⊇ per-row` assumption the new column set rests on is structural: `derive_inputs_internal` with `resolved_data: None` skips no subtree and walks each `repeat:` once with the same `repeated_names` filter (`src/templates.rs:376-406`), and both derivations emit in `self.params` order (`:415`). No column can vanish. [verified]
- Only two grids exist (`useBatchRowInputs` has exactly two callers), so nothing is left behind on the old rule. [verified]
- The three `MODIFIED` blocks diff minimally and coherently against the published spec: no unrelated normative text is dropped or altered. `openspec validate --changes --strict` passes. [verified]
- The eight new tests are red against the pre-change pages by construction (each turns on a column or an editor the old per-row union could not produce), and 3.3/3.5 assert submitted `data` on both sides of the gate rather than only the rendering. [verified by deriving old `requiredUnion` per fixture]
- Gates: `ui` lint, test (574/574), build all pass; `cargo fmt --check`, `clippy --all-targets --all-features`, `cargo test` all pass. `git diff --stat` touches the four files and nothing else. Tasks 4.1-4.3 and 5.1-5.2 are earned. [verified]

One note on noise: my first full `ui` run showed `Import.test.tsx:145` failing on a `waitFor` timeout. It ran concurrently with `npm run lint` on the same box; the file in isolation and a clean full run both pass. I attribute it to CPU contention I caused, not to this diff. [verified by re-run]

### Findings

**1. [BLOCKING] The spec delta grew after the plan verdict, and the commit gate refuses the change.**

`review.md` records `SPECS_SHA256: bfd8d60a2d03...`. The current delta digests to `865c877bb72c...`:

```
$ ./tools/openspec-loop/workflow/review-gate-check.sh --probe .
review gate: change 'issue-385-...': specs/ has changed since the verdict
(recorded bfd8d60a2d03, now 865c877bb72c). A change to the contract voids the
review; re-run it in a fresh context.
EXIT=1
```

The cause is traceable. `diff-review-1.md` finding 1 required adding `An input list describes the controls one label needs` to the delta as `MODIFIED`; it states plainly "That requirement is not in the delta". It is in the delta now (`specs/template-inputs/spec.md:5`), so the contract acquired a whole restated requirement after the plan review approved a different one. `review-gate-check.sh:165-171` runs from `.githooks/pre-commit` and again in CI, and this commit touches `ui/src`, so the in-flight branch of the gate applies. The commit cannot land.

This is not only mechanical. `review.md` still reads `VERDICT: APPROVE_WITH_CHANGES` / `CHANGES_APPLIED: yes` over specs that are no longer the specs, which is a verdict claiming coverage it does not have. Note that `specs-digest.sh --write` is not the fix; that script's own header calls re-running it to clear a stale verdict "laundering". Which remedy applies (re-run the plan review in fresh context, or record the extension with an honest note) is the driver's call, not mine.

**2. [non-blocking] A rewritten scenario anchors a new claim to unrelated code.**

`specs/template-inputs/spec.md:212-215`, "The Connect grid preserves input-list order ... (`ui/src/pages/Connect.tsx:153`)". Line 153 is `schema={schema}` inside the `<ConnectorBrowser>` JSX, nothing to do with columns. The sibling Import scenario's citation was deliberately refreshed to `Import.tsx:124` (design D7 names that refresh), which is correct and points at `displayedFields`. The Connect anchor should be `Connect.tsx:203` or `:176`. The staleness predates this change (866078c moved the file), but the delta rewrites that scenario's THEN clause and fixes its twin, so it publishes a wrong anchor under a new normative sentence.

**3. [non-blocking] The change makes a test comment's citation stale in a file it edits.**

`ui/src/pages/Import.test.tsx:799` cites `(Import.tsx:154)` for the `if (input.control === "list") continue` guard. On `origin/main` that guard was at 154 exactly; this diff removes 12 lines above it, so it now sits at `Import.tsx:142`.

**4. [non-blocking, rigor] 3.5's read-only claim rests on a synchronous negative query.**

`Connect.test.tsx:2602-2603` doubleClicks the cell and then calls `queryByLabelText("edit tags")` synchronously, while every positive editor assertion in the same file awaits `findByLabelText`. If the editor mounts a tick later the assertion passes whatever the cell's editability. It matches the house idiom (`LabelGrid.test.tsx:83-84,184-185`) and the underlying contract is independently held by `LabelGrid.tsx:261`, so the exposure is bounded, but the new claim "a mapped list behind an unsatisfied gate is read-only" is the one leaning on it. [assumption: I did not prove the editor mounts asynchronously; proving it requires adding a probe test, which this review may not write.]

Finding 1 alone forbids landing. Findings 2-4 are cheap to fix while the delta is open anyway.

VERDICT: REVISE
