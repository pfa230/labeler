---
name: openspec-loop
description: The gated adversarial-review loop. Use when running or explaining /change and /apply, when a stage stops with a question, or when a review gate refuses a commit. Covers the stage order, who reviews whom, and what each gate checks.
---

# The loop

`proposal → specs → design → review → tasks → apply`. Order matters: the review gates
implementation, and archive rewrites the main specs after it.

Every change is one GitHub issue, one git worktree, one commit. A worktree rather than a branch,
because a branch shares one working directory and sessions here run concurrently.

## The guarantee

Nobody reviews their own plan, and nobody reviews their own code. Four agents are named up front
because the pairing *is* the guarantee, and `run-change.sh` refuses a lineup that breaks it before
launching anything rather than after four agent runs.

A reviewer never edits. Its only output is findings, which go back to whoever wrote the thing.
A reviewer that fixes what it found has produced a delta nobody reviewed.

## Running it

`/change <issue#>` scopes the issue with you, then runs every stage unattended to a green branch
run and stops there. The merge is the one step a person approves. `/apply` runs implementation and
diff review as a pair.

Agent names come from `.workflow/roles.local`, gitignored, all four or none.

## When a stage cannot decide

It writes `QUESTIONS.md` at the worktree root and stops rather than guess. Answers go in
`ANSWERS.md` beside it. The bar is a contradiction in what the stage was given, or a missing
decision that changes the contract. Anything a stage can decide, it decides.

## The gates

`review-gate-check.sh` judges a change at two points: landing, when its folder moves into
`archive/`, and in flight, when a commit touches implementation paths. It checks that the plan
verdict passed with author and reviewer differing, that `specs/` still matches the digest that
verdict recorded, and that the diff review passed with its reviewer in no author position.

`archive-merge-check.sh` checks that the published specs are the delta applied to the previous
commit: every requirement the delta names landed or is gone, and every requirement it does not
name is untouched.

Both are run by the git hooks and by CI, so no agent is judged differently from another. They
inspect files, never which tool produced them.
