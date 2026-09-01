- **MAJOR — The plan falsely claims no caller can trigger the packed-flow failure.** The delta acknowledges that a caller activates a self-named gate by supplying the parameter’s own name (`specs/template-inputs/spec.md:74-76`, `:139-140`). A caller can therefore activate both sibling gates by supplying both self-named values. This contradicts the claims that the template renders for “every caller” because “no caller satisfies both gates at once” (`:86-87`, `:394-395`), repeated in `proposal.md:14-15` and `design.md:12-13`. The scenario title at `specs/template-inputs/spec.md:134` likewise says “by nobody else” while its own outcome says a caller can reach it.

- **MINOR — The proposal misstates the scenario changes.** `proposal.md:32-35` promises two new self-named-gate scenarios, but the delta adds three: thumbnail activation (`specs/template-inputs/spec.md:134-140`), thumbnail overrun (`:142-148`), and preview overrun (`:450-456`). The planning record should accurately describe the reviewed contract.

### Required changes

1. In `proposal.md:14-15`, `design.md:12-13`, and both delta paragraphs at `specs/template-inputs/spec.md:81-91` and `:391-397`, replace the universal “every caller”/“no caller” claim with the precise rule: callers using ordinary values render, while a caller that deliberately supplies every involved parameter’s own name activates all gates and reproduces the same overrun.
2. Rename the scenario at `specs/template-inputs/spec.md:134` so it states that the placeholder activates the gate and a caller can activate it only by supplying the same-name value.
3. Revise `proposal.md:32-35` to enumerate the three added scenarios: one thumbnail activation/reachability scenario and one packed-overrun scenario in each modified requirement.

The author applies these changes, and no further review follows.

VERDICT: APPROVE_WITH_CHANGES
