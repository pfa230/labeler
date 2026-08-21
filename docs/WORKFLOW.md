# How changes get made

What happens between asking for a change and it landing on `main`, and the one point where you are
asked for something.

The mechanics, commands, and enforcement details are in [`AGENTS.md`](../AGENTS.md); they are written
for the agent, not for you.

## Asking for a change

Describe what you want. That is the whole interface.

Everything after that runs without you: filing the issue, isolating the work, writing the spec and
design, getting them reviewed, implementing, reviewing the implementation, updating the specs of
record, and merging.

## What you get back

A change lands as **one commit** containing the code, an ADR recording the decision, the updated
specs, and the archived planning record. It closes its issue on push.

The planning record is kept, not discarded. For any change you can read what was proposed, what the
design was, what the reviewer objected to, and how that was resolved.

## What the review guarantees

Every change is reviewed twice, and the reviews are adversarial by construction rather than by good
intentions:

- **The plan is reviewed before any code is written.** Spec and design together, so a design that
  contradicts its own spec is catchable.
- **The implementation is reviewed after.**

Whoever wrote something never reviews it. Different models take the two roles, so a review is never
an author being asked to find fault with itself. The reviewer works from the files alone, with no
access to the conversation that produced them, and cannot edit anything it reviews.

A review ends in one of three verdicts. `APPROVE` proceeds. `APPROVE WITH CHANGES` lists specific
required edits, which are applied and re-checked before proceeding. `REVISE` means a fundamental
defect: the plan is fixed and reviewed again from scratch.

Some things the process refuses to let slide:

- A blocking finding cannot coexist with approval.
- Changing the plan after it was approved voids the approval.
- The author cannot dismiss a serious finding on its own say-so; the reviewer has to accept the
  rebuttal.
- If the reviewer is unavailable or produces nothing usable, the change stops. It does not fall back
  to reviewing itself, and it does not invent a verdict.

## When you are asked for something

Once, and only when the loop fails: **the plan was revised twice and still did not pass review.**

At that point the change stops. Nothing is implemented. You get the review and the artifacts.

What is wanted from you is direction, not proofreading. The reviewer has already checked correctness,
scope, and edge cases more thoroughly than a read-through would. The questions it cannot answer are
yours: is this solving the right problem, is the scope right, should it be dropped.

## What is not guaranteed

Worth knowing, so the process is not trusted further than it earns:

- The review gate is enforced for work done by Claude Code. Work done by another agent is not yet
  blocked by it. A CI check that applies regardless of which tool made the edit is tracked in #188.
- Specs live in two places during migration. `docs/SPEC.md` is frozen and still authoritative for
  behavior that has not moved yet; `openspec/specs/` holds everything since. A spec in the new
  location names the frozen section it replaces, so the answer to "which one is current" is written
  down rather than inferred.

## Where things live

| | |
| --- | --- |
| Current behavior, frozen baseline | `docs/SPEC.md` |
| Current behavior, everything since | `openspec/specs/` |
| Decisions and their rationale | `docs/adr/` |
| Planning record for past changes | `openspec/changes/archive/` |
| How to write a template | `docs/AUTHORING.md` |
| Rules the agent follows | `AGENTS.md` |
