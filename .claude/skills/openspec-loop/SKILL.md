---
name: openspec-loop
description: Supervise an openspec-loop run of a committed OpenSpec change. Use when the user asks to run, step, watch, stop or resume a change with the loop, or asks what a loop run is doing.
---

# Supervise an openspec-loop run

The loop runs a committed OpenSpec change unattended. You supervise it only through its five commands, exactly as a person at a terminal would:

- `loop run <change>` starts a run and advances it in the foreground. It prints a progress line, prefixed with the time, as each step starts and ends, then a report. It exits 0 when the run completes, 1 when it stops, and 2 when it refuses to start.
- `loop step <change>` enrolls a new run or continues an existing one through the first successful pending-stage completion, including its internal cycles and recovery handoffs. It exits 0 when it returns control between stages or completes the whole run, 1 when it stops and 2 when it refuses. It shares the existing state, preparation, lock, transcripts and configured attempt budgets; it accepts no options.
- `loop status <change>` shows the run's state, branch, stages and locations.
- `loop stop <change>` ends the current step at once; the active advancement then prints its stop report.
- `loop resume <change>` continues a run that is between stages, stopped or interrupted, with a fresh attempt budget; `loop resume <change> --attempts <n>` sets that budget, and `--implement` sends the run back to the implementer, which implements the plan as it stands; proposal review stays as it is, so an approved plan is not reviewed again. It advances in the foreground like `loop run`, with the same output and exit statuses.

## Rules

- Run every `loop` command outside your sandbox. The loop writes the repository's Git directory, which sandboxes make read-only, and it starts its own workers. In a Codex session, request escalated permissions for each `loop` command.
- Relay each progress line to the user as it appears, in order and unchanged. Do not batch them until the run ends.
- When the command exits, show the user the report unchanged, in a code block. On a stop, name the stopped stage, the cause and the recovery the report gives, say where the transcripts are, and diagnose the stop with the user as described under Diagnosing a stop.
- Run `loop stop` only when the user asks. Run `loop resume` or another `loop step` only after the user agrees, even when the report names it as the continuation or recovery. A successful stage boundary is an inspection point: show its report unchanged, identify what can run next and wait for the user without diagnosing a failure or automatically advancing.
- Change nothing to investigate a stop: do not edit the run's state, worktree or transcripts, and do not commit, resume or rerun anything to test a theory. Diagnose by reading only, as described under Diagnosing a stop. The one exception is answering a decision stop, below, and only with the user's agreement.

## Watching a run in Claude Code

1. Start the command with Bash and `run_in_background: true`, redirecting its output to a log file: `loop run <change> > <log> 2>&1`. Use `loop resume <change>` or `loop step <change>` the same way.
2. Start a Monitor on `tail -n +1 -f <log>` with the longest timeout. Each new line arrives as a notification. Relay each progress line, which starts with the time, and keep count of the lines relayed. The first line that does not start with the time ends the relay: relay neither it nor any line after it, because Monitor events drop each line's leading whitespace, and step 3 shows the rest from the log. If the monitor expires while the run continues, re-arm it with `tail -n +<count + 1> -f <log>`, so no line is skipped or repeated.
3. When Bash reports that the background command exited, stop the monitor, read the log with Bash from the first line not yet relayed to the end, and show the user all of those lines unchanged, in a code block, with the exit status.

## Watching a run in Codex

Verified with Codex CLI 0.160.0. Start the command with `exec_command` and a short `yield_time_ms` such as 1000. While the command runs, the call returns the lines printed so far and a session id. Poll that session with `write_stdin`, passing empty `chars`, and relay the new lines from each poll. The poll that returns the exit status also carries the report.

## Stopping and resuming

To stop, run `loop stop <change>` from any checkout of the repository; the watched advancement prints the stop report and exits 1. To continue after the user agrees, start `loop resume <change>` and watch it the same way. For one successful stage at a time, use `loop step <change>` and watch it the same way; the report names the stage completed and what can run next. Returning between stages is different from a stopped run and from whole-run completion. After resolving a stop, `step` follows the same recovery as `resume`, within its single-stage limit. If `loop status` shows the run as interrupted, the process advancing it died; `loop resume` takes the run over and redoes the interrupted step.

## Diagnosing a stop

The loop reports what it observed and investigates nothing; finding out why a run stopped is your work with the user. Use only reads: the report, `loop status`, `cat`, `tail` and `jq` on the files under the run directory, `git log` and `git diff` in the run's worktree, and `cat` on `openspec/loop.yaml` and `openspec/loop.local.yaml` in the checkout where you gave the `loop` command. The report names the worktree, the run directory and the transcripts directory.

1. **Classify the stop from the report's cause.** A stop by the user needs no diagnosis. A decision stop is answered as described below. A stop at integration or delivery for a dirty checkout, unpushed or diverged commits, or a rejected push carries its own recovery: explain it. Diagnose the rest as an execution failure or as exhausted attempts.
2. **An execution failure.** Read the end of the transcript the cause names, the reply file if there is one, and the usage on the step's end line. For a worker's failure, also read the entry under `agents` for the alias the cause names, from `openspec/loop.local.yaml` when it defines the alias and from `openspec/loop.yaml` otherwise, and check the command's arguments, such as the model, against the error.
   - Name the likely cause and quote the lines that show it, such as an authentication failure, a quota or rate limit (`401`, `402`, `429`), a network error, a permission the harness denied, a turn limit, a missing command, or a reply that does not match the schema.
   - Say what the evidence does not show. When nothing in the report, the transcript or the reply names an error, such as a transcript that ends without one, an empty transcript or a step killed by a signal, the cause is open: say so instead of guessing. Missing usage alone does not make an explicit error uncertain.
   - A failed `git` or `gh` command quotes its output in the cause. A refused `openspec archive` names only its transcript, which holds OpenSpec's explanation: read it. An execution failure of the loop itself has no transcript, and its cause is all the evidence there is.
3. **Exhausted attempts.** List the run's `*.verdict.json` and `*.findings.json` files in the transcripts directory in step order, and compare their findings from attempt to attempt:
   - The same finding repeats: the implementer or plan reviser does not address it, or cannot within the plan.
   - The findings change every attempt: the reviewer's demands move, or each fix breaks something new.
   - The same check failure repeats unchanged: it may fail on the base too, and the implementer must not repair what the change did not break. Suggest the user check the base.

   Then look at what each attempt changed. Each implementation review record names the `commit` it reviewed: `git diff` consecutive ones in the worktree. For proposal review, diff the plan between the `<change>: revise plan` commits that `git log` shows on the run's branch.
4. **Present and wait.** Give the likely cause with the files and lines that support it, what the evidence leaves open, and the recoveries that fit: `loop resume <change>` after the user fixes the environment, `loop resume <change> --attempts <n>`, a decision or plan edit committed on the run's branch and then resumed, or discarding the run as the README describes. When the user has edited the plan of a run whose implementation is complete, offer `loop resume <change> --implement`, which has the implementer implement the edited plan; a plain resume would continue where the run stopped. It is refused once the change is archived. Do nothing more until the user chooses.

## Answering a decision stop

When the report's cause says a review needs decisions the plan does not record, show the user each question exactly as the report lists it. The questions are the user's to answer; do not answer them yourself. When the user has answered, offer to record the answers. With the user's agreement, write each decision into the change's `design.md` in the run's worktree that the report's recovery names, and commit it there on the run's branch with `git commit`. Then resume with `loop resume <change>` once the user agrees, and watch the run as before. The resumed run reviews the plan again.
