#!/usr/bin/env bash
# Run one accepted issue end to end, on four named agents (#283).
#
#   .workflow/run-change.sh <issue#> [<planner> <plan-reviewer> <implementer> <code-reviewer>] \
#                           [--rounds N] [--dry-run]
#   .workflow/run-change.sh 283 claude codex agy codex
#   .workflow/run-change.sh 283                  # the four roles from .workflow/roles.local
#
# Worktree, plan, plan review, implementation, diff review, archive, the gates and
# the commit, printing the merge commands and stopping. It never merges to main: that
# is the one step a person approves, and by then it is mechanical.
#
# The four agents are named first because the pairing is the guarantee, exactly as in
# apply.sh: the model that writes a plan is never the one that judges it, and neither
# is the pair that writes and judges the code.
#
# STATE IS THE ARTIFACTS, NEVER A LEDGER. Which stage runs next is read off the change
# folder: no folder means propose; a folder whose plan review does not pass the gate
# means review; no passing diff-review.md means apply; a folder still outside archive/
# means archive. So a re-run after any stop resumes instead of redoing, and the script
# cannot believe a stage happened that did not. It is the rule the gates already
# follow: inspect files, never trust a record of who did what.
#
# Exit codes:
#   0  committed, and the merge commands are printed
#   1  a stage failed
#   2  bad arguments
#   6  a loop hit its round cap; the findings want a person, not another round
#   8  a stage wrote QUESTIONS.md and stopped rather than guess. Answer them in
#      ANSWERS.md at the worktree root and re-run; every stage reads that file
#  11  the gate fix edited code, and the review of that edit came back REVISE (#328).
#      A gate fix is one unattended round on a lint; findings against it are a defect,
#      which is what the second gate failure above already stops for
#   3, 4, 5, 7, 10  passed through from apply.sh; see its header. 10 is the one worth
#      knowing here: the fix round left the tree byte-identical to what the previous
#      round judged, so no second verdict on the same bytes was launched (#299)
set -uo pipefail

usage='usage: run-change.sh <issue#> [<planner> <plan-reviewer> <implementer> <code-reviewer>] [--rounds N] [--dry-run]'
here=$(cd "$(dirname "$0")" && pwd)
source "$here/agents.sh"
source "$here/questions.sh"
source "$here/gates.sh"
# next_round_file, no_account_stop, and the review round the gate fix now goes through
# (#328). Shared with apply.sh, so both callers write one shape of round artifact.
source "$here/review-round.sh"

issue=""; planner=""; plan_reviewer=""; implementer=""; code_reviewer=""
max_rounds=3; dry_run=0
positional=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --rounds) max_rounds="${2:?--rounds needs a number}"; shift 2 ;;
    --rounds=*) max_rounds="${1#*=}"; shift ;;
    --dry-run) dry_run=1; shift ;;
    -h|--help) echo "$usage"; exit 0 ;;
    -*) echo "unknown option: $1" >&2; echo "$usage" >&2; exit 2 ;;
    *)
      case "$positional" in
        0) issue="$1" ;;
        1) planner="$1" ;;
        2) plan_reviewer="$1" ;;
        3) implementer="$1" ;;
        4) code_reviewer="$1" ;;
        *) echo "too many arguments: $1" >&2; echo "$usage" >&2; exit 2 ;;
      esac
      positional=$((positional + 1)); shift ;;
  esac
done

# All four agents, or none of them (#330). Two roles from the command line and two from
# a file is a half-configured lineup nobody can reason about, so a partial one is
# refused rather than merged: what is on the command line does not top up what is in the
# file, it replaces it entirely or is absent entirely.
#
# roles_from is what every validation below blames. Empty means the command line, and a
# path means the file: a value that came from a file is not fixed by reading a synopsis
# of arguments nobody typed, and printing one sends the reader to the wrong place.
roles_from=""
role_blame() { # role_blame <key>
  if [ -n "$roles_from" ]; then printf '%s, key %s' "$roles_from" "$1"
  else printf 'the command line'; fi
}
role_stop() { # role_stop <key> - the closing line of a failed role validation
  if [ -n "$roles_from" ]; then echo "Fix '$1' in $roles_from." >&2
  else echo "$usage" >&2; fi
}
case "$positional" in
  5) ;;
  1) roles_from=$(roles_path "$here")
     roles_load "$roles_from" || exit 2
     planner="$ROLE_PLANNER"; plan_reviewer="$ROLE_PLAN_REVIEWER"
     implementer="$ROLE_IMPLEMENTER"; code_reviewer="$ROLE_CODE_REVIEWER" ;;
  0) echo "$usage" >&2; exit 2 ;;
  *) echo "name all four agents or none: $((positional - 1)) named." >&2
     echo "Filling some roles here and the rest from $(roles_path "$here") is a" >&2
     echo "half-configured lineup, so it is refused rather than merged." >&2
     echo "$usage" >&2; exit 2 ;;
esac
case "$issue" in ''|*[!0-9]*) echo "the first argument is the issue number, got '$issue'" >&2; exit 2 ;; esac
case "$max_rounds" in ''|*[!0-9]*) echo "--rounds takes a number, got '$max_rounds'" >&2; exit 2 ;; esac
[ "$max_rounds" -ge 1 ] || { echo "--rounds must be at least 1" >&2; exit 2; }

