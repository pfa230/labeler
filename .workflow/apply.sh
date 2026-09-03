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
# Exit 7 = a stage produced no readable result, so nothing it did can be reported (#315).
# It covers the implementer as well as the reviewer: an implement stage whose answer cannot
# be read is indistinguishable from one that ran, and during #287 the loop reviewed a diff
# on that basis.
#
# Exit 8 = a stage wrote QUESTIONS.md and stopped rather than guess (#283). Answer them
# in ANSWERS.md at the worktree root and re-run; every stage reads that file.
#
# Exit 2 also covers a reviewer the change's author ledger already names: that pairing is
# refused here rather than at the landing gate, which sees it only after every agent has
# run (#313).
#
# Exit 10 = the fix round changed nothing, so the next review would judge bytes a previous
# round already judged (#299). Nothing is launched. Either the implementer answered every
# finding in prose, or it acted on none of them, and no round of review can tell those
# apart; that is a person's call, which is why this stops for one.
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

# run-stage.sh prints the digest of the tree it left behind, and that is the only place it
# can be measured. Captured from stdout alone: stderr flows straight through, so nothing a
# caller reads there moves.
#
# PIPESTATUS[0] rather than $?, though `set -o pipefail` above already makes the two agree
# whenever the stage is what failed. They part when `tee` is: a full disk fails the write,
# pipefail reports that as the pipeline's status, and apply.sh would stop saying the
# implement failed when it did not. The stage's own status is the thing being asked for.
run_stage() { # run_stage <run-stage.sh args...> -> run-stage.sh's own exit status
  mkdir -p "$wt/.agent-runs"
  "$stage" "$@" | tee "$wt/.agent-runs/apply-stage.out"
  return "${PIPESTATUS[0]}"
}
tree_printed() { # tree_printed -> the digest the last stage printed, or empty
  grep -E '^tree: [0-9a-f]{64}$' "$wt/.agent-runs/apply-stage.out" 2>/dev/null \
    | tail -1 | sed 's/^tree:[[:space:]]*//'
}

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

# The digest recorded for the newest round already on disk. Read back from the artifact
# rather than held in a variable: a restarted apply.sh begins its round counter at 1 again,
# and a variable would leave it with nothing to compare against (#299).
last_round_tree() { # last_round_tree -> digest, or empty when no round has run
  local dir="$wt/openspec/changes/$change" i=1 last=""
  while [ -e "$dir/diff-review-$i.md" ]; do last="$dir/diff-review-$i.md"; i=$((i + 1)); done
  [ -n "$last" ] || return 0
  grep -E '^TREE_SHA256: [0-9a-f]{64}$' "$last" 2>/dev/null | head -1 | sed 's/^TREE_SHA256:[[:space:]]*//'
}
last_round_file() { # last_round_file -> basename of the newest round artifact, or empty
  local dir="$wt/openspec/changes/$change" i=1 last=""
  while [ -e "$dir/diff-review-$i.md" ]; do last="diff-review-$i.md"; i=$((i + 1)); done
  printf '%s' "$last"
}

# The reviewer must not be one of the authors, and the ledger is the only place that is
# knowable before the fact. The landing gate refuses a diff-review.md whose REVIEWER appears
# among its AUTHORS, so left to that gate this surfaces at the commit, after every agent has
# been paid for. It is reachable without an implementer swap: on a change whose delta is the
# whole deliverable the planner is the only author, so the code reviewer named at launch can
# turn out to be the one agent that cannot review it (#313).
# Lowercased on both sides, because review-gate-check.sh:88 compares that way and the two
# must agree: a name this matched case-sensitively would pass here, launch both agents, and
# be refused at the commit, which is the worst place to learn it.
ledger="$wt/openspec/changes/$change/authors"
if [ -f "$ledger" ] && tr '[:upper:]' '[:lower:]' < "$ledger" \
     | grep -qxF "$(printf '%s' "$reviewer" | tr '[:upper:]' '[:lower:]')"; then
  echo "$reviewer wrote part of $change: the author ledger at $ledger names it." >&2
  echo "Nobody reviews their own work, and the landing gate refuses a diff-review.md whose" >&2
  echo "REVIEWER is among its AUTHORS. Name a reviewer that wrote none of this." >&2
  exit 2
fi

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

# run-stage.sh exits 7 when no answer could be read out of what the agent printed, and it
# does so for a writing role as readily as for a review (#315). One spelling for both roles
# here, because it is one failure: the reviewer's used to say "produced a transcript", which
# is only half of it - an agent that printed nothing produced no transcript either, and that
# is the half that destroyed a run's record during #287. Carried out with its own status
# rather than folded into the generic stop, so a person is not sent looking for a failure
# the agent never reported.
no_account_stop() { # no_account_stop <rc> <who>
  [ "$1" -eq 7 ] || return 0
  echo "$2 produced no readable result; stopping." >&2
  echo "Its log under .agent-runs/ says whether that was a transcript with no answer in it," >&2
  echo "or nothing at all." >&2
  exit 7
}

