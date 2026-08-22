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
prompt="/openspec-apply-change $change. Do not commit; the change is committed once at the end after archive and verification. $extra"

# `script` gives agy a pseudo-TTY: bare `agy -p` off a pipe greets or narrates
# instead of working. util-linux script(1) is `script [options] [file]`, so a
# trailing command is read as script's own options and the run dies on
# `invalid option -- 'p'` before agy starts; the agent goes in -c instead, and
# -e is what makes the child's exit status the wrapper's. %q quotes a prompt
# that contains quotes of its own, so SHELL pins the interpreter script(1)
# hands it to: %q emits bash quoting and the operator's login shell need not be
# bash. sed/tr strip ANSI and carriage returns.
printf -v agy_cmd 'agy -p --mode accept-edits --effort high %q' "$prompt"
( cd "$wt" && SHELL="$BASH" script -q -e -c "$agy_cmd" /dev/null ) \
  2>&1 | sed 's/\x1B\[[0-9;]*[A-Za-z]//g' | tr -d '\r' > "$log"
status=$?

# 127 is the pty shell failing to exec agy. Say so, because the tail below is
# then the wrapper's own error and reads exactly like agy refusing the task.
if [ "$status" -eq 127 ]; then
  echo "agy failed to start under script(1); the output below is not agent output." >&2
fi

echo "log: $log"
echo "exit: $status"
echo "--- last 30 lines ---"
tail -30 "$log"
exit $status
