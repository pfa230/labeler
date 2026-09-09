# Plan review

AUTHOR: claude
REVIEWER: codex
VERDICT: APPROVE_WITH_CHANGES
ROUNDS: 2

## Required changes

1. **The mapped-list scenario permits submission with a missing required gate.** The [scenario at spec.md:995](/home/pfa/projects/labeler/.worktrees/issue-385/openspec/changes/issue-385-a-parameter-behind-an-unsatisfied-when-g/specs/template-inputs/spec.md:995) declares `orientation` without a default and supplies no value, yet says the row is not refused and submits data. The delta makes such an entry required at [spec.md:95](/home/pfa/projects/labeler/.worktrees/issue-385/openspec/changes/issue-385-a-parameter-behind-an-unsatisfied-when-g/specs/template-inputs/spec.md:95); gate keys remain reported even when their condition fails ([templates.rs:242](/home/pfa/projects/labeler/.worktrees/issue-385/src/templates.rs:242)). **Change the outcome to retain the read-only array, report no error for `tags`, and block submission for missing `orientation`. Add a subsequent step setting `orientation: vertical`, with both enum values explicitly declared, to verify a valid inactive row submits without `tags`. Keep the horizontal activation scenario verifying unchanged array submission.**

2. **The Import ordering scenarios require conflicting results.** [Spec.md:210](/home/pfa/projects/labeler/.worktrees/issue-385/openspec/changes/issue-385-a-parameter-behind-an-unsatisfied-when-g/specs/template-inputs/spec.md:210) requires columns in `title, subtitle, code` order without constraining CSV headers. The explicit rule at [spec.md:504](/home/pfa/projects/labeler/.worktrees/issue-385/openspec/changes/issue-385-a-parameter-behind-an-unsatisfied-when-g/specs/template-inputs/spec.md:504) preserves header order, and the scenario at line 697 requires `subtitle, title, code`. **Qualify the line-209 premise with a CSV whose headers are `title, subtitle`; retain the declaration-order validation assertion. In the line-699 premise, explicitly state that the layout reads all three parameters unconditionally, so this scenario and its following Connect scenario actually produce all three inputs.**

3. **The restated list contract still forbids the mapped arrays the plan preserves.** [Spec.md:113–125](/home/pfa/projects/labeler/.worktrees/issue-385/openspec/changes/issue-385-a-parameter-behind-an-unsatisfied-when-g/specs/template-inputs/spec.md:113) says connector mapping omits lists and grid rows cannot carry them. The same delta explicitly permits mapped arrays at [spec.md:351](/home/pfa/projects/labeler/.worktrees/issue-385/openspec/changes/issue-385-a-parameter-behind-an-unsatisfied-when-g/specs/template-inputs/spec.md:351). **Rewrite the former passage to distinguish Import’s omitted list column, Connect’s read-only list cell, and connector mapping’s supported multi-valued-to-list pairing. Restrict the missing-list failure rule to rows carrying no array and having no resolvable default; preserve submission of mapped arrays when the row’s input list reports them.**

The author applies these specific edits; **NO further review follows**.

## Decision on post-verdict delta updates and rebase integration

`SPECS_SHA256` was updated after the round-2 plan approval to incorporate:
1. The post-#271 fast-forward rebase to `origin/main` (`7b424f4`), which incorporated #271's control-aware grid editor block and resting cell contract (including 13 published scenarios) into `specs/template-inputs/spec.md`.
2. Diff review findings 2 and 3: accurately scoping inert cells to names neither the row's list nor `inputs.all` reports, and noting the tolerate rule's omission of list columns on grids that cannot draw them.

All delta additions beyond the three plan review required changes represent #271's published contract and these clarifications, preserving the approved design without altering the underlying contract.

CHANGES_APPLIED: yes
SPECS_SHA256: 293519ff909fdb11683ed5290171f67402782a3cfd089d23d58db0ca4578e267
