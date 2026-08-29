#!/usr/bin/env bash
# Run one stage of a change on a named agent (#224).
#
#   .workflow/run-stage.sh <role> <agent> <change> [--resume] [extra prompt...]
#
# Exit 5 = the reviewer edited files. 7 = the review produced no structured result,
# so its log is a transcript rather than a review. 3 = implement changed nothing.
#
#   role   implement | review
#   agent  see .workflow/agents.sh
#
# The pairing is the point: an implementer and a reviewer that are different agents,
# expressed at dispatch rather than left to whoever remembers. /apply drives both.
#
# The agent's transcript goes to a log, never to stdout. Only the status, the files
# touched and a tail come back; a full transcript is thousands of lines and pulling
# one through the orchestrator is waste.
set -uo pipefail

role="${1:?role required: implement | review}"; shift
agent="${1:?agent required, e.g. agy}"; shift
change="${1:?change name required, e.g. issue-186-pin-rust-toolchain}"; shift || true

resume_requested=0
if [ "${1:-}" = "--resume" ]; then shift; resume_requested=1; fi
extra="$*"

case "$role" in implement|review) ;; *) echo "role must be implement or review: $role" >&2; exit 2 ;; esac

# Siblings are resolved beside this script, and .worktrees/ hangs off the main
# checkout rather than whichever worktree we were called from: --show-toplevel
# answers the latter, so it cannot locate either one (#264, same defect as #256).
here=$(cd "$(dirname "$0")" && pwd)
common=$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null) || { echo "not in a git repo" >&2; exit 2; }
root=$(dirname "$common")
. "$here/agents.sh"

agent_known "$agent" || { echo "unknown agent: $agent" >&2; exit 2; }
command -v "$agent" >/dev/null 2>&1 || { echo "$agent is not on PATH; nothing would run." >&2; exit 2; }

issue=$(printf '%s' "$change" | sed -n 's/^\(issue-[0-9]\{1,\}\).*/\1/p')
[ -n "$issue" ] || { echo "change name must start with issue-<N>-: $change" >&2; exit 2; }
wt="$root/.worktrees/$issue"
[ -d "$wt" ] || { echo "no worktree at $wt" >&2; exit 2; }
[ -d "$wt/openspec/changes/$change" ] || { echo "no change '$change' in $wt" >&2; exit 2; }

# Implementing past a failed plan review wastes the run; reviewing is always allowed.
# --plan-only because this fires before the diff review exists: demanding one here
# would refuse to start the very run that produces it.
if [ "$role" = "implement" ] && ! "$here/review-gate-check.sh" --plan-only "$wt" src/_probe >/dev/null 2>&1; then
  echo "review gate refuses this change; not starting:" >&2
  "$here/review-gate-check.sh" --plan-only "$wt" src/_probe 2>&1 >/dev/null | sed 's/^/  /' >&2
  exit 1
fi

# Every run artifact lands in one ignored directory, so a `git add -A` in the
# worktree cannot sweep a transcript into the change's commit (#255).
runs="$wt/.agent-runs"
mkdir -p "$runs"
log="$runs/$role-$agent.log"
raw="$runs/$role-$agent.json"
conv_file="$runs/$role-$agent.conversation"

resume=""
if [ "$resume_requested" -eq 1 ]; then
  [ -s "$conv_file" ] || { echo "--resume needs a previous run; no id at $conv_file" >&2; exit 2; }
  resume=$(cat "$conv_file")
fi

if [ "$role" = "implement" ]; then
  apply_step=$(agent_apply_prompt "$agent" "$change") || { echo "no apply prompt for $agent" >&2; exit 2; }
  base="$apply_step Stop when the tasks are implemented. Do not commit. Do not archive. Do not sync specs into openspec/specs/. Do not move or delete the change folder. Do not edit docs/SPEC.md, which is frozen. Check a task only after actually performing it."
  [ "$resume_requested" -eq 1 ] && base="Review findings on your implementation of $change. Fix each one, then stop. The same limits still hold: do not commit, archive, sync specs, move the change folder or edit docs/SPEC.md."
