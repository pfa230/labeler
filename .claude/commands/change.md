---
description: Run one accepted issue end to end on four named agents
argument-hint: <issue#> <planner> <plan-reviewer> <implementer> <code-reviewer>
allowed-tools: Bash, Read, Write, AskUserQuestion
---

Scope the issue with the user, then hand it to `.workflow/run-change.sh` and relay what it asks.

## 1. Scope it

Read the issue: `gh issue view $1 --json title,body -q '"# " + .title + "\n\n" + .body'`. The
`--json` is not optional. Bare `gh issue view` asks for the deprecated `projectCards` field and exits 1
on the `gh` Ubuntu ships, printing no body at all, which stops the stage before it starts (#303). A
`--json` view requests only the fields it names, so it never asks. This is also the line
`run-change.sh:248` runs to cache the scope, so the command and the driver read the issue alike.

Ask the user about anything underspecified that would change the contract, the scope or the acceptance
criteria. Small details you can decide, decide, and record them in the issue rather than asking.

Write the refined scope back with `gh issue edit $1 --body-file <file>`. That body is literally what
the planner is given: the driver writes it to `.agent-runs/issue-$1.md` in the worktree and the propose
stage is told to read it first. A vague issue produces a vague plan, and this is the only point where
that is cheap to fix.

**This is the only interactive stage.** Everything after it runs unattended.

## 2. Launch it

Detached, never as a background task this session holds: one such run was killed 4.3 seconds after
its turn ended, taking 15,127 lines of review with it (#275).

```bash
run=$(.workflow/detach.sh /tmp/run-change-$1 .workflow/run-change.sh $ARGUMENTS)
.workflow/detach.sh --wait "$run"
```

The launch prints a handle on stdout, which is the log file and what `--wait` takes; every launch gets
its own. `detach.sh` puts the run in its own session (`setsid`, else `python3`'s `os.setsid()`, else `nohup`,
which is only SIGHUP-proof and says so). `--wait` is how you learn the outcome, never the process
existing: it gives you the run's exit status, or tells you its deadline passed. The
launch prints the handle either way and uses its exit status to say whether the run was seen to
start, so a non-zero launch is worth checking before you wait on it. Do not hand-roll the launch. The line this replaced named `setsid` and `timeout`,
macOS ships neither, and backgrounding hid the resulting 127 (#284).

Then wait on it. Report only the `== stage ==` lines, the per-stage status lines and the tails. Do
**NOT** read the agent logs under `.agent-runs/` in full: they run to thousands of lines, and keeping
them out of this context is why the script writes them to files. Read a targeted range to diagnose a
failure, nothing more.

## 3. Relay its questions

Exit **8** means a stage wrote `QUESTIONS.md` at the worktree root and stopped rather than guess. Read
it, put the questions to the user with `AskUserQuestion`, write the answers to `ANSWERS.md` beside it,
and launch the same command again. The script resumes from the artifacts; every stage reads
`ANSWERS.md`, so the stage that asked gets its answer.

Nothing else needs re-running by hand. The other stops want a person to look, not a relay.

## Exit codes

- **0** green: committed, pushed, and the branch run passed. It prints the merge commands; run them
  once the user approves. Nothing has reached `main`.
- **8** a stage asked something. Relay it, as above.
- **6** a loop hit its round cap. Surface the findings and stop; three rounds that will not converge
  want a person, not a fourth.
- **9** the branch run was red or never appeared. Do not merge either way.
- **10** the fix round left the tree byte-identical to the one the previous round judged, so no
  second verdict on the same bytes was launched. Surface the round file it names and stop: either
  every finding was answered in prose, which only a person can accept, or none was acted on.
- **1** a stage failed. **2** bad arguments, or the worktree is on another issue's branch.
- **3, 4, 5, 7** from `apply.sh`; see `.claude/commands/apply.md`.

## What it owns, and what it does not

It owns the worktree, the plan and its review, the implementation and its review, the archive, the
three cargo gates, the commit and the push. It never merges to `main`, and it never edits
`docs/SPEC.md`.

Do not step in between its stages, and do not review any of its output yourself: this session scoped
the issue, so reviewing what came of that is self-review, and the four names exist precisely so that
separation does not depend on remembering. If you find yourself about to, say so instead.
