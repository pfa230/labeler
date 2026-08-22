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

# Refuse to start if the gate would refuse the commit anyway. Failing here beats
# letting an agent write code that cannot land.
if ! "$root/scripts/review-gate-check.sh" "$wt" src/_apply_probe >/dev/null 2>&1; then
  echo "review gate refuses this change; not starting apply:" >&2
  "$root/scripts/review-gate-check.sh" "$wt" src/_apply_probe 2>&1 >/dev/null | sed 's/^/  /' >&2
  exit 1
fi

log="$wt/.agy-apply.log"
prompt="/openspec-apply-change $change. Do not commit; the change is committed once at the end after archive and verification. $extra"

# `script` gives agy a pseudo-TTY: bare `agy -p` off a pipe greets or narrates
# instead of working. sed/tr strip ANSI and carriage returns.
( cd "$wt" && script -q /dev/null agy -p --mode accept-edits --effort high "$prompt" ) \
  2>&1 | sed 's/\x1B\[[0-9;]*[A-Za-z]//g' | tr -d '\r' > "$log"
status=$?

echo "log: $log"
echo "exit: $status"
echo "--- last 30 lines ---"
tail -30 "$log"
exit $status
