# Plan review

AUTHOR: claude
REVIEWER: opencode
VERDICT: APPROVE
ROUNDS: 3

Plan review requested — inspecting the proposal against the spec.Deep scan underway — checking spec deltas against the existing contract.Review of `proposal.md`, `specs/template-inputs/spec.md`, `design.md` against `issue-318` body, `openspec/specs/template-inputs/spec.md`, `openspec/specs/datetime-params/spec.md`, `openspec/specs/list-params/spec.md`, `AGENTS.md`, `openspec/config.yaml` [verified]:

**Prior REVISE findings — verified fixed:**
1. Print-form scoping: delta now scopes every new SHALL to the print form `specs/template-inputs/spec.md:281-285`, `spec.md:287-293`, `spec.md:335-336`, `spec.md:510-513`, with grid retention explicit `spec.md:98-110`, `spec.md:235-241`, `spec.md:842-846` and `proposal.md:17-28`. Design confines seeding to `PrintForm.tsx` private `initialFieldState`/`withArrivals` `design.md:44-68`, leaving `pruneDataForSubmit` unchanged `design.md:54-61`. Contradiction at `review-1.md:1` resolved.
2. Batch error envelope: delta now states `422 BatchInvalid` with `details.failures` carrying `MissingField` `spec.md:107-110`, `spec.md:236-241`, consistent with `spec.md:522-523` and `docs/SPEC.md` error model. `review-1.md:2` resolved.
3. Scope exceed: `proposal.md:47-59` lists only `template-inputs` as Modified Capability, `proposal.md:64-75` touches only `ParamInput.tsx`/`PrintForm.tsx`, cut items explicitly assigned to `#351`/`#352` with issues verified open `proposal.md:30-39`, `design.md:39`. `config.yaml:49` rule satisfied.
4. Disabled vs focus: spec requires inert controls report unavailable but remain in focus order `spec.md:311-319`, deferred case natively disabled `spec.md:335-336`, design chooses `aria-disabled="true"` staying focusable `design.md:74-96`, scenario covers boundary focus retention `spec.md:811-816`. Implementation verifies: `ParamInput.tsx:278-360` uses `aria-disabled` when `!disabled && isFirst/isLast`, `disabled={disabled}` only for deferral, handlers return early at boundary, `pendingFocusRef` moves focus to new row `ParamInput.tsx:310-330`. `review-1.md:4` resolved.

**No new blocking issues. Minor observations (non-blocking):**
- `proposal.md:47-59` lists `template-inputs` twice as separate Modified Capability items for the two requirements `An input list...` and `A screen renders...`. Capability is the path `template-inputs`; listing it once with both requirements is the canonical form. Editorial duplication, no contract effect.
- `proposal.md:33-34`/`specs/template-inputs/spec.md:342-346` deferral checkbox names the published default as text. For `list` that text is `String(value)` (`A,B` for `["A","B"]`), indistinguishable from a single element holding a comma. Proposal correctly cuts to `#351` (verified open) and spec does not prescribe a join spelling, so the SHALL is vacuously satisfied but leaves the rendering undefined until `#351` lands. Acceptable as scoped cut.
- `specs/template-inputs/spec.md:329-330` requires accessible name contain entry `name` and element position but does not state 1-based; `tasks.md:1.5` and implementation `ParamInput.tsx:290` use `idx+1`. Adding "1-based" to the SHALL would match the tested contract with no behavior change.
- Stale cell `openspec/specs/datetime-params/spec.md:311` still reads "`list` control (#318 builds the editor; until it lands a screen reports the entry and draws nothing)". After this lands that parenthesis is false for the print form. Proposal notes this and assigns correction to `#352` (verified open) and issue `#318` scope explicitly says delta lands in `template-inputs` only, so temporary inaccuracy is tracked rather than silently shipped. No spec drift beyond the filed follow-up.

Spec uses `MODIFIED` correctly for two existing requirements `specs/template-inputs/spec.md:3`, `:243`, names match `openspec/specs/template-inputs/spec.md:12`, `:787`. Full post-change contract is carried, `config.yaml:59-72` satisfied. Design documents decisions over alternatives `design.md:43-130` and risks `design.md:132-150`. Tasks cover gates `tasks.md:54-60` and are execution-only.

SPECS_SHA256: fe2c1c9d2002f20b0f86e0d672e14de31d813886a4d5d1e5bc722fe812580ad9