for pair in "planner:$planner" "plan-reviewer:$plan_reviewer" \
            "implementer:$implementer" "code-reviewer:$code_reviewer"; do
  agent_known "${pair#*:}" || {
    echo "unknown agent '${pair#*:}', from $(role_blame "${pair%%:*}")." >&2
    echo "The agents are claude, agy, codex and opencode." >&2
    role_stop "${pair%%:*}"; exit 2; }
done

# Both pairs, because the gate refuses either self-review at commit time, and finding
# that out after four agent runs is finding it out too late.
if [ "$planner" = "$plan_reviewer" ]; then
  echo "planner and plan reviewer must differ: both are '$planner'." >&2
  echo "Nobody reviews their own plan." >&2
  role_stop plan-reviewer; exit 2
fi
if [ "$implementer" = "$code_reviewer" ]; then
  echo "implementer and code reviewer must differ: both are '$implementer'." >&2
  echo "Nobody reviews their own code." >&2
  role_stop code-reviewer; exit 2
fi
# Both authors must be resumable. Every loop here returns findings to whoever wrote the
# thing, so an author whose session cannot be continued either starts over or stops at
# the first REVISE; refused up front rather than four agent runs later.
for pair in "planner:$planner" "implementer:$implementer"; do
  agent_resumable "${pair#*:}" || {
    echo "${pair#*:} cannot be resumed, so it cannot be the ${pair%%:*} here." >&2
    echo "Findings go back to the author, which continues the session that wrote the work." >&2
    echo "It is named in $(role_blame "${pair%%:*}")." >&2
    role_stop "${pair%%:*}"; exit 2; }
done

[ -z "$roles_from" ] || printf 'roles: %s %s %s %s (from %s)\n' \
  "$planner" "$plan_reviewer" "$implementer" "$code_reviewer" "$roles_from"

common=$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null) || { echo "not in a git repo" >&2; exit 2; }
root=$(dirname "$common")
wt="$root/.worktrees/issue-$issue"


# --- state, read off the artifacts ------------------------------------------------
live_change() { # the one live change folder in this worktree, or nothing
  local d n=""
  for d in "$wt"/openspec/changes/*/; do
    [ -d "$d" ] || continue
    case "$(basename "$d")" in archive) continue ;; esac
    n="$(basename "$d")"
  done
  printf '%s' "$n"
}
archived_change() { # this issue's folder under archive/, or nothing
  local d n=""
  for d in "$wt"/openspec/changes/archive/*issue-"$issue"-*/; do
    [ -d "$d" ] && n="$(basename "$d")"
  done
  printf '%s' "$n"
}
# A change folder is not a proposal. propose can create the directory and then stop to
# ask, or fail, and reading "the folder exists" as "the plan is written" sends half a
# plan to review. The artifacts it must leave behind are what is checked, which is the
# same rule as everywhere else here: inspect the files.
# What the plan review reads: the schema has review requiring proposal, specs and
# design, and tasks requiring review. So tasks.md is deliberately NOT part of this:
# demanding it here would invert the schema's order and hand the reviewer a task list
# written for a plan it had not yet judged.
propose_complete() {
  local d="$wt/openspec/changes/${1:-}"
  [ -n "${1:-}" ] || return 1
  [ -f "$d/proposal.md" ] || return 1
  # Only behavior changes come through this loop, and a behavior change always carries a
  # delta; a change folder with no specs/ is an unfinished one. design.md is conditional
  # in the schema, so its absence is not evidence of anything.
  [ -n "$(find "$d/specs" -name '*.md' 2>/dev/null | head -1)" ] || return 1
  return 0
}
# The plan's state is whatever the gate says it is. Asking the gate rather than
# re-reading review.md here means one parser for the verdict and the digest, so this
# script cannot reach a different conclusion than the commit will.
plan_passes() { "$here/review-gate-check.sh" --plan-only "$wt" src/_probe >/dev/null 2>&1; }
# The recorded diff-review verdict, or nothing — and nothing also when that verdict no
# longer covers the contract. A code approval is against the delta specs as they stood;
# once they move, the approval is about something else. Checked here rather than only
# where the file is retired, so the state function itself cannot say "approved" about a
# verdict that has been superseded.
diff_verdict() {
  local f="$wt/openspec/changes/$1/diff-review.md" recorded current
  [ -f "$f" ] || return 0
  recorded=$(grep -m1 '^SPECS_SHA256:' "$f" | sed 's/^SPECS_SHA256:[[:space:]]*//' | tr -d '[:space:]')
  current=$("$here/specs-digest.sh" "$wt/openspec/changes/$1" 2>/dev/null)
  # No recorded digest means what it covered cannot be established; the safe answer to
  # "does this approval still apply?" is another review, not an assumption.
  [ -n "$recorded" ] && [ "$recorded" = "$current" ] || return 0
  grep -m1 '^VERDICT:' "$f" | sed 's/^VERDICT:[[:space:]]*//' | tr -d '[:space:]'
}
# The reviewer's final word ends its output, and a verdict quoted mid-prose never
# starts a line. Read from the closing lines only, for the reason apply.sh gives.
#
# Matched as a WHOLE line against the three verdicts, never as a prefix. A prefix match
# reads "VERDICT: APPROVE WITH CHANGES" - the reviewer spelling it with spaces instead
# of the underscore - as a plain APPROVE, and the driver then writes a canonical
# APPROVE that skips the required changes and that the commit gate accepts. Anything
# unrecognised yields nothing, which is the refusal below rather than a guess.
log_verdict() {
  tail -40 "$1" 2>/dev/null \
    | grep -E '^VERDICT:[[:space:]]*(APPROVE_WITH_CHANGES|APPROVE|REVISE)[[:space:]]*$' \
    | tail -1 | sed 's/^VERDICT:[[:space:]]*//' | tr -d '[:space:]'
}