else
  # The verdict line is what lets apply.sh decide whether to loop. Without a
  # machine-readable answer the caller has to interpret prose, which is how a
  # review that found problems gets read as one that passed.
  base="Adversarially review the implementation diff for $change against its proposal, specs, design and tasks, and against AGENTS.md. Find real problems; do not rubber-stamp. Cite file:line evidence and verify each finding against the actual code before raising it. Report findings only: you must not edit any file. End your output with exactly one line, on its own line, reading either VERDICT: APPROVE or VERDICT: REVISE. Use REVISE if any finding must be fixed before this can land; any blocking finding forbids APPROVE."
fi
prompt="$base $extra"

# Only the implementer takes the lock: it is the one that must not commit, merge or
# push mid-run. A read-only reviewer has nothing to hold back.
lock=""
if [ "$role" = "implement" ]; then
  lock="$(cd "$wt" && git rev-parse --path-format=absolute --git-common-dir)/APPLY_IN_PROGRESS"
  [ -f "$lock" ] && { echo "a run is already in progress: $(cat "$lock")" >&2; exit 1; }
  printf '%s %s started %s (pid %s)\n' "$agent" "$change" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$$" > "$lock"
  trap 'rm -f "$lock"' EXIT INT TERM
fi

cmd=$(agent_command "$agent" "$role" "$prompt" "$resume") || { echo "no invocation for $agent/$role" >&2; exit 2; }
# What the worktree looks like right now, content included. The reviewer guard below
# needs a DELTA across the stage, not the absolute dirtiness of the tree: apply never
# commits, so the implementer's work is always uncommitted when the reviewer runs, and
# counting `git status` lines blamed the reviewer for the implementer's diff every time.
worktree_digest() {
  (
    cd "$wt" || exit
    git status --porcelain -- . ':!openspec/changes' ':!.agent-runs'
    git diff HEAD -- . ':!openspec/changes' ':!.agent-runs'
    git ls-files --others --exclude-standard -- . ':!openspec/changes' ':!.agent-runs' \
      | LC_ALL=C sort | tr '\n' '\0' | xargs -0 -r sha256sum
  ) 2>/dev/null | sha256sum | cut -d' ' -f1
}
before_digest=$(worktree_digest)

( cd "$wt" && pty_run "$cmd" ) 2>&1 | clean_capture > "$raw"
status=$?
after_digest=$(worktree_digest)

# How an answer is separated from a transcript is per-CLI knowledge, so it lives in
# agents.sh beside the invocation that produced it. Here only the outcome matters:
# either the agent's own answer is in $log, or $log is the console capture.
extracted=1
if ! agent_status=$(agent_extract "$agent" "$raw" "$log" "$conv_file"); then
  cp "$raw" "$log"
  agent_status="NO_STRUCTURED_RESULT"
  extracted=0
fi

changed=$(cd "$wt" && git status --porcelain -- . ':!openspec/changes' ':!.agent-runs' | wc -l | tr -d ' ')
echo "role: $role   agent: $agent   status: $agent_status   exit: $status"
echo "files touched: $changed"
echo "log: $log"
echo "--- last 30 lines ---"
tail -30 "$log"

# Without a structured result the log is the raw console capture, not the agent's
# answer. For a review that is not a small problem: the caller would read a verdict
# out of a transcript, hand the transcript to the implementer, and commit it as the
# review artifact (#264). Stop instead; a review that cannot be extracted did not
# happen. Keyed on extraction having failed for THIS agent rather than on one agent's
# envelope being absent: keyed the latter way, no agent but agy could pass a review it
# had actually written, and agy is the one agent with no read-only mode (#274).
if [ "$role" = "review" ] && [ "$extracted" -eq 0 ]; then
  echo >&2
  echo "no structured result from $agent, so $log is the raw transcript rather than the review." >&2
  echo "Refusing to treat a transcript as a review. The capture is at $raw." >&2
  exit 7
fi
# A reviewer that changed files has broken the rule it was told to follow. Judged by
# whether this stage altered the tree, not by whether the tree was already dirty.
if [ "$role" = "review" ] && [ "$before_digest" != "$after_digest" ]; then
  echo >&2; echo "the reviewer altered the worktree during its stage. Reviewers report; they do not edit." >&2
  exit 5
fi
# An implement run that exits cleanly having written nothing did not run.
if [ "$role" = "implement" ] && [ "$status" -eq 0 ] && [ "$changed" -eq 0 ]; then
  echo >&2; echo "implement produced no changes despite a clean exit. It did not run. See $raw" >&2
  exit 3
fi
exit $status
