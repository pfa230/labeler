#!/usr/bin/env bash
# One round of code review, and the one place that writes its record (#328).
#
# Sourced, never run. apply.sh uses it for the review loop that follows implementation;
# run-change.sh uses it for the single round that follows a gate fix. Two callers, one
# implementation, because the round artifact and diff-review.md are what the landing gate
# reads: a second copy of this code is a second set of fields for that gate to disagree
# with.
#
# What the caller must already have defined:
#   $wt      the worktree
#   $stage   the path to run-stage.sh
#   say                 the caller's own progress banner
#   ask_stop            <label> <role>, the question protocol's stop
# Both exist in both callers; they are their conventions, not this file's, so they stay
# there. no_account_stop is defined here instead, because what it says is about a stage's
# result rather than about either caller's loop.
#
# The change directory is a parameter rather than derived, because the two callers are on
# opposite sides of archive's move: apply.sh names a live folder under openspec/changes/,
# and run-change.sh names the archived one.

# Where specs-digest.sh is, resolved from this file rather than the caller's $here: both
# callers happen to define that variable as the same directory, and depending on it would
# make this file break the day one of them does not.
review_round_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

# run-stage.sh exits 7 when no answer could be read out of what the agent printed, and it
# does so for a writing role as readily as for a review (#315). One spelling for both roles
# and both callers, because it is one failure: the reviewer's used to say "produced a
# transcript", which is only half of it - an agent that printed nothing produced no
# transcript either, and that is the half that destroyed a run's record during #287.
# Carried out with its own status rather than folded into the generic stop, so a person is
# not sent looking for a failure the agent never reported.
no_account_stop() { # no_account_stop <rc> <who>
  [ "$1" -eq 7 ] || return 0
  echo "$2 produced no readable result; stopping." >&2
  echo "Its log under .agent-runs/ says whether that was a transcript with no answer in it," >&2
  echo "or nothing at all." >&2
  exit 7
}

# The first free index, so a re-run after a stop adds a round rather than overwriting
# the record of an earlier one.
next_round_file() { # next_round_file <dir> <prefix> -> basename
  local dir="$1" pre="$2" i=1
  while [ -e "$dir/$pre-$i.md" ]; do i=$((i + 1)); done
  printf '%s-%s.md' "$pre" "$i"
}

# Run one review stage and record what it said. Sets REVIEW_VERDICT (APPROVE or REVISE),
# REVIEW_ROUND_FILE (the artifact's basename), REVIEW_ROUND_NO and REVIEW_LOG.
#
# Every way a review can fail to be a review exits from here, because each of them means
# the same thing to both callers: a verdict nothing should act on. Only APPROVE and
# REVISE return.
review_round() { # review_round <change-dir> <reviewer> <change> <tree> <banner>
  local change_dir="$1" reviewer="$2" change="$3" tree="$4" banner="$5" rc

  say "$banner"
  "$stage" review "$reviewer" "$change"
  rc=$?
  ask_stop "$banner" review
  REVIEW_LOG="$wt/.agent-runs/review-$reviewer.log"
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
  REVIEW_VERDICT=$(tail -40 "$REVIEW_LOG" 2>/dev/null \
    | grep -E '^VERDICT:[[:space:]]*(APPROVE|REVISE)[[:space:]]*$' \
    | tail -1 | sed 's/^VERDICT:[[:space:]]*//' | tr -d '[:space:]')
  # Every round is preserved, the approving one included. The gate reads
  # diff-review.md, so a verdict that exists only in an untracked log is a verdict
  # nothing can check (#223).
  REVIEW_ROUND_FILE=$(next_round_file "$change_dir" diff-review)
  # The canonical count is the artifact's index, not the caller's loop counter: a
  # restart begins at 1 while the file it writes is diff-review-4.md.
  REVIEW_ROUND_NO=$(printf '%s' "$REVIEW_ROUND_FILE" | sed 's/[^0-9]//g')
  # The tree this round judged, kept with the round that judged it. Without it the folder
  # holds a stack of verdicts and no way to tell which of them, if any, describes the diff
  # that shipped (#299).
  { printf 'TREE_SHA256: %s\n\n' "$tree"
    cat "$REVIEW_LOG" 2>/dev/null
  } > "$change_dir/$REVIEW_ROUND_FILE"

  case "$REVIEW_VERDICT" in
    APPROVE|REVISE) return 0 ;;
    *)
      echo "no readable VERDICT line in $REVIEW_LOG (found '${REVIEW_VERDICT:-none}')." >&2
      echo "Refusing to guess whether the review passed." >&2
      exit 4 ;;
  esac
}

# The approving verdict, in the file the landing gate reads. Overwrites whatever stood
# there: a later approval supersedes an earlier one, and the rounds it supersedes are
# still on disk beside it under their own digests.
write_diff_review() { # write_diff_review <change-dir> <reviewer> <tree> <round-no> <log>
  local change_dir="$1" reviewer="$2" tree="$3" round_no="$4" log="$5" authors
  # Every agent that changed the tree, in the order they first wrote, read from the
  # ledger run-stage.sh keeps. Not the implementer this invocation was given: that names
  # the last stage to run, which during #291 attributed six rounds of another agent's
  # work to an agent that wrote none of it. An empty list is written as an empty list and
  # refused by the landing gate; there is no default, because a default is the same
  # silent pass.
  authors=$(paste -sd, "$change_dir/authors" 2>/dev/null | sed 's/,/, /g')
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
    # The tree this approval covers. Checked for shape at landing, and against
    # gate-fix.tree when a gate fix wrote one: archive and the commit message also write
    # after this point, so the committed tree is never the reviewed tree, but the one
    # stage that edits code after it is now measured (#328).
    printf 'TREE_SHA256: %s\n' "$tree"
    # The contract this code was approved against. A later plan revision changes it,
    # and whoever reads this verdict then knows the approval no longer covers what
    # is in the folder: run-change.sh retires it on exactly that comparison.
    printf 'SPECS_SHA256: %s\n\n' "$("$review_round_dir/specs-digest.sh" "$change_dir" 2>/dev/null)"
    # The body's own verdict line is dropped: the canonical one is above, and the
    # gate refuses a file carrying two.
    grep -v '^VERDICT:' "$log"
  } > "$change_dir/diff-review.md"
}