# WHICH STAGE RUNS NEXT, and the only place that decides it. Every guard below asks this
# rather than re-deriving the same conditions, because three separate copies of "is the
# plan done?" is three chances for one of them to disagree - and the one that disagreed
# was the condition that skipped propose entirely for a brand-new issue, because the
# gate has no live change to refuse and so passes.
#
# --dry-run prints its answer, which is what makes the resumption logic testable without
# launching an agent.
next_stage() {
  [ -d "$wt" ] || { printf 'worktree'; return; }
  local c a
  a=$(archived_change)
  if [ -n "$a" ]; then printf 'gates'; return; fi
  c=$(live_change)
  propose_complete "$c" || { printf 'propose'; return; }
  plan_passes                                  || { printf 'plan-review'; return; }
  [ -f "$wt/openspec/changes/$c/tasks.md" ]    || { printf 'tasks'; return; }
  [ "$(diff_verdict "$c")" = "APPROVE" ]       || { printf 'apply'; return; }
  printf 'archive'
}

if [ "$dry_run" = "1" ]; then
  printf 'issue: %s\nplanner: %s\nplan-reviewer: %s\nimplementer: %s\ncode-reviewer: %s\nworktree: %s\nrounds: %s\nnext stage: %s\n' \
    "$issue" "$planner" "$plan_reviewer" "$implementer" "$code_reviewer" "$wt" "$max_rounds" "$(next_stage)"
  exit 0
fi

stage="$here/run-stage.sh"
say() { printf '\n== %s ==\n' "$1"; }

# A stage that stopped to ask stops the run with it. Checked before the exit status is
# judged: asking IS how that stage ended, and reporting it as a plain failure buries
# the question it wrote.
ask_stop() { # ask_stop <label> <role>
  questions_pending "$wt" || return 0
  questions_record "$wt" "$2"
  questions_report "$wt" "$1"
  exit 8
}

# stage_done <role> — the point at which an outstanding question is settled: the stage
# that asked has run again, with the answers in front of it.
stage_done() {
  [ "$force_stage" = "$1" ] || return 0
  questions_clear "$wt"
  force_stage=""
}

# --- stage 0: the worktree --------------------------------------------------------
# Created from the issue title, reused when it is already there. The branch name and
# the change name are one string on purpose: run-stage.sh derives the worktree path
# back out of it, so two spellings could not drift apart.
if [ ! -d "$wt" ]; then
  command -v gh >/dev/null 2>&1 || { echo "gh is not on PATH; it is needed to read issue #$issue" >&2; exit 2; }
  title=$(gh issue view "$issue" --json title -q .title 2>/dev/null) || { echo "cannot read issue #$issue" >&2; exit 2; }
  [ -n "$title" ] || { echo "issue #$issue has no title; is that the right number?" >&2; exit 2; }
  # BSD sed has no \+, so the repeated class is spelled out.
  slug=$(printf '%s' "$title" | tr '[:upper:]' '[:lower:]' \
    | sed 's/[^a-z0-9][^a-z0-9]*/-/g; s/^-//; s/-$//' | cut -c1-40 | sed 's/-$//')
  [ -n "$slug" ] || slug="change"
  branch="issue-$issue-$slug"
  say "worktree: $branch"
  git -C "$root" worktree add "$wt" -b "$branch" || { echo "could not create the worktree" >&2; exit 1; }
else
  branch=$(git -C "$wt" rev-parse --abbrev-ref HEAD) || { echo "cannot read the branch in $wt" >&2; exit 2; }
fi
case "$branch" in issue-"$issue"-*) ;; *)
  echo "$wt is on branch '$branch', which is not this issue's. Never carry one change's worktree into another's work." >&2
  exit 2 ;;
esac

# The issue body IS the scope: /change refines it with the user, and the planner works
# from it. Handed over as a file rather than on the command line, because a body of any
# size on a command line is how a run died with "Argument list too long" (#264).
# Refreshed every run, so an edited issue reaches the next stage.
mkdir -p "$wt/.agent-runs"
scope="$wt/.agent-runs/issue-$issue.md"
if command -v gh >/dev/null 2>&1 && body=$(gh issue view "$issue" --json title,body -q '"# " + .title + "\n\n" + .body' 2>/dev/null) && [ -n "$body" ]; then
  printf '%s\n' "$body" > "$scope"
elif [ ! -s "$scope" ]; then
  # No silent fallback: a planner given no scope writes a plan for a title.
  echo "cannot read issue #$issue and no scope is cached at $scope." >&2
  echo "The issue body is what the planner works from; refusing to plan without it." >&2
  exit 2
else
  echo "note: could not refresh issue #$issue; using the scope cached at $scope." >&2
fi

# A question outstanding from an earlier run names the stage that asked, and this runs
# below say() and below the worktree because it needs both.
force_stage=""
if questions_outstanding "$wt"; then
  asker=$(questions_asker "$wt")
  if [ -s "$(answers_file "$wt")" ]; then
    force_stage="$asker"
    say "answered: re-running the $asker stage that asked"
    # NOT cleared here. Clearing on entry loses the record if the forced stage fails or
    # is never reached, and the next run would have no idea a question was outstanding.
    # stage_done clears it once that stage has actually run.
  else
    echo "an earlier run stopped on a question from the $asker stage, and there are no" >&2
    echo "answers in $(answers_file "$wt")." >&2
    sed 's/^/  /' "$(pending_file "$wt")" >&2
    echo "Answer it there and re-run." >&2
    exit 8
  fi
