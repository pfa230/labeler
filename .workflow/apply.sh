#!/usr/bin/env bash
# Implement a change on one agent, review it on another, loop until it passes.
#
#   .workflow/apply.sh <change> <implementer> <reviewer> [max-rounds]
#   .workflow/apply.sh issue-123-thing agy codex
#
# The pair is the point: the model that writes the code is never the model that
# judges it. Findings go back to the implementer, which resumes its session and
# keeps what it built; the reviewer re-checks and never edits.
#
# Stops before commit, archive and merge. Those are deliberate steps, and the apply
# lock makes git refuse them mid-run anyway.
set -uo pipefail

change="${1:?usage: apply.sh <change> <implementer> <reviewer> [max-rounds]}"
implementer="${2:?implementer required, e.g. agy}"
reviewer="${3:?reviewer required, e.g. codex}"
max_rounds="${4:-3}"

if [ "$implementer" = "$reviewer" ]; then
  echo "implementer and reviewer must differ: both are '$implementer'." >&2
  echo "Nobody reviews their own work; that is the entire reason this takes two names." >&2
  exit 2
fi

root=$(git rev-parse --show-toplevel 2>/dev/null) || { echo "not in a git repo" >&2; exit 2; }
stage="$root/.workflow/run-stage.sh"
issue=$(printf '%s' "$change" | sed -n 's/^\(issue-[0-9]\{1,\}\).*/\1/p')
wt="$root/.worktrees/$issue"

say() { printf '\n== %s ==\n' "$1"; }

say "implement: $implementer"
"$stage" implement "$implementer" "$change" || { echo "implement failed; stopping." >&2; exit 1; }

round=1
while :; do
  say "review $round: $reviewer"
  "$stage" review "$reviewer" "$change"
  rc=$?
  review_log="$wt/.agent-review-$reviewer.log"
  [ "$rc" -eq 5 ] && { echo "the reviewer edited files; its verdict cannot be trusted." >&2; exit 5; }

  # Last line-start VERDICT wins: the reviewer's final word ends its output, and a
  # verdict quoted mid-prose never starts a line. This is deliberately more
  # forgiving than review-gate-check.sh, which refuses anything but exactly one:
  # that gate decides whether code may be committed, this only decides whether to
  # loop again.
  # Searched in the closing lines only. When the agent emits no structured result the
  # log is its whole transcript, which contains every file it read: a reviewer that
  # opened `review.md` echoed the PLAN review's `VERDICT: APPROVE` into its own log,
  # and a whole-file grep read that as the diff verdict.
  verdict=$(tail -40 "$review_log" 2>/dev/null | grep -o '^VERDICT:[[:space:]]*[A-Z_]*' | tail -1 | sed 's/^VERDICT:[[:space:]]*//')
  # Every round is preserved, the approving one included. The gate reads
  # diff-review.md, so a verdict that exists only in an untracked log is a verdict
  # nothing can check (#223).
  cp "$review_log" "$wt/openspec/changes/$change/diff-review-$round.md" 2>/dev/null || true

  case "$verdict" in
    APPROVE)
      dr="$wt/openspec/changes/$change/diff-review.md"
      {
        printf '# Diff review\n\n'
        printf 'AUTHOR: %s\n' "$implementer"
        printf 'REVIEWER: %s\n' "$reviewer"
        printf 'VERDICT: APPROVE\n'
        printf 'ROUNDS: %s\n\n' "$round"
        # The body's own verdict line is dropped: the canonical one is above, and the
        # gate refuses a file carrying two.
        grep -v '^VERDICT:' "$review_log"
      } > "$dr"
      say "APPROVE after $round round(s)"
      echo "Recorded in openspec/changes/$change/diff-review.md."
      echo "Not committed, not archived, not merged: those are separate steps."
      exit 0 ;;
    REVISE) ;;
    *)
      echo "no readable VERDICT line in $review_log (found '${verdict:-none}')." >&2
      echo "Refusing to guess whether the review passed." >&2
      exit 4 ;;
  esac

  if [ "$round" -ge "$max_rounds" ]; then
    say "still REVISE after $max_rounds round(s)"
    echo "Stopping rather than looping. The findings are in $review_log; a change that cannot" >&2
    echo "converge in $max_rounds rounds wants a human, not another round." >&2
    exit 6
  fi

  say "fix round $round: $implementer"
  "$stage" implement "$implementer" "$change" --resume "$(cat "$review_log")" \
    || { echo "fix round failed; stopping." >&2; exit 1; }
  round=$((round + 1))
done
