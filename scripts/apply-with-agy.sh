#!/usr/bin/env bash
# Run the apply stage on agy, from Claude, without a human cd-ing anywhere.
#
#   scripts/apply-with-agy.sh <change-name> [extra prompt...]
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
if ! "$root/scripts/review-gate-check.sh" "$wt" src/_apply_probe >/dev/null 2>&1; then
  echo "review gate refuses this change; not starting apply:" >&2
  "$root/scripts/review-gate-check.sh" "$wt" src/_apply_probe 2>&1 >/dev/null | sed 's/^/  /' >&2
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
trap 'rm -f "$lock"' EXIT INT TERM

log="$wt/.agy-apply.log"
# WORKFLOW form, not skill form. OpenSpec writes both for the Antigravity target:
# .agent/skills/openspec-*/SKILL.md and .agent/workflows/opsx-*.md. Print mode
# resolves the workflow; opsx-apply.md documents its own invocation as
# "/opsx-apply add-auth". Sending the skill name silently resolves to nothing and
# agy answers from its own documentation instead of working, which is what the
# no-op detection below exists to catch.
prompt="/opsx-apply $change. Do not commit; the change is committed once at the end after archive and verification. $extra"

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
timeout="${AGY_PRINT_TIMEOUT:-120m}"
printf -v agy_cmd 'agy --mode accept-edits --effort high --print-timeout %q -p=%q' "$timeout" "$prompt"

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

( cd "$wt" && run_pty "$agy_cmd" ) \
  2>&1 | sed 's/\x1B\[[0-9;]*[A-Za-z]//g' | tr -d '\r' > "$log"
status=$?

# 127 is the pty shell failing to exec agy. Say so, because the tail below is
# then the wrapper's own error and reads exactly like agy refusing the task.
if [ "$status" -eq 127 ]; then
  echo "agy failed to start under script(1); the output below is not agent output." >&2
fi

echo "log: $log"
echo "exit: $status"

# A clean exit is not success. agy answering a question about its own flags exits 0
# having written nothing, and that read as a completed apply. Success is a changed
# tree, so check the tree. The change folder itself is excluded: it was there before.
changed=$(cd "$wt" && git status --porcelain -- . ':!openspec/changes' ':!.agy-apply.log' | wc -l | tr -d ' ')
echo "files touched: $changed"
echo "--- last 30 lines ---"
tail -30 "$log"

if [ "$status" -eq 0 ] && [ "$changed" -eq 0 ]; then
  echo >&2
  echo "apply produced no changes despite exiting 0. It did not run." >&2
  echo "Usual cause: the slash command did not resolve, so agy answered from its own docs." >&2
  echo "Check the log for agy documentation instead of work: $log" >&2
  exit 3
fi
exit $status