fi

change=$(live_change)
archived=$(archived_change)

# A review.md this driver did not write is not a review. It writes review.md only
# alongside a review-<n>.md round artifact, so review.md without one came from somewhere
# else - most likely a propose workflow that walked its whole dependency closure and
# reviewed its own plan. The gate would accept that: AUTHOR and REVIEWER are whatever it
# typed. Parked before next_stage reads it, because a self-review that passes the gate
# skips both propose and the plan review.
if [ -n "$change" ] && [ -f "$wt/openspec/changes/$change/review.md" ] \
   && [ -z "$(find "$wt/openspec/changes/$change" -maxdepth 1 -name 'review-*.md' 2>/dev/null | head -1)" ]; then
  mv "$wt/openspec/changes/$change/review.md" \
     "$wt/.agent-runs/unattributed-review-$(date -u +%Y%m%dT%H%M%SZ).md" 2>/dev/null
  echo "note: parked a review.md with no round artifact beside it; this loop writes its own." >&2
fi

# --- stage 1: the plan ------------------------------------------------------------
# The author resumes, the reviewer never does. The author must keep what it built; a
# resumed reviewer judges the delta since its own last message rather than the artifact
# in front of it, so a regression the fix round introduced outside its findings is
# never examined, and it is anchored on its own prior verdict besides.
plan_ran=0
case "$(next_stage)" in propose|plan-review) run_plan=1 ;; *) run_plan=0 ;; esac
if [ "$run_plan" = "1" ]; then
  plan_ran=1
  round=1
  while :; do
    if ! propose_complete "$change" || [ "$force_stage" = "propose" ]; then
      [ -n "$change" ] || change="$branch"
      # Resumed when a half-written plan is already there: that plan was written by this
      # planner's session, and continuing it beats a second planner starting over it.
      presume=""
      [ -s "$wt/.agent-runs/plan-$planner.conversation" ] && presume="--resume"
      say "propose: $planner"
      "$stage" propose "$planner" "$change" $presume \
        "The issue this change implements is in .agent-runs/issue-$issue.md, relative to your worktree root. That is the scope: read it first, and plan what it asks for and nothing else."; rc=$?
      ask_stop "propose ($planner)" propose
      [ "$rc" -eq 0 ] || { echo "propose failed; stopping." >&2; exit 1; }
      change=$(live_change)
      [ -n "$change" ] || { echo "propose wrote no change folder in $wt. It did not run." >&2; exit 1; }
      case "$change" in issue-"$issue"-*) ;; *)
        echo "propose named the change '$change', which is not issue-$issue-<slug>." >&2
        echo "run-stage.sh finds the worktree by that prefix; rename it or start over." >&2
        exit 1 ;;
      esac
      propose_complete "$change" || {
        echo "propose left an incomplete change in $change: it needs proposal.md and at" >&2
        echo "least one delta spec under specs/. Not sending half a plan to review." >&2
        exit 1; }
      stage_done propose
      # The same parking as at entry, because propose has just run and may have written
      # one. Keyed the same way: a review.md with no round artifact beside it is not a
      # review this loop performed.
      planner_review="$wt/openspec/changes/$change/review.md"
      [ -f "$planner_review" ] \
        && [ -z "$(find "$wt/openspec/changes/$change" -maxdepth 1 -name 'review-*.md' 2>/dev/null | head -1)" ] \
        && mv "$planner_review" \
        "$wt/.agent-runs/planner-wrote-review-$(date -u +%Y%m%dT%H%M%SZ).md" 2>/dev/null
    fi

    say "plan review $round: $plan_reviewer"
    "$stage" plan-review "$plan_reviewer" "$change"; prc=$?
    ask_stop "plan review $round ($plan_reviewer)" plan-review
    review_log="$wt/.agent-runs/plan-review-$plan_reviewer.log"
    [ "$prc" -eq 5 ] && { echo "the plan reviewer edited files; its verdict cannot be trusted." >&2; exit 5; }
    # 7 covers both shapes of an unreadable stage (#315): a transcript with no answer in
    # it, and an agent that printed nothing at all. plan-review-$plan_reviewer.log says which.
    [ "$prc" -eq 7 ] && { echo "the plan reviewer produced no readable review; stopping." >&2; exit 7; }
    # Any other non-zero exit is a review that did not finish. A CLI can print a verdict
    # and then die, and reading it would record an approval nobody stands behind.
    [ "$prc" -ne 0 ] && { echo "the plan reviewer exited $prc; its verdict cannot be trusted." >&2; exit 1; }

    verdict=$(log_verdict "$review_log")
    dir="$wt/openspec/changes/$change"
    round_file=$(next_round_file "$dir" review)
    # The canonical count is the artifact's index, not this invocation's loop counter: a
    # restart begins at 1 while the file it writes is review-4.md.
    round_no=$(printf '%s' "$round_file" | sed 's/[^0-9]//g')
    cp "$review_log" "$dir/$round_file" 2>/dev/null || true

    # review.md is built here rather than by the reviewer, for the reason apply.sh
    # builds diff-review.md: the canonical fields are the gate's contract, and an agent
    # asked to fill them in is an agent that can fill them in wrong. Its own words are
    # the body; only the field lines are stripped, because the gate refuses a file
    # carrying two of any of them.
    write_review() { # write_review <verdict>
      {
        printf '# Plan review\n\n'
        printf 'AUTHOR: %s\n' "$planner"
        printf 'REVIEWER: %s\n' "$plan_reviewer"
        printf 'VERDICT: %s\n' "$1"
        printf 'ROUNDS: %s\n\n' "$round_no"
        grep -v '^VERDICT:\|^AUTHOR:\|^REVIEWER:\|^CHANGES_APPLIED:\|^SPECS_SHA256:' "$review_log"
      } > "$dir/review.md"
    }

    case "$verdict" in
      APPROVE)
        stage_done plan-review
        write_review APPROVE
        "$here/specs-digest.sh" "$dir" --write || exit 1
        say "plan APPROVE after $round round(s)"
        break ;;
      APPROVE_WITH_CHANGES)
        stage_done plan-review
        # This verdict ENDS the loop: the author applies the listed changes and nobody
        # reviews them again, which is why the reviewer is told to file anything it
        # cannot state completely as REVISE instead. The digest is written after those
        # edits, so it covers the contract that will actually be built.
        write_review APPROVE_WITH_CHANGES
        say "plan APPROVE_WITH_CHANGES: $planner applies them"
        "$stage" propose "$planner" "$change" --resume \
          "Apply the required changes. The plan review is in openspec/changes/$change/review.md, relative to your worktree root. Apply every change listed under its Required changes heading, and nothing else. Do not edit review.md itself."; rc=$?
        ask_stop "required changes ($planner)" propose
        [ "$rc" -eq 0 ] || { echo "applying the required changes failed; stopping." >&2; exit 1; }
        printf 'CHANGES_APPLIED: yes\n' >> "$dir/review.md"
        "$here/specs-digest.sh" "$dir" --write || exit 1
        break ;;
      REVISE) ;;
      *)
        echo "no readable VERDICT line in $review_log (found '${verdict:-none}')." >&2
        echo "Refusing to guess whether the plan review passed." >&2
        exit 4 ;;
    esac

    # The findings go to the author BEFORE the cap is weighed. Checking the cap first
    # means the last REVISE is never acted on, so a re-run re-reviews the same unchanged
    # artifacts, gets the same verdict, and stops again: a loop that cannot make
    # progress no matter how many times a person restarts it.
    say "plan revision $round: $planner"
    "$stage" propose "$planner" "$change" --resume \
      "Revise the plan. The review findings are in openspec/changes/$change/$round_file, relative to your worktree root. Read that file first; it is the whole review, and addressing every finding is the task. Do not edit it."; rc=$?
    ask_stop "plan revision $round ($planner)" propose
    [ "$rc" -eq 0 ] || { echo "the plan revision failed; stopping." >&2; exit 1; }

    if [ "$round" -ge "$max_rounds" ]; then
      say "plan still REVISE after $max_rounds round(s)"
      echo "Stopping rather than looping. The findings are in $review_log, and the planner has" >&2
      echo "acted on them; a plan that cannot converge in $max_rounds rounds wants a person." >&2
      echo "Re-running reviews what the planner just fixed, so a restart makes progress." >&2
      exit 6
    fi
    round=$((round + 1))
  done
  plan_passes || { echo "the plan review still does not pass the gate; stopping." >&2; exit 1; }
