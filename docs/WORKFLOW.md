# How changes get made

This document describes the path a behavior change takes from issue to `main`, and the single point
at which it stops for a human decision.

The mechanics — commands, tooling, enforcement — are in [`AGENTS.md`](../AGENTS.md).

## The input

A change starts as a GitHub issue. Issues and milestones are the only backlog, and one change
implements exactly one open issue: scope is agreed there, before anything else happens.

From there the work runs in three stages — plan, implement, archive — each started by one command and
each running to completion without supervision: isolating the work, writing the spec and design,
getting them reviewed, implementing, reviewing the implementation, updating the specs of record, and
merging. The merge commit closes the issue.

The stage boundaries are deliberate. Planning stops before implementation so the spec and design are
settled, and reviewed, while changing them is still cheap.

Work discovered mid-change that falls outside its issue becomes a new issue. It does not widen the
one in flight, and it is never parked as an unchecked task.

## Running it

File the issue, then drive the stages. Use the slash commands explicitly — a plain-English request
like "implement #181" is unreliable, because it matches the *apply* skill ("Implement tasks from an
OpenSpec change") rather than propose, and would try to apply a change that does not exist yet.

```bash
gh issue create --title "Duplicate template id should not be fatal" --body "..."
```

```
/opsx:propose issue #181
```

Writes the proposal, the delta specs and the design, then **stops**. Planning does not run on into
implementation in the same turn, by design: the artifacts are meant to be settled before code exists.
The adversarial review of that plan runs next, on a different model, and gates what follows.

```
/opsx:apply
```

Refused until the review passes. Runs the task list, then the implementation is reviewed in turn.

```
/opsx:archive
```

Folds the delta specs into `openspec/specs/`, archives the planning record, and the change is
committed and merged.

Also available: `/opsx:explore` for thinking something through before an issue exists, and
`/opsx:update` for revising a change's plan in place after a review asks for edits.

### Which agent runs which stage

Any of the four can take the propose or the apply step. Roles are interchangeable; the only fixed
constraint is that the reviewer is not the author.

The stage commands differ per agent, because OpenSpec writes a separate command set for each and not
every tool reads the same one. Two forms exist: the **workflow** form `/opsx-*` and the **skill** form
`/openspec-*`.

| Agent | Stage commands | Notes |
| --- | --- | --- |
| Claude Code | `/opsx:propose`, `/opsx:apply`, `/opsx:archive` | Colon-separated. From `.claude/commands/opsx/`. |
| `agy` (Antigravity) | `/openspec-propose`, `/openspec-apply-change`, `/openspec-archive-change` | **Skill form.** The CLI reads `.agent/skills/` and ignores `.agent/workflows/`, so `/opsx-apply` is not a command despite what OpenSpec's tool table implies. |
| `codex` | none needed | Used for reviews via `codex exec`, with the instructions piped in. |
| `opencode` | `/opsx-apply` and friends, from `.opencode/commands/` | Unverified. |

Implementation and its review run on two named agents, given when the stage is started rather than
decided later:

```bash
.workflow/apply.sh issue-186-pin-rust-toolchain agy codex
```

The first agent implements, the second reviews, and naming the same one twice is refused. The script
owns the loop: findings return to the implementer, which resumes its session, and the reviewer
re-checks, for up to three rounds. A change that has not converged by then stops for a person rather
than looping; the reviewer's verdict is a `VERDICT:` line, so the decision to loop is read rather than
inferred from prose. Findings go
back to the implementer, which keeps the session it built in, and the reviewer re-checks; the reviewer
never fixes what it found. Transcripts stay in logs rather than being read back, since a full agent
transcript is thousands of lines of no interest to anyone.

Its commits are gated the same as anyone's — `core.hooksPath` resolves inside a worktree, so
`.githooks/pre-commit` runs. What it cannot see is the Claude Code edit-time hook, so its only gate is
at commit time.

Stages hand off through **files on disk, not conversation**. Every artifact's instructions re-read
their dependencies from disk, so a stage can run in a fresh session, a different agent, or the same
session as the last one. Fresh context is required in exactly one place: the review, which must not
run in the context that wrote the artifacts.

The gate applies to all of them equally. It is a committed git pre-commit hook plus the same check in
CI, judging files rather than which agent produced them. Enable it once per clone:

```bash
.workflow/setup-hooks.sh
```

### Checking on a change

```bash
openspec list                              # changes in flight
openspec status --change issue-181-...     # which artifacts are done, what is blocked
openspec show issue-181-...                # read a change
openspec view                              # interactive dashboard of specs and changes
```

`openspec status` is the one that answers "why has this not moved": it prints each artifact and what
it is blocked by.

### Prerequisites

`openspec` (1.9.0, pinned — the committed schema is generated by it), `gh`, `codex` for reviews, and
`agy` for implementation. The repository already carries the schema, config, and review hook.

```bash
openspec doctor                            # confirms the repo's OpenSpec root is sound
```

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

An artifact is never reviewed by whoever wrote it. `review.md` records `AUTHOR:` and `REVIEWER:`, and
the gate refuses a change where they match, so the rule is checked rather than trusted.

The reviewer runs read-only and cannot write files, so its stdout is redirected straight into
`review.md`: that file is its output, not a summary of it. Nothing transcribes the review, which keeps
the interested party out of the record and avoids pulling a thousand-line transcript through the
author's context to copy something already on disk. The reviewer
works from the files alone, without access to the conversation that produced them, and cannot edit
what it reviews.

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

Stages are started by hand, but only one thing stops the work and needs a *decision*: a plan revised
three times that still has not passed review.

The change halts with nothing implemented, and the review and artifacts are surfaced for a decision.

The decision needed there is direction, not proofreading. Correctness, scope, and edge cases have
already been examined more thoroughly than a read-through would manage. What remains unanswerable by
a reviewer is whether the change solves the right problem, whether its scope is right, and whether it
is worth doing at all.

## What is not guaranteed

- The pre-commit hook is skippable with `git commit --no-verify`. CI runs the identical check on what
  lands, so a skipped hook delays the refusal rather than avoiding it.
- There are no pull requests, so a change is checked by pushing its branch, which runs the validation
  jobs without publishing anything. Merging on a red or absent branch run puts the failure on `main`,
  where CI becomes a post-mortem rather than a gate.
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
