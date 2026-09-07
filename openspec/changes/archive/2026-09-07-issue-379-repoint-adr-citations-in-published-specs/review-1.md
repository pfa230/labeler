Reviewing 379 repoint ADR plan against AGENTS.md and config — findings only, no edits.Deep verification of the six repointed citations is complete — now checking structural compliance.Findings against `proposal.md`, `specs/*/spec.md` deltas and `design.md`, verified on disk.

**1. Acceptance check regex misses two of the six citations** `[verified]`
`design.md:103-108` acceptance check 1 is:

`grep -rnoE "ADR-0(0[6-9][0-9]|[1-9][0-9]{2})" openspec/specs/`

That matches ADR-0060-0099 and ADR-0100+, but not ADR-0058 or ADR-0059. Per `proposal.md:18-20` and verified in published specs, two of the six sites are `layout-sizing/spec.md:743` `ADR-0059` and `layout-sizing/spec.md:993` `ADR-0058`. `openspec/specs/layout-sizing/spec.md:743,993` confirmed. `python` count shows `layout-sizing` has three `>=058` hits `['ADR-0059','ADR-0084','ADR-0058']`. A run of the check as written would return zero even if `ADR-0058/0059` remained, so #378 would still leave dangling references undetected. The correct predicate for “ADR above 0057” is `ADR-00(5[89]|[6-9][0-9])` or equivalent.

**2. Proposal understates edits per requirement** `[verified]`
`proposal.md:40-41` says “the edit inside each is one pointer”. The table `proposal.md:15-20` and the published file show `layout-sizing/spec.md:743` (`ADR-0059`) and `layout-sizing/spec.md:781` (`ADR-0084`) both lie in the single requirement `Text is laid out against the box it will get...` (`openspec/specs/layout-sizing/spec.md:690`). The delta `specs/layout-sizing/spec.md` indeed replaces both in that one requirement (`specs/layout-sizing/spec.md:57` `2026-08-21-issue-180...` and `:97` `2026-08-28-issue-245...`). So one of the five requirements carries two substitutions, not one. The six edits across five requirements are correctly stated in `design.md:8,25` but the proposal sentence is inaccurate.

**3. “Keeps its wording” contradicts the dropped §6 anchor** `[verified]`
`proposal.md:11` says “every sentence around a repointed pointer keeps its wording.” At `colour-vocabulary/spec.md:164` the published sentence is `This requirement supersedes the divergence ADR-0092 §6 recorded as intentional...`. The delta `specs/colour-vocabulary/spec.md:19` is `This requirement supersedes the divergence `openspec/changes/archive/2026-08-31-issue-280-shape-paint-model` recorded as intentional...` — `§6` is dropped. `design.md:83-87` correctly acknowledges “The `§6` anchor at `colour-vocabulary:164` is lost.” The proposal’s absolute wording guarantee therefore conflicts with the delta and with the design’s own risk acceptance.

**Other checks passed** `[verified]`
- Six sites and line numbers in `proposal.md:15-20` verified against `openspec/specs/flow-layout/spec.md:339,559`, `layout-sizing/spec.md:743,781,993`, `colour-vocabulary/spec.md:164`.
- All five archive folders exist (`openspec/changes/archive/2026-08-27-issue-226-unify-size-resolution` etc.).
- Deltas are `MODIFIED` blocks reproducing complete requirements and, after normalizing the six folder-for-ADR swaps (backticks around folder paths account for the only remaining token differences), are word-identical to published requirements. `DELIVERABLE: spec-only` is correctly placed at `proposal.md:7`, capabilities listed match the three files archive rewrites, no `src/`/`docs/adr` edits, and `openspec validate --all --strict --no-interactive` passes.

### Required changes

1. Fix `design.md:103-108` acceptance check 1 to catch ADR-0058 and ADR-0059. Replace the grep with e.g. `grep -rnoE "ADR-00(5[89]|[6-9][0-9])|ADR-0[1-9][0-9]{2}" openspec/specs/` or `grep -rnoE "ADR-0(05[89]|0[6-9][0-9]|[1-9][0-9]{2})" openspec/specs/` and note it must catch the six repointed ADRs (0058, 0059, 0082×2, 0084, 0092) while still ignoring ADR-0045, ADR-0050, etc. that intentionally remain.

2. Correct `proposal.md:40-41` wording from “the edit inside each is one pointer” to reflect reality, e.g. “the edits are six pointer swaps across five requirements (one requirement carries two).”

3. Soften `proposal.md:11` from “every sentence around a repointed pointer keeps its wording” to acknowledge the intentional loss, e.g. “every sentence keeps its wording except the `§6` anchor at `colour-vocabulary:164`, which has no folder equivalent and is dropped as accepted in design.md Risks.” Alternatively add the exception there.

Author applies the three edits above; NO further review follows.

VERDICT: APPROVE_WITH_CHANGES
