#!/usr/bin/env bash
# Run the apply stage on agy, from Claude, without a human cd-ing anywhere.
#
#   .workflow/apply-with-agy.sh <change-name> [extra prompt...]
#
# Claude orchestrates, agy implements, Claude reviews the resulting diff: the
# implementer and the reviewer are different models by construction.
#
# agy's transcript goes to a log file, NOT to stdout. A reviewer or implementer
# transcript runs to thousands of lines and passing it through the orchestrator's
# context is pure waste. Only the tail and the exit status come back; read the log
# directly if more is needed.
set -uo pipefail

change="${1:?change name required, e.g. issue-186-pin-rust-toolchain}"; shift || true

# --fix resumes the conversation from the apply instead of starting a new one. A
# review round corrects work agy still remembers; a fresh prompt makes it rebuild
# that understanding from the diff, losing why it chose what it chose.
resume_requested=0
if [ "${1:-}" = "--fix" ]; then shift; resume_requested=1; fi
extra="$*"

root=$(git rev-parse --show-toplevel 2>/dev/null) || { echo "not in a git repo" >&2; exit 2; }
issue=$(printf '%s' "$change" | sed -n 's/^\(issue-[0-9]\{1,\}\).*/\1/p')
[ -n "$issue" ] || { echo "change name must start with issue-<N>-: $change" >&2; exit 2; }

wt="$root/.worktrees/$issue"
[ -d "$wt" ] || { echo "no worktree at $wt. Create it before applying." >&2; exit 2; }
[ -d "$wt/openspec/changes/$change" ] || { echo "no change '$change' in $wt" >&2; exit 2; }
command -v agy >/dev/null 2>&1 || { echo "agy is not on PATH; nothing would run." >&2; exit 2; }

# Refuse to start if the gate would refuse the commit anyway. Failing here beats
# letting an agent write code that cannot land.
if ! "$root/.workflow/review-gate-check.sh" "$wt" src/_apply_probe >/dev/null 2>&1; then
  echo "review gate refuses this change; not starting apply:" >&2
  "$root/.workflow/review-gate-check.sh" "$wt" src/_apply_probe 2>&1 >/dev/null | sed 's/^/  /' >&2
  exit 1
fi

# Hold a lock for the duration. The apply stage implements and stops; it does not
# commit, merge, push or archive. Telling an agent that is a request, and agents have
# merged and left main in a broken state anyway, so git refuses instead: pre-commit,
# pre-merge-commit and pre-push all check this file. It lives in the common git dir,
# so it applies from every worktree and from the repo root.
lock="$(cd "$wt" && git rev-parse --path-format=absolute --git-common-dir)/APPLY_IN_PROGRESS"
if [ -f "$lock" ]; then
  echo "an apply is already in progress: $(cat "$lock")" >&2
  echo "if that is stale, remove $lock" >&2
  exit 1
fi
printf '%s started %s (pid %s)\n' "$change" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$$" > "$lock"

log="$wt/.agy-apply.log"
conv_file="$wt/.agy-conversation"
raw="$wt/.agy-apply.json"

# Salvage whatever the stream has already written, then release the lock. A killed
# apply used to leave an empty file and no conversation id, so the work was there in
# the worktree but the agent that did it was unreachable and `--fix` could not run.
# With --output-format stream-json the first line is an `init` event carrying
# conversation_id, so this recovers a resumable id seconds into a run and the prose
# up to the moment of the kill.
salvage() {
  [ -s "$raw" ] || return 0
  local id
  id=$(grep -m1 -o '"conversation_id":"[^"]*"' "$raw" | cut -d'"' -f4 || true)
  [ -n "$id" ] && printf '%s' "$id" > "$conv_file"
  jq -rs 'map(select(.event=="result"))[-1].result.response // empty' "$raw" 2>/dev/null > "$log" \
    || cp "$raw" "$log"
}
trap 'salvage; rm -f "$lock"' EXIT INT TERM