fi

# On a re-run that finds the change already archived, the live folder is gone and the
# name has to come back out of the archived one: the stages after archive still take it.
if [ -z "$change" ]; then
  change=$(live_change)
  [ -n "$change" ] || change=$(printf '%s' "$archived" | sed 's/^[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]-//')
fi
[ -n "$change" ] || { echo "no change found in $wt, live or archived; nothing to run." >&2; exit 1; }

# --- stage 1b: the task list ------------------------------------------------------
# After the plan review, never before it: the schema has tasks requiring review, and a
# task list written for a plan the reviewer then sent back describes work nobody
# approved. Rewritten whenever the plan moved in this run, for the same reason.
tasks_md="$wt/openspec/changes/$change/tasks.md"
# next_stage says tasks when the file is missing. plan_ran forces a rewrite because the
# plan moved in this run, and force_stage because this stage asked and has been answered.
if [ "$(next_stage)" = "tasks" ] || [ "$plan_ran" = "1" ] || [ "$force_stage" = "tasks" ]; then
  say "tasks: $planner"
  tresume=""
  [ -s "$wt/.agent-runs/plan-$planner.conversation" ] && tresume="--resume"
  # This file specifically, not "did the worktree change": a stage that left a stale task
  # list and edited something else would otherwise pass as having rewritten it.
  before_tasks=$(cksum < "$tasks_md" 2>/dev/null || echo none)
  "$stage" tasks "$planner" "$change" $tresume; rc=$?
  ask_stop "tasks ($planner)" tasks
  [ "$rc" -eq 0 ] || { echo "writing the task list failed; stopping." >&2; exit 1; }
  [ -f "$tasks_md" ] || { echo "the tasks stage wrote no tasks.md. It did not run." >&2; exit 1; }
  if [ "$plan_ran" = "1" ] && [ "$before_tasks" = "$(cksum < "$tasks_md" 2>/dev/null || echo none)" ]; then
    echo "the plan changed in this run but tasks.md did not. The task list describes a plan" >&2
    echo "that no longer exists; refusing to implement from it." >&2
    exit 1
  fi
  stage_done tasks
fi

