#!/usr/bin/env bash
# Run one stage of a change on a named agent (#224).
#
#   .workflow/run-stage.sh <role> <agent> <change> [--resume] [extra prompt...]
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

root=$(git rev-parse --show-toplevel 2>/dev/null) || { echo "not in a git repo" >&2; exit 2; }
. "$root/.workflow/agents.sh"

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
if [ "$role" = "implement" ] && ! "$root/.workflow/review-gate-check.sh" --plan-only "$wt" src/_probe >/dev/null 2>&1; then
  echo "review gate refuses this change; not starting:" >&2
  "$root/.workflow/review-gate-check.sh" --plan-only "$wt" src/_probe 2>&1 >/dev/null | sed 's/^/  /' >&2
  exit 1
fi

log="$wt/.agent-$role-$agent.log"
raw="$wt/.agent-$role-$agent.json"
conv_file="$wt/.agent-$role-$agent.conversation"

resume=""
if [ "$resume_requested" -eq 1 ]; then
  [ -s "$conv_file" ] || { echo "--resume needs a previous run; no id at $conv_file" >&2; exit 2; }
  resume=$(cat "$conv_file")
fi

if [ "$role" = "implement" ]; then
  base="/opsx-apply $change. Stop when the tasks are implemented. Do not commit. Do not archive. Do not sync specs into openspec/specs/. Do not move or delete the change folder. Do not edit docs/SPEC.md, which is frozen. Check a task only after actually performing it."
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
    git status --porcelain -- . ':!openspec/changes' ':!.agent-*'
    git diff HEAD -- . ':!openspec/changes' ':!.agent-*'
    git ls-files --others --exclude-standard -- . ':!openspec/changes' ':!.agent-*' \
      | LC_ALL=C sort | tr '\n' '\0' | xargs -0 -r sha256sum
  ) 2>/dev/null | sha256sum | cut -d' ' -f1
}
before_digest=$(worktree_digest)

( cd "$wt" && pty_run "$cmd" ) 2>&1 | sed 's/\x1B\[[0-9;]*[A-Za-z]//g' | tr -d '\r' > "$raw"
status=$?
after_digest=$(worktree_digest)

# Only the FINAL lines are searched for the agent's own result envelope. An agent
# that reads another agent's `.agent-*.json` echoes that file's envelope into its
# transcript, and a whole-file grep then stored the wrong agent's id: a codex review
# that had read `.agent-implement-agy.json` recorded agy's conversation as its own,
# so `--resume` would have continued the implementer's session instead.
json=$(tail -5 "$raw" | grep -o '{"conversation_id".*}' | tail -1 || true)
if [ -n "$json" ]; then
  printf '%s' "$json" | jq -r '.response // ""' > "$log"
  printf '%s' "$json" | jq -r '.conversation_id // empty' > "$conv_file"
  agent_status=$(printf '%s' "$json" | jq -r '.status // "UNKNOWN"')
else
  cp "$raw" "$log"
  agent_status="NO_STRUCTURED_RESULT"
  # codex emits no envelope, but prints its own session id, so `--resume` can still
  # continue it rather than starting the round from scratch.
  codex_session=$(grep -o 'session id: [0-9a-f-]\{36\}' "$raw" | tail -1 | sed 's/session id: //')
  [ -n "$codex_session" ] && printf '%s' "$codex_session" > "$conv_file"
fi

changed=$(cd "$wt" && git status --porcelain -- . ':!openspec/changes' ':!.agent-*' | wc -l | tr -d ' ')
echo "role: $role   agent: $agent   status: $agent_status   exit: $status"
echo "files touched: $changed"
echo "log: $log"
echo "--- last 30 lines ---"
tail -30 "$log"

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