# The id, from the best source that has it. $conv_file is written by a clean exit or
# by salvage(); $raw is the live stream and carries the id from its first line onward,
# so it answers even when neither ran - bash deals a trap only after the foreground
# command returns, so a signal delivered to this shell alone leaves salvage() queued
# behind a pipeline that is still running. Reading the stream directly does not care.
conversation_id() {
  if [ -s "$conv_file" ]; then cat "$conv_file"; return 0; fi
  [ -s "$raw" ] || return 1
  grep -m1 -o '"conversation_id":"[^"]*"' "$raw" | cut -d'"' -f4
}

resume=""
if [ "$resume_requested" -eq 1 ]; then
  prev=$(conversation_id || true)
  if [ -n "$prev" ]; then
    printf -v resume -- '--conversation=%q' "$prev"
  else
    # Nothing survives a run killed before its first line reached disk. `--continue`
    # takes the most recent conversation, which is that run, so a fix round is still
    # possible.
    resume='--continue'
    echo "no conversation id in $conv_file or $raw; falling back to --continue" >&2
  fi
fi
# WORKFLOW form, not skill form. OpenSpec writes both for the Antigravity target:
# .agent/skills/openspec-*/SKILL.md and .agent/workflows/opsx-*.md. Print mode
# resolves the workflow; opsx-apply.md documents its own invocation as
# "/opsx-apply add-auth". Sending the skill name silently resolves to nothing and
# agy answers from its own documentation instead of working, which is what the
# no-op detection below exists to catch.
if [ "$resume_requested" -eq 1 ]; then
  prompt="Review findings on your implementation of $change. Fix each one, then stop. The same limits still hold: do not commit, do not archive, do not sync specs into openspec/specs/, do not move or delete the change folder, do not edit docs/SPEC.md. $extra"
else
  prompt="/opsx-apply $change. Stop when the tasks are implemented. Do not commit. Do not archive. Do not sync specs into openspec/specs/. Do not move or delete the change folder. Do not edit docs/SPEC.md, which is frozen. Check a task only after actually performing it, including the ones that say to render a label and look at it. $extra"
fi

# The prompt names the stage boundary in the imperative, and bluntly. `openspec/config.yaml`
# already carried "Do not commit here" when agy, on 2026-08-24, committed, archived, synced the
# deltas into `openspec/specs/`, edited the frozen `docs/SPEC.md` and checked three tasks it had
# not performed. Guidance an agent has to infer a boundary from is guidance it argues itself past,
# so the boundary is spelled out here too, at the point of invocation, in the fewest words that
# leave nothing to infer. Do not soften this into a description of the workflow: the previous
# wording mentioned archive ("committed once at the end after archive and verification") and got
# an archive.
#
# `script` gives agy a pseudo-TTY: bare `agy -p` off a pipe greets or narrates
# instead of working. util-linux script(1) is `script [options] [file]`, so a
# trailing command is read as script's own options and the run dies on
# `invalid option -- 'p'` before agy starts; the agent goes in -c instead, and
# -e is what makes the child's exit status the wrapper's. %q quotes a prompt
# that contains quotes of its own, so SHELL pins the interpreter script(1)
# hands it to: %q emits bash quoting and the operator's login shell need not be
# bash. sed/tr strip ANSI and carriage returns.
#
# -p is the short alias for --print, and --print TAKES A VALUE, so the prompt is
# attached (-p=...) and -p goes last. Written as a bare `agy -p --mode ...` the
# flag swallows `--mode` as its prompt and the real prompt is left as an ignored
# positional; agy now refuses that outright instead of running.
# Cap cargo's parallelism for everything the agent runs. Unset, cargo takes one
# codegen job per core and holds a rustc, and finally a linker, for each; on this
# project that means building Typst's dependency graph several times over for
# `cargo test && cargo clippy --all-targets --all-features`. With an agent, an
# orchestrator and a reviewer all resident, three applies in a row died to the OOM
# killer mid-build: user.slice showed oom_kill 3, peak 15.2GB against 15GB of RAM,
# and 4GB of swap fully consumed. Overridable for a machine with room to spare.
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"

