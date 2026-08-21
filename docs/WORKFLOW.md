# How changes get made

This document describes the path a behavior change takes from issue to `main`, and the single point
at which it stops for a human decision.

The mechanics — commands, tooling, enforcement — are in [`AGENTS.md`](../AGENTS.md).

## The input

A change starts as a GitHub issue. Issues and milestones are the only backlog, and one change
implements exactly one open issue: scope is agreed there, before anything else happens.

Everything after that runs unattended — isolating the work, writing the spec and design, getting them
reviewed, implementing, reviewing the implementation, updating the specs of record, merging — and the
merge commit closes the issue.

Work discovered mid-change that falls outside its issue becomes a new issue. It does not widen the
one in flight, and it is never parked as an unchecked task.

## The output

A change lands as **one commit** carrying the code, an ADR recording the decision, the updated specs,
and the archived planning record. It closes its issue on push.

The planning record is kept rather than discarded, so for any past change the proposal, the design,
the reviewer's objections, and their resolution remain readable.

## What review guarantees

Every change is reviewed twice, and the reviews are adversarial by construction rather than by
intent:

- **The plan is reviewed before any code exists.** Spec and design are judged together, so a design
  contradicting its own spec is catchable.
- **The implementation is reviewed after it is written.**

An artifact is never reviewed by whoever wrote it. The two roles run on different models, so a review
is never an author asked to find fault with itself. The reviewer works from the files alone, without
access to the conversation that produced them, and cannot edit what it reviews.

A review ends in one of three verdicts. `APPROVE` proceeds. `APPROVE WITH CHANGES` lists specific
required edits, applied and re-checked before anything continues. `REVISE` marks a fundamental
defect: the plan is fixed and reviewed again from scratch.

Four things the process refuses to let slide:

- A blocking finding cannot coexist with approval.
- Altering the plan after approval voids that approval.
- A serious finding cannot be dismissed by the author alone; the reviewer must accept the rebuttal.
- A reviewer that is unavailable or produces nothing usable stops the change. It never degrades to
  self-review, and never substitutes an assumed verdict.

## Where it stops

Once, and only on failure: when a plan has been revised twice and still not passed review.

The change halts with nothing implemented, and the review and artifacts are surfaced for a decision.

The decision needed there is direction, not proofreading. Correctness, scope, and edge cases have
already been examined more thoroughly than a read-through would manage. What remains unanswerable by
a reviewer is whether the change solves the right problem, whether its scope is right, and whether it
is worth doing at all.

## What is not guaranteed

- The review gate is enforced for work done by Claude Code. Work done by another agent is not yet
  blocked by it. A tool-agnostic CI check is tracked in #188.
- Specs live in two places during migration. `docs/SPEC.md` is frozen and remains authoritative for
  behavior that has not moved; `openspec/specs/` holds everything since. A spec in the new location
  names the frozen section it replaces, so precedence is recorded rather than inferred.

## Where things live

| | |
| --- | --- |
| Current behavior, frozen baseline | `docs/SPEC.md` |
| Current behavior, everything since | `openspec/specs/` |
| Decisions and their rationale | `docs/adr/` |
| Planning records of past changes | `openspec/changes/archive/` |
| Template authoring guide | `docs/AUTHORING.md` |
| Rules the agent follows | `AGENTS.md` |
