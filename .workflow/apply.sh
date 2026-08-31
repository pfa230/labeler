#!/usr/bin/env bash
# Implement a change on one agent, review it on another, loop until it passes.
#
#   .workflow/apply.sh <implementer> <reviewer> [change] [--rounds N] [--dry-run]
#   .workflow/apply.sh agy codex                  # resolves the change from the worktree
#   .workflow/apply.sh agy codex issue-123-thing  # or name it
#
# The pair is the point, and it is named first for that reason: the model that
# writes the code is never the model that judges it (#224). The change comes last
# because it is the one argument the tool can usually work out for itself.
#
# Findings go back to the implementer, which resumes its session and keeps what it
# built; the reviewer re-checks and never edits.
#
# Stops before commit, archive and merge. Those are deliberate steps, and the apply
# lock makes git refuse them mid-run anyway.
#
# Exit 8 = a stage wrote QUESTIONS.md and stopped rather than guess (#283). Answer them
# in ANSWERS.md at the worktree root and re-run; every stage reads that file.
set -uo pipefail

usage='usage: apply.sh <implementer> <reviewer> [change] [--rounds N] [--dry-run]'
here=$(cd "$(dirname "$0")" && pwd)

implementer=""; reviewer=""; change=""; max_rounds=3; dry_run=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --rounds) max_rounds="${2:?--rounds needs a number}"; shift 2 ;;
    --rounds=*) max_rounds="${1#*=}"; shift ;;
    --dry-run) dry_run=1; shift ;;
    -h|--help) echo "$usage"; exit 0 ;;
    -*) echo "unknown option: $1" >&2; echo "$usage" >&2; exit 2 ;;
    *)
      if   [ -z "$implementer" ]; then implementer="$1"
      elif [ -z "$reviewer" ];    then reviewer="$1"
      elif [ -z "$change" ];      then change="$1"
      else echo "too many arguments: $1" >&2; echo "$usage" >&2; exit 2; fi
      shift ;;
  esac
done

[ -n "$implementer" ] || { echo "$usage" >&2; exit 2; }
[ -n "$reviewer" ] || { echo "reviewer required, e.g. codex" >&2; echo "$usage" >&2; exit 2; }

case "$max_rounds" in ''|*[!0-9]*) echo "--rounds takes a number, got '$max_rounds'" >&2; exit 2 ;; esac
[ "$max_rounds" -ge 1 ] || { echo "--rounds must be at least 1" >&2; exit 2; }

if [ "$implementer" = "$reviewer" ]; then
  echo "implementer and reviewer must differ: both are '$implementer'." >&2
  echo "Nobody reviews their own work; that is the entire reason this takes two names." >&2
  exit 2
fi
. "$here/agents.sh"
agent_resumable "$implementer" || {
  echo "$implementer cannot be resumed, so it cannot be the implementer here." >&2
  echo "Every fix round continues the session that wrote the code; an agent that cannot" >&2
  echo "be resumed would either start over or stop at the first REVISE." >&2
  exit 2; }

# The main checkout, not whichever worktree we were called from: --show-toplevel
# answers the latter, and .worktrees/ hangs off the former.
common=$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null) || { echo "not in a git repo" >&2; exit 2; }
root=$(dirname "$common")
here_top=$(git rev-parse --show-toplevel 2>/dev/null)

# A live change folder is untracked and sits inside a worktree, so that is where to
# look. Called from inside one, only that worktree counts: a session that just
# proposed is standing in the answer.
live_changes() { # live_changes <checkout>... -> one change name per line
  local c d
  for c in "$@"; do
    for d in "$c"/openspec/changes/*/; do
      [ -d "$d" ] || continue
      case "$(basename "$d")" in archive) continue ;; esac
      basename "$d"
    done
  done
}

if [ -z "$change" ]; then
  if [ -n "$here_top" ] && [ "$here_top" != "$root" ]; then
    found=$(live_changes "$here_top")
    where="$here_top"
  else
    found=$(live_changes "$root"/.worktrees/*/)
    where="$root/.worktrees"
  fi
  count=$(printf '%s' "$found" | grep -c . || true)
  case "$count" in
    1) change="$found"; echo "change: $change (resolved from $where)" ;;
    0) echo "no change in flight under $where, and none named." >&2
       echo "$usage" >&2; exit 2 ;;
    *) echo "several changes in flight under $where; name the one you mean:" >&2
       printf '  %s\n' $found >&2
       echo "$usage" >&2; exit 2 ;;
  esac
fi

stage="$here/run-stage.sh"
. "$here/questions.sh"

# The first free index, so a re-run after a stop adds a round rather than overwriting
# the record of an earlier one.
next_round_file() { # next_round_file <dir> <prefix> -> basename
  local dir="$1" pre="$2" i=1
  while [ -e "$dir/$pre-$i.md" ]; do i=$((i + 1)); done
  printf '%s-%s.md' "$pre" "$i"
}
issue=$(printf '%s' "$change" | sed -n 's/^\(issue-[0-9]\{1,\}\).*/\1/p')
[ -n "$issue" ] || { echo "change name must start with issue-<N>-: $change" >&2; exit 2; }
wt="$root/.worktrees/$issue"

if [ "$dry_run" = "1" ]; then
  printf 'implementer: %s\nreviewer: %s\nchange: %s\nworktree: %s\nrounds: %s\n' \
    "$implementer" "$reviewer" "$change" "$wt" "$max_rounds"
  exit 0
fi

say() { printf '\n== %s ==\n' "$1"; }