timeout="${AGY_PRINT_TIMEOUT:-120m}"

# --effort is deliberately absent. The default model rejects it outright:
#   Error: invalid model selection (--model "" --effort "high"): --effort is not
#   supported for the current model
# and agy still exits 0 while doing nothing, so it fails as a silent no-op.
#
# --output-format json buys two things a prose transcript cannot: a `status` field,
# so success stops being inferred from an exit code that is 0 even on that error,
# and a `conversation_id`, which is what lets a fix round resume instead of starting
# over.
#
# On a fix round $resume pins that conversation. agy keeps what it just built and
# why it chose it; a fresh prompt would make it re-derive both from the diff.
printf -v agy_cmd 'agy --mode accept-edits --print-timeout %q --output-format stream-json %s -p=%q' \
  "$timeout" "$resume" "$prompt"

# script(1) is two incompatible programs with one name. util-linux is
# `script [options] -c CMD FILE`; BSD/macOS is `script [options] FILE CMD ARGS...`
# with no -c at all, so each form is an error on the other platform. Detect once
# rather than assume: this repo is developed on macOS and its CI is ubuntu, so a
# form that works for whoever wrote it silently breaks for everyone else. -e makes
# the child's status the wrapper's on both.
if script -q -e -c true /dev/null >/dev/null 2>&1; then
  run_pty() { script -q -e -c "$1" /dev/null; }
else
  run_pty() { script -q -e /dev/null "${BASH:-/bin/bash}" -c "$1"; }
fi

# -u and stdbuf -oL matter as much as stream-json does: block-buffered filters would
# hold the first lines in a 4KB pipe buffer and a kill would still find $raw empty.
( cd "$wt" && run_pty "$agy_cmd" ) \
  2>&1 | sed -u 's/\x1B\[[0-9;]*[A-Za-z]//g' | stdbuf -oL tr -d '\r' > "$raw"
status=$?

# Pull the result object out of whatever the pty wrapper wrapped around it, then
# keep the prose in the log the operator reads and the id where the next round
# looks for it.
result=$(jq -cs 'map(select(.event=="result"))[-1].result // empty' "$raw" 2>/dev/null || true)
if [ -n "$result" ]; then
  printf '%s' "$result" | jq -r '.response // ""' > "$log"
  printf '%s' "$result" | jq -r '.conversation_id // empty' > "$conv_file"
  agy_status=$(printf '%s' "$result" | jq -r '.status // "UNKNOWN"')
else
  # No result event: the run was killed or died. salvage() has already written what
  # the stream held, including the id, so a fix round can still reach this agent.
  salvage
  agy_status="NO_RESULT"
fi

# 127 is the pty shell failing to exec agy. Say so, because the tail below is
# then the wrapper's own error and reads exactly like agy refusing the task.
if [ "$status" -eq 127 ]; then
  echo "agy failed to start under script(1); the output below is not agent output." >&2
fi

echo "log: $log"
echo "exit: $status"
echo "agy status: $agy_status"

# A clean exit is not success. agy answering a question about its own flags exits 0
# having written nothing, and that read as a completed apply. Success is a changed
# tree, so check the tree. The change folder itself is excluded: it was there before.
changed=$(cd "$wt" && git status --porcelain -- . ':!openspec/changes' ':!.agy-apply.log' | wc -l | tr -d ' ')
echo "files touched: $changed"
echo "--- last 30 lines ---"
tail -30 "$log"

if [ "$agy_status" != "SUCCESS" ]; then
  echo >&2
  echo "agy did not report success (status: $agy_status). Raw result: $raw" >&2
  exit 4
fi

if [ "$status" -eq 0 ] && [ "$changed" -eq 0 ]; then
  echo >&2
  echo "apply produced no changes despite exiting 0. It did not run." >&2
  echo "Usual cause: the slash command did not resolve, so agy answered from its own docs." >&2
  echo "Check the log for agy documentation instead of work: $log" >&2
  exit 3
fi
exit $status