# A code approval covers the contract it was given. Judged on that contract having moved,
# never on the plan loop having run: a loop that ran only to record a missing digest
# changed nothing, and retiring a valid verdict costs a whole implementation round.
# Without this the state reads "diff review approved" and jumps to archive, landing code
# written for a superseded contract. The round artifacts stay; only the verdict the gate
# reads is retired, which puts the state back at apply.
# diff_verdict already refuses a superseded approval, so the state is right whatever
# happens here. The file still has to GO, because the commit gate reads diff-review.md
# directly and an approved one left in the folder would satisfy it at landing time.
dr="$wt/openspec/changes/$change/diff-review.md"
if [ -f "$dr" ]; then
  if [ -z "$(diff_verdict "$change")" ]; then
    mv "$dr" "$wt/.agent-runs/superseded-diff-review-$(date -u +%Y%m%dT%H%M%SZ).md" || {
      echo "could not retire the superseded diff review at $dr." >&2
      echo "Leaving it would send this run to archive with code approved against an older" >&2
      echo "contract; stopping instead." >&2
      exit 1; }
    echo "note: the delta specs have changed since the code was approved, so that approval" >&2
    echo "no longer applies and the implementation is reviewed again." >&2
  fi
fi

# --- stage 2: the code ------------------------------------------------------------
# apply.sh unchanged: it already is this loop, for the implementation diff.
if [ "$(next_stage)" = "apply" ]; then
  say "apply: $implementer, reviewed by $code_reviewer"
  "$here/apply.sh" "$implementer" "$code_reviewer" "$change" --rounds "$max_rounds"; rc=$?
  [ "$rc" -eq 0 ] || exit "$rc"
  # apply.sh runs both of these inside itself, so either may have been the one that asked.
  stage_done implement
  stage_done review
fi

# --- stage 3: archive -------------------------------------------------------------
# On the planner, resuming the session that wrote the deltas: run-stage.sh gives
# propose and archive one conversation slot for exactly this.
if [ "$(next_stage)" = "archive" ]; then
  say "archive: $planner"
  resume=""
  [ -s "$wt/.agent-runs/plan-$planner.conversation" ] && resume="--resume"
  "$stage" archive "$planner" "$change" $resume; rc=$?
  ask_stop "archive ($planner)" archive
  [ "$rc" -eq 0 ] || { echo "archive failed; stopping." >&2; exit 1; }
  stage_done archive
  archived=$(archived_change)
  [ -n "$archived" ] || { echo "archive left no folder under openspec/changes/archive/. It did not run." >&2; exit 1; }
fi

# --- stage 4: the gates -----------------------------------------------------------
# The three CI runs, so that a pass here and a pass there mean the same thing. One fix
# round, because a lint is exactly what an unattended round should absorb; a second
# failure is a real defect and wants a person.
#
# A failure that fails identically at the base commit is neither, and that was the gap
# (#298): the driver read every non-zero as this change's, so on a machine whose suite does
# not pass at HEAD it could never finish. gates.sh answers that question, and only on the
# failing path.
gates_log="$wt/.agent-runs/gates.log"
gates_base_log="$wt/.agent-runs/gates-base.log"
# Every command here is read-only, and fmt is the one that had to be made so: it ran in
# rewrite mode, after the diff review had approved the tree and before the commit, so a
# formatter change landed having been reviewed by nobody (#326). Check mode reports
# instead, which is also what CI runs, so a pass here and a pass there compare the same
# bytes. Repairing it is the gate-fix round's job, which is what that round is for.
run_gates() {
  (
    cd "$wt" || exit 1
    cargo fmt --check || exit "$GATE_FMT_FAILED"
    cargo clippy --all-targets --all-features -- -D warnings || exit "$GATE_CLIPPY_FAILED"
    # --no-fail-fast because a partial failure set is worse than none here: cargo stops
    # after the first failing test binary, so a regression in a later one would sit behind
    # a pre-existing failure in an earlier one and be subtracted away unseen.
    cargo test --no-fail-fast || exit "$GATE_TEST_FAILED"
    # The ui gate runs only when this change touches ui/. Most changes here are Rust or
    # harness, and ui/node_modules is gitignored so a fresh worktree has none; running it
    # unconditionally would charge every change an npm ci (#354). Decided from both the
    # committed range against the base and the uncommitted tree.
    if gate_ui_touches "$wt"; then
      if ! command -v npm >/dev/null 2>&1; then
        echo "ui gate: npm is not on PATH; install Node (see .nvmrc) and run 'npm ci' in ui/" >&2
        exit "$GATE_UI_FAILED"
      fi
      if [ ! -d "ui/node_modules" ]; then
        echo "ui gate: ui/node_modules is missing; running 'npm ci' in ui/" >&2
        (cd ui && npm ci) || {
          echo "ui gate: npm ci failed; run 'npm ci' in ui/ and retry" >&2
          exit "$GATE_UI_FAILED"
        }
      fi
      (cd ui && npm run lint) || exit "$GATE_UI_FAILED"
      (cd ui && npm test) || exit "$GATE_UI_FAILED"
    fi
  ) > "$gates_log" 2>&1
}
# One attempt at the gates and what its failure means. Zero when this change is clear:
# everything passed, or every failure fails identically at the base commit and is not this
# change's to fix. Anything gate_attribute cannot tell apart counts as this change's.
gates_clear() {
  run_gates; local rc=$?
  [ "$rc" -eq 0 ] && return 0
  tail -30 "$gates_log"
  gate_attribute "$wt" "$rc" "$gates_log" "$gates_base_log"
}
# An answered question whose stage never ran again. It happens when that stage had
# already produced its output before it asked, so the state moved past it: archive with
# the folder already moved, or a gate fix with the gates now passing. Nothing here can
# apply that answer, and clearing the record would bury it, so this stops instead.
# gate-fix and commit-msg are still ahead of this point, so an answer for either is not
# stranded yet; every other stage is behind it and will not run again.
stranded() {
  echo "the $force_stage stage asked a question, and it has been answered, but that stage" >&2
  echo "had already finished its work, so it did not run again and nothing applied the answer." >&2
  sed 's/^/  /' "$(pending_file "$wt")" 2>/dev/null >&2
  echo "Decide what it means for the work in $wt, then delete .agent-runs/pending-question." >&2
  exit 8
}
case "$force_stage" in
  ''|gate-fix|commit-msg) ;;
  *) stranded ;;