# Whether this continues a session is run-stage.sh's decision, made under its own lock:
# made here it would be a guess with a window in it, since another writing stage can
# finish between deciding and locking (#292). --resume says what this caller intends; for
# an implementing role run-stage.sh settles it against the record of who holds the tree.
first="--resume"

say "implement: $implementer"
run_stage implement "$implementer" "$change" $first; rc=$?
ask_stop "implement ($implementer)" implement
no_account_stop "$rc" "the implementer"
[ "$rc" -eq 0 ] || { echo "implement failed; stopping." >&2; exit 1; }

round=1
while :; do
  # The tree about to be handed to the reviewer, as the stage that produced it measured it.
  # Refusing rather than defaulting: an unreadable digest would silently disable both the
  # repeated-tree stop below and the TREE_SHA256 the landing gate demands.
  tree=$(tree_printed)
  [ -n "$tree" ] || {
    echo "the last stage printed no tree digest, so the review cannot be bound to a tree." >&2
    echo "run-stage.sh prints one line reading 'tree: <64 hex>'; this run saw none." >&2
    exit 1; }

  # Bytes a previous round already judged are not worth a second review, and a second
  # verdict on them is not worth trusting: during #291 two rounds returned opposite
  # verdicts on an identical tree, and the second one shipped. run-stage.sh already warns
  # that a handover fix round changed nothing; that warning is what this flapped past, so
  # this stops instead, and stops BEFORE the launch, because the waste is the review.
  prev=$(last_round_tree)
  if [ -n "$prev" ] && [ "$prev" = "$tree" ]; then
    prev_file=$(last_round_file)
    next_file=$(next_round_file "$wt/openspec/changes/$change" diff-review)
    say "the tree has not moved since $prev_file"
    echo "$prev_file recorded TREE_SHA256: $tree, and that is still the tree." >&2
    echo "So $next_file would be a second verdict on bytes already judged; not launching one." >&2
    echo "Read $prev_file: either every finding was answered in prose, which a person must" >&2
    echo "accept, or none was acted on, which is a fix round to re-run." >&2
    exit 10
  fi

  say "review $round: $reviewer"
  "$stage" review "$reviewer" "$change"
  rc=$?
  ask_stop "review $round ($reviewer)" review
  review_log="$wt/.agent-runs/review-$reviewer.log"
  [ "$rc" -eq 5 ] && { echo "the reviewer edited files; its verdict cannot be trusted." >&2; exit 5; }
  # Checked before anything is copied or read: an unreadable stage must not become the
  # round artifact, and must not be mistaken for a verdict.
  no_account_stop "$rc" "the reviewer"
  # Any other non-zero exit is a review that did not finish. A CLI can print a verdict
  # and then die, and reading that verdict would record an approval nobody stands behind.
  [ "$rc" -ne 0 ] && { echo "the reviewer exited $rc; its verdict cannot be trusted." >&2; exit 1; }

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
  # The tree this round judged, kept with the round that judged it. Without it the folder
  # holds a stack of verdicts and no way to tell which of them, if any, describes the diff
  # that shipped (#299).
  { printf 'TREE_SHA256: %s\n\n' "$tree"
    cat "$review_log" 2>/dev/null
  } > "$wt/openspec/changes/$change/$round_file"

  case "$verdict" in
    APPROVE)
      dr="$wt/openspec/changes/$change/diff-review.md"
      # Every agent that changed the tree, in the order they first wrote, read from the
      # ledger run-stage.sh keeps. Not "$implementer": that names the last stage to run,
      # which during #291 attributed six rounds of another agent's work to an agent that
      # wrote none of it. An empty list is written as an empty list and refused by the
      # landing gate; there is no default, because a default is the same silent pass.
      authors=$(paste -sd, "$wt/openspec/changes/$change/authors" 2>/dev/null | sed 's/,/, /g')
      [ -n "$authors" ] || {
        echo "warning: no implement or gate-fix stage changed this worktree, so nothing" >&2
        echo "claims authorship of the code. The landing gate refuses an empty AUTHORS:" >&2
        echo "line; whoever finishes this change writes it by hand." >&2; }
      {
        printf '# Diff review\n\n'
        printf 'AUTHORS: %s\n' "$authors"
        printf 'REVIEWER: %s\n' "$reviewer"
        printf 'VERDICT: APPROVE\n'
        printf 'ROUNDS: %s\n' "$round_no"
        # The tree this approval covers. Checked for shape at landing and never against the
        # committed tree: archive, the gate fix and the commit message all write after this
        # point, so the committed tree is never the reviewed one.
        printf 'TREE_SHA256: %s\n' "$tree"
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
  run_stage implement "$implementer" "$change" --resume \
    "Review findings on your implementation. They are in openspec/changes/$change/$round_file, relative to your worktree root. Read that file first; it is the whole review, and fixing every finding is the task."
  rc=$?
  ask_stop "fix round $round ($implementer)" implement
  no_account_stop "$rc" "the implementer"
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