# A stage that stopped to ask stops the loop with it (#283). Checked before the exit
# status is judged, because asking IS how that stage failed: an implementer that wrote
# its questions and stopped exits 3, having changed nothing, and reporting that as "it
# did not run" buries the question it wrote.
ask_stop() { # ask_stop <label> <role>
  questions_pending "$wt" || return 0
  questions_record "$wt" "$2"
  questions_report "$wt" "$1"
  exit 8
}

# Resumed when this worktree already holds a session: a re-run after a stop continues
# the implementation rather than starting a second one over the top of the first.
first=""
[ -s "$wt/.agent-runs/implement-$implementer.conversation" ] && first="--resume"
say "implement: $implementer"
"$stage" implement "$implementer" "$change" $first; rc=$?
ask_stop "implement ($implementer)" implement
[ "$rc" -eq 0 ] || { echo "implement failed; stopping." >&2; exit 1; }

round=1
while :; do
  say "review $round: $reviewer"
  "$stage" review "$reviewer" "$change"
  rc=$?
  ask_stop "review $round ($reviewer)" review
  review_log="$wt/.agent-runs/review-$reviewer.log"
  [ "$rc" -eq 5 ] && { echo "the reviewer edited files; its verdict cannot be trusted." >&2; exit 5; }
  # Any other non-zero exit is a review that did not finish. A CLI can print a verdict
  # and then die, and reading that verdict would record an approval nobody stands behind.
  [ "$rc" -ne 0 ] && [ "$rc" -ne 7 ] && { echo "the reviewer exited $rc; its verdict cannot be trusted." >&2; exit 1; }
  # Checked before anything is copied or read: a transcript must not become the
  # round artifact, and must not be mistaken for a verdict.
  [ "$rc" -eq 7 ] && { echo "the reviewer produced a transcript, not a review; stopping." >&2; exit 7; }

  # Last line-start VERDICT wins: the reviewer's final word ends its output, and a
  # verdict quoted mid-prose never starts a line.
  #
  # Matched as a WHOLE line against the two verdicts this loop accepts, never as a
  # prefix: a prefix match reads "VERDICT: APPROVE WITH CHANGES" as APPROVE. Anything
  # unrecognised yields nothing, which is the refusal below rather than a guess.
  # Searched in the closing lines only. When the agent emits no structured result the
  # log is its whole transcript, which contains every file it read: a reviewer that
  # opened `review.md` echoed the PLAN review's `VERDICT: APPROVE` into its own log,
  # and a whole-file grep read that as the diff verdict.
  verdict=$(tail -40 "$review_log" 2>/dev/null \
    | grep -E '^VERDICT:[[:space:]]*(APPROVE|REVISE)[[:space:]]*$' \
    | tail -1 | sed 's/^VERDICT:[[:space:]]*//' | tr -d '[:space:]')
  # Every round is preserved, the approving one included. The gate reads
  # diff-review.md, so a verdict that exists only in an untracked log is a verdict
  # nothing can check (#223).
  round_file=$(next_round_file "$wt/openspec/changes/$change" diff-review)
  # The canonical count is the artifact's index, not this invocation's loop counter: a
  # restart begins at 1 while the file it writes is diff-review-4.md.
  round_no=$(printf '%s' "$round_file" | sed 's/[^0-9]//g')
  cp "$review_log" "$wt/openspec/changes/$change/$round_file" 2>/dev/null || true

  case "$verdict" in
    APPROVE)
      dr="$wt/openspec/changes/$change/diff-review.md"
      {
        printf '# Diff review\n\n'
        printf 'AUTHOR: %s\n' "$implementer"
        printf 'REVIEWER: %s\n' "$reviewer"
        printf 'VERDICT: APPROVE\n'
        printf 'ROUNDS: %s\n' "$round_no"
        # The contract this code was approved against. A later plan revision changes it,
        # and whoever reads this verdict then knows the approval no longer covers what
        # is in the folder: run-change.sh retires it on exactly that comparison.
        printf 'SPECS_SHA256: %s\n\n' "$("$here/specs-digest.sh" "$wt/openspec/changes/$change" 2>/dev/null)"
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

  # The findings go to the implementer BEFORE the cap is weighed. Checking the cap
  # first means the last REVISE is never acted on, so a re-run reviews the same
  # unchanged diff, gets the same verdict, and stops again: a loop that cannot make
  # progress no matter how many times a person restarts it.
  #
  # The findings go by path, not by value. Passing them as an argument put the whole
  # review on the command line, where agents.sh re-quotes it and pty_run evals the
  # result, so a large review died with "Argument list too long" before the
  # implementer started (#264). The round artifact is already on disk, inside the
  # worktree the implementer runs in.
  say "fix round $round: $implementer"
  "$stage" implement "$implementer" "$change" --resume \
    "Review findings on your implementation. They are in openspec/changes/$change/$round_file, relative to your worktree root. Read that file first; it is the whole review, and fixing every finding is the task."
  rc=$?
  ask_stop "fix round $round ($implementer)" implement
  [ "$rc" -eq 0 ] || { echo "fix round failed; stopping." >&2; exit 1; }

  if [ "$round" -ge "$max_rounds" ]; then
    say "still REVISE after $max_rounds round(s)"
    echo "Stopping rather than looping. The findings are in $review_log, and the implementer" >&2
    echo "has acted on them; a change that cannot converge in $max_rounds rounds wants a human." >&2
    echo "Re-running reviews what the implementer just fixed, so a restart makes progress." >&2
    exit 6
  fi
  round=$((round + 1))
done