esac

# next_stage is the authority, so it is asked here too rather than trusted to have been
# satisfied by falling past three conditional guards. A writing stage can invalidate an
# earlier artifact - a tasks run that edits a delta spec voids the plan verdict - and
# without this the driver would commit and push work that was never implemented.
reached=$(next_stage)
[ "$reached" = "gates" ] || {
  echo "the driver reached the gates with the state reading '$reached'." >&2
  echo "Something undid an earlier stage; stopping rather than committing past it." >&2
  exit 1; }
# Where the change is now. Archive has moved it, and the three stages left - the gate
# fix, the review of that fix and the commit message - all write into it there.
change_dir="$wt/openspec/changes/archive/$(archived_change)"
[ -d "$change_dir" ] || { echo "no archived folder for $change in $wt; stopping." >&2; exit 1; }
gate_fix_tree="$change_dir/gate-fix.tree"

say "gates: fmt, clippy, test"
if ! gates_clear; then
  say "gates failed: $implementer gets one round"
  # Captured, because what this stage leaves behind is what the review below judges, and
  # run-stage.sh's stdout is the only place it is stated: the digest of the tree it
  # produced, and whether it produced one at all. stderr flows straight through.
  #
  # PIPESTATUS[0] rather than $?, for the reason apply.sh gives: pipefail reports tee's
  # failure as the pipeline's, and a full disk would then be read as a gate fix that
  # failed when it did not.
  gate_fix_out="$wt/.agent-runs/gate-fix-stage.out"
  # run-stage.sh decides whether this continues a session, under its own lock (#292).
  "$stage" gate-fix "$implementer" "$change" --resume | tee "$gate_fix_out"; rc=${PIPESTATUS[0]}
  ask_stop "gate fix ($implementer)" gate-fix
  [ "$rc" -eq 0 ] || { echo "the gate fix round failed; stopping." >&2; exit 1; }
  stage_done gate-fix
  # The edit this round made, recorded where it survives the run that made it. A stop
  # anywhere below - the second gate attempt, the review, a signal - leaves a re-run with
  # no memory of the launch, and .agent-runs is working state a broom carries off, so the
  # record goes in the change folder beside the author ledger and lands with it.
  #
  # Only when the round actually changed something. A gate fix that edited nothing, on
  # gates that then passed, added nothing for anyone to review.
  if grep -qx 'changed the worktree: yes' "$gate_fix_out"; then
    fixed_tree=$(grep -E '^tree: [0-9a-f]{64}$' "$gate_fix_out" | tail -1 | sed 's/^tree:[[:space:]]*//')
    [ -n "$fixed_tree" ] || {
      echo "the gate fix changed the worktree and printed no tree digest, so what it wrote" >&2
      echo "cannot be bound to a review. run-stage.sh prints one line reading 'tree: <64 hex>'." >&2
      exit 1; }
    printf '%s\n' "$fixed_tree" > "$gate_fix_tree" || {
      echo "cannot record the gate fix's tree at $gate_fix_tree; stopping." >&2
      echo "An unrecorded edit after the review is the one this records to prevent." >&2
      exit 1; }
  fi
  say "gates, again"
  if ! gates_clear; then
    echo >&2; echo "the gates still fail after one fix round, on something that is this change's." >&2
    echo "That is a defect, not a lint; stopping." >&2
    exit 1
  fi
fi

# --- stage 4b: the review of what the gate fix wrote ------------------------------
# The gate fix edits code after the diff review approved the tree and after archive, and
# it used to fall straight through to the commit: the approving diff-review.md described
# a tree that no longer existed (#328). Nothing could catch it, either - the landing check
# on TREE_SHA256 is shape-only on purpose - so the one stage that exists to make edits was
# the one stage whose edits nobody read. That contradicts the rule the whole loop is built
# on, and it contradicts the commit-message stage immediately below, which refuses to let
# an unreviewed edit through at all.
#
# Read off the artifacts, never off this run's control flow. The gate fix can have
# happened in an earlier invocation whose review then stopped; nested inside the branch
# that launched it, this check would never fire again on the re-run, which is the shape of
# bug it exists to close.
if [ -f "$gate_fix_tree" ]; then
  fixed_tree=$(tr -d '[:space:]' < "$gate_fix_tree")
  approved=$(grep -m1 '^TREE_SHA256:' "$change_dir/diff-review.md" 2>/dev/null \
    | sed 's/^TREE_SHA256:[[:space:]]*//' | tr -d '[:space:]')
  if [ "$fixed_tree" != "$approved" ]; then
    # The digest the gate fix left is handed to the round as the tree it judges: every
    # gate command is read-only (#326) and the review role may not write, so nothing
    # between that measurement and this one can have moved the tree.
    #
    # The reviewer is the code reviewer, which cannot be one of the authors here: the only
    # agent that can have written since the diff review is $implementer, and the two are
    # refused as equal at the top of this script.
    review_round "$change_dir" "$code_reviewer" "$change" "$fixed_tree" \
      "gate-fix review: $code_reviewer"
    case "$REVIEW_VERDICT" in
      APPROVE)
        write_diff_review "$change_dir" "$code_reviewer" "$fixed_tree" "$REVIEW_ROUND_NO" "$REVIEW_LOG"
        say "the gate fix is approved" ;;
      REVISE)
        echo >&2
        echo "the gate fix was reviewed and rejected. Its findings are in" >&2
        echo "$change_dir/$REVIEW_ROUND_FILE." >&2
        echo "A gate fix is one unattended round on a lint; findings against it are a defect," >&2
        echo "which is what the second gate failure above already stops for. Stopping." >&2
        exit 11 ;;
    esac
  fi
fi

# --- stage 5: the commit ----------------------------------------------------------
# The message is written by whoever wrote the diff, because it is the only participant
# that knows why. The hook runs both gate scripts over what is staged.
if [ -n "$(git -C "$wt" status --porcelain)" ]; then
  msg="$wt/.agent-runs/commit-msg.txt"
  : > "$msg"
  say "commit message: $implementer"
  # Resumed when there is a session to resume: on a re-run that found the diff review
  # already passing, apply.sh never ran and there is none.
  resume=""
  [ -s "$wt/.agent-runs/implement-$implementer.conversation" ] && resume="--resume"
  "$stage" commit-msg "$implementer" "$change" $resume \
    "End the message with a line reading Fixes #$issue."; rc=$?
  ask_stop "commit message ($implementer)" commit-msg
  # Exit 5 means it changed something other than the message. That matters more here
  # than anywhere: this stage runs after the review and after the gates, so whatever it
  # touched would land having passed neither.
  [ "$rc" -eq 0 ] || { echo "the commit-message stage exited $rc; nothing committed." >&2; exit 1; }
  [ -s "$msg" ] || { echo "no commit message at $msg; stopping rather than inventing one." >&2; exit 1; }
  grep -q "Fixes #$issue" "$msg" || printf '\nFixes #%s\n' "$issue" >> "$msg"
  git -C "$wt" add -A || exit 1
  git -C "$wt" commit -F "$msg" || { echo "the commit was refused; the hook says why." >&2; exit 1; }
  stage_done commit-msg
fi

# --- stage 6: the merge -------------------------------------------------------
# The driver ends at the commit. Nothing is pushed and no branch run is waited for:
# that run cost three to four minutes and bought almost nothing the local gates had not
# already checked, and a broken commit ships nothing because build needs [rust, ui] and
# runs only on main or a tag (#354). A clean-machine difference grounded in harness is
# the one class it did catch, gating no publish.
# The last point at which an unapplied answer can still be caught: gate-fix runs only if
# the gates failed, and commit-msg only if there was something to commit, so either can
# be skipped on a run that is otherwise fine.
if [ -n "$force_stage" ]; then stranded; fi

# Which sequence to print depends on whether main has moved, and that is the only case
# where it matters. A plain `git merge` fast-forwards when it can and builds a merge commit
# when it cannot, so printing it unconditionally recommended the shape #341 removed on
# exactly the occasion it would be created, and nothing would have refused it: the hooks
# allow a merge on main, which is where this one happens (#346).
#
# Anything that leaves the answer unreliable is printed WITH the commands rather than
# ahead of them on stderr. What a person copies is this block; a caveat scrolled past two
# screens earlier is one nothing downstream will repeat, and the hooks allow a merge on
# main, so wrong advice here ships.
behind=no; caveat=""
git -C "$wt" fetch origin --quiet 2>/dev/null \
  || caveat="Could not fetch origin, so this was decided from the last-known origin/main and may name the wrong command. Fetch and look before you run it."
if git -C "$wt" rev-parse -q --verify origin/main >/dev/null 2>&1; then
  git -C "$wt" merge-base --is-ancestor origin/main HEAD || behind=yes
else
  # Never silently the permissive answer. origin/main is what "has main moved" is asked of,
  # and without it the question has no answer at all; saying so beats printing a
  # fast-forward that has nothing to fast-forward onto.
  caveat="origin/main does not resolve here, so whether main has moved could not be read at all. The sequence below assumes it has not."
fi

[ -n "$caveat" ] && caveat="
WARNING: $caveat
"
# No `git push origin --delete $branch`: the branch was never pushed, so there is
# no remote ref to delete. `merge --ff-only` pushes main, `worktree remove` and
# `branch -d` clean up the local worktree and branch (#354).
if [ "$behind" = yes ]; then
  cat <<EOF
Issue #$issue is committed on $branch. Nothing has reached main.
$caveat
main has moved past this branch, so it will not fast-forward, and a change branch does not
merge main into itself (#341). Rebase it and then fast-forward:

  git -C "$wt" rebase origin/main
  git -C "$root" fetch origin
  git -C "$root" merge --ff-only $branch && git -C "$root" push
  git -C "$root" worktree remove $wt && git -C "$root" branch -d $branch
EOF
else
  cat <<EOF
Issue #$issue is committed on $branch. Nothing has reached main.
$caveat
  git -C "$root" fetch origin
  git -C "$root" merge --ff-only $branch && git -C "$root" push
  git -C "$root" worktree remove $wt && git -C "$root" branch -d $branch
EOF
fi
