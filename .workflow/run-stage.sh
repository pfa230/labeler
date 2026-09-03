#!/usr/bin/env bash
# Run one stage of a change on a named agent (#224, #283).
#
#   .workflow/run-stage.sh <role> <agent> <change> [--resume] [extra prompt...]
#
# Exit 5 = the reviewer edited files. 7 = no answer could be read out of what the agent
# printed, whatever role it was running and whatever status it exited with (#315).
# 3 = implement changed nothing, which a plan declaring `DELIVERABLE: spec-only`
# answers for its own change (#313).
#
#   role   propose | plan-review | tasks | implement | review | gate-fix | archive |
#          commit-msg
#   agent  see .workflow/agents.sh
#
# Roles carry two independent properties, and every guard below keys on a property
# rather than on a role name, so a role added later cannot quietly opt out of one:
#
#   writes    needs write access and the apply lock
#   guarded   must leave the worktree as it found it, checked by a digest across the
#             stage. Both reviewers, because a reviewer that edits has produced a delta
#             nobody reviewed. And commit-msg, because it runs after both the review and
#             the gates, so anything it changed would be committed without either.
#   produces  must leave the worktree DIFFERENT, by the same digest. A stage that was
#             asked for an artifact and exited cleanly having written nothing did not
#             run, and the caller would otherwise record the work as done. What "the
#             worktree" means is per role: for implement it excludes openspec/changes,
#             so a run that only ticked task boxes has not implemented anything.
#
# Both are measured as a delta ACROSS the stage, never as the absolute dirtiness of the
# tree: by the time a fix round runs, the tree is already dirty with the work being
# fixed, and counting files there reports every no-op as a change.
#
# The pairing is the point: an implementer and a reviewer that are different agents,
# expressed at dispatch rather than left to whoever remembers. /apply drives both.
#
# The agent's transcript goes to a log, never to stdout. Only the status, the files
# touched and a tail come back; a full transcript is thousands of lines and pulling
# one through the orchestrator is waste.
#
# Two facts about the tree go out with it, because this is the only place that knows
# them and nothing downstream can recover them (#299):
#
#   tree: <sha>   the digest of the worktree this stage left behind, minus
#                 openspec/changes. apply.sh records it as the TREE_SHA256 of the review
#                 it is about to launch, and refuses to launch one on a tree a previous
#                 round already judged.
#   the author ledger, openspec/changes/<change>/authors: the agent's name, appended when
#                 an implement or gate-fix stage actually changed the tree. apply.sh
#                 renders diff-review.md's AUTHORS: line from it.
set -uo pipefail

role="${1:?role required: propose | plan-review | tasks | implement | review | gate-fix | archive | commit-msg}"; shift
agent="${1:?agent required, e.g. agy}"; shift
change="${1:?change name required, e.g. issue-186-pin-rust-toolchain}"; shift || true

resume_requested=0
if [ "${1:-}" = "--resume" ]; then shift; resume_requested=1; fi
# Whether this run found work already on the tree. It is the only thing that can excuse a
# producing run that changes nothing: "there was nothing left to do" is a claim about
# inherited work, and on a clean tree there is none to have been done (#292).
inherited=no
extra="$*"

case "$role" in
  propose|tasks|implement|gate-fix|archive|commit-msg) writes=1 ;;
  plan-review|review)                                 writes=0 ;;
  *) echo "unknown role: $role (propose | plan-review | tasks | implement | review | gate-fix | archive | commit-msg)" >&2; exit 2 ;;
esac
case "$role" in plan-review|review|commit-msg) guarded=1 ;; *) guarded=0 ;; esac
# A producing role must leave something behind, with one exception: a run that INHERITED
# work may legitimately conclude there is nothing left to do, having answered the findings
# in prose or verified what it was handed. That is judged by the tree, not by the role,
# further down.
case "$role" in
  propose|tasks|gate-fix|archive) produces=1 ;;
  implement)                      produces=1 ;;
  *)                              produces=0 ;;
esac

# Siblings are resolved beside this script, and .worktrees/ hangs off the main
# checkout rather than whichever worktree we were called from: --show-toplevel
# answers the latter, so it cannot locate either one (#264, same defect as #256).
here=$(cd "$(dirname "$0")" && pwd)
common=$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null) || { echo "not in a git repo" >&2; exit 2; }
root=$(dirname "$common")
. "$here/agents.sh"
. "$here/questions.sh"

agent_known "$agent" || { echo "unknown agent: $agent" >&2; exit 2; }
command -v "$agent" >/dev/null 2>&1 || { echo "$agent is not on PATH; nothing would run." >&2; exit 2; }

issue=$(printf '%s' "$change" | sed -n 's/^\(issue-[0-9]\{1,\}\).*/\1/p')
[ -n "$issue" ] || { echo "change name must start with issue-<N>-: $change" >&2; exit 2; }
wt="$root/.worktrees/$issue"
[ -d "$wt" ] || { echo "no worktree at $wt" >&2; exit 2; }
# propose is the one role that runs before the folder exists; a resumed propose finds
# it there. Every other role requires it, live or archived: the gate fix and the commit
# message both run after archive has moved it, and demanding the live path there would
# refuse the last two stages of every completed change.
if [ "$role" != "propose" ]; then
  found=0
  [ -d "$wt/openspec/changes/$change" ] && found=1
  for d in "$wt"/openspec/changes/archive/*"$change"/; do [ -d "$d" ] && found=1; done
  [ "$found" = "1" ] || { echo "no change '$change' in $wt, live or archived" >&2; exit 2; }
fi

# Where that folder is right now. The author ledger is written into it, and the two stages
# that write it straddle archive's move: implement runs before it, gate-fix after. Resolved
# the way the check above resolves it, so the two cannot disagree about where the change is.
# A function because propose creates the folder DURING its stage, so the answer taken before
# the run is empty for exactly the role that then has to write into it.
resolve_change_dir() {
  local d
  change_dir=""
  if [ -d "$wt/openspec/changes/$change" ]; then
    change_dir="$wt/openspec/changes/$change"
  else
    for d in "$wt"/openspec/changes/archive/*"$change"/; do [ -d "$d" ] && change_dir="$d"; done
  fi
}
resolve_change_dir

# What the plan says this change delivers, and empty when the plan says nothing. One legal
# value, `spec-only`: the delta under specs/ IS the deliverable, so implementing it writes
# no file outside openspec/changes, which is the shape the produces guard below would
# otherwise read as a stage that never ran (#313). Absent is the stated
# default and means the change delivers code, which is every other change.
#
# Anything else stops the stage rather than being read as absent. There is no second
# spelling of the default and no third value: a plan carrying one has said something this
# tooling cannot act on, and guessing which way it meant is the silent fallback.
read_deliverable() { # read_deliverable <change-dir> -> the declared value; 1 = malformed
  local dir="${1:-}" p n v
  p="$dir/proposal.md"
  [ -n "$dir" ] && [ -f "$p" ] || return 0
  n=$(grep -c '^DELIVERABLE:' "$p" 2>/dev/null || true)
  case "${n:-0}" in
    0) return 0 ;;
    1) ;;
    *) echo "$p carries $n 'DELIVERABLE:' lines, so which one is the plan is a guess." >&2
       return 1 ;;
  esac
  # Trimmed at the ends only. Deleting every space would read `spec - only` as `spec-only`
  # and accept it, which is this reader normalising a malformed value into the one legal
  # one while every other malformation above it stops the stage. The field is written by
  # hand, so what it says is what it must mean (#313).
  v=$(grep '^DELIVERABLE:' "$p" | sed 's/^DELIVERABLE:[[:space:]]*//; s/[[:space:]]*$//')
  [ "$v" = "spec-only" ] || {
    echo "$p declares 'DELIVERABLE: $v', which is not a deliverable this loop knows." >&2
    echo "The only value is 'spec-only', for a change whose delta under specs/ is the whole" >&2
    echo "deliverable. Every change that delivers code omits the line." >&2
    return 1; }
  printf '%s' "$v"
}
# Read HERE, before the agent is launched, and never re-read for the guard below. The
# exemption it grants is the one thing an implement stage gains by writing the line itself,
# and openspec/changes is excluded from that stage's work digest, so writing it would cost
# nothing and buy the exemption. Read before the launch, the stage it exempts cannot have
# written it.
deliverable=$(read_deliverable "$change_dir") || exit 2

# Implementing past a failed plan review wastes the run; reviewing is always allowed.
# --plan-only because this fires before the diff review exists: demanding one here
# would refuse to start the very run that produces it.
if [ "$role" = "implement" ] && ! "$here/review-gate-check.sh" --plan-only "$wt" src/_probe >/dev/null 2>&1; then
  echo "review gate refuses this change; not starting:" >&2
  "$here/review-gate-check.sh" --plan-only "$wt" src/_probe 2>&1 >/dev/null | sed 's/^/  /' >&2
  exit 1
fi

# Every run artifact lands in one ignored directory, so a `git add -A` in the
# worktree cannot sweep a transcript into the change's commit (#255).
runs="$wt/.agent-runs"
mkdir -p "$runs"
# Parked before the agent starts, so a QUESTIONS.md found afterwards was written by
# this stage rather than left by an earlier one.
questions_park "$wt"
log="$runs/$role-$agent.log"
raw="$runs/$role-$agent.json"
# propose and archive share one conversation slot, so archive resumes the session that
# wrote the deltas rather than reading them back cold (#283). Every other role is its
# own slot, which leaves the existing files named as they were.
# propose and archive share the planning session; the commit message is written by the
# implementer, resuming the session that wrote the diff, because that is the only
# participant that knows why the diff looks as it does.
case "$role" in
  propose|tasks|archive)  session=plan ;;
  commit-msg|gate-fix)    session=implement ;;
  *)                      session="$role" ;;
esac
conv_file="$runs/$session-$agent.conversation"

resume=""
if [ "$resume_requested" -eq 1 ]; then
  case "$role" in
    # Advisory for the writing roles: the caller states intent, and the decision under the
    # lock below settles it against the record of who holds this tree. A missing session is
    # not an error there, it is a handover.
    implement|gate-fix) : ;;
    *)
      [ -s "$conv_file" ] || { echo "--resume needs a previous run; no id at $conv_file" >&2; exit 2; }
      resume=$(cat "$conv_file") ;;
  esac
fi

# Every role may stop and ask, because a stage that cannot ask has to guess, and a
# guess buried in an artifact is worse than an hour of waiting (#283). The bar is in
# the sentence: this is for what the stage cannot decide, not for what it would
# rather not decide.
lock=""
if [ "$writes" = "1" ]; then
  # --git-DIR, not --git-common-dir. The common dir resolves to the same .git for every
  # worktree, so one lock file served the whole repository and a writing stage anywhere
  # blocked a writing stage everywhere - two changes on disjoint files, serialized, in a
  # repo whose rule is one change per worktree precisely so they need not be (#294). The
  # git dir is per worktree (.git/worktrees/<name>, and plain .git for the main checkout),
  # so this is exactly "writing THIS tree", which is what must be exclusive.
  #
  # Not .agent-runs/: that directory is gitignored working state, created and deleted by
  # this script and by whoever cleans up after a run, and a lock a passing broom can carry
  # off protects nothing. Git's own storage is not swept, is not stageable, and exists
  # before any stage runs.
  lock="$(cd "$wt" && git rev-parse --path-format=absolute --git-dir)/APPLY_IN_PROGRESS"
  # noclobber makes the create atomic. Testing for the file and then writing it is two
  # steps, and two callers can both pass the test before either writes, which is exactly
  # the concurrency the lock exists to prevent. The suite provokes exactly that race, by
  # putting a barrier in the `date` call below so two stages arrive at the redirection
  # together: this form lets one through, the check-then-write form let both.
  if ! ( set -o noclobber
         printf '%s %s started %s (pid %s)\n' "$agent" "$change" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$$" > "$lock"
       ) 2>/dev/null; then
    echo "a run is already in progress: $(cat "$lock" 2>/dev/null)" >&2; exit 1
  fi
  # A signal handler that only cleans up is worse than none: bash runs it and then CARRIES
  # ON, so an interrupt could release the lock and the run would launch its writer with no
  # protection at all. Each signal exits with its own conventional status.
  trap 'rm -f "$lock"' EXIT
  trap 'rm -f "$lock"; exit 130' INT
  trap 'rm -f "$lock"; exit 143' TERM
fi


# THE HANDOVER DECISION IS MADE HERE, under the lock and before the command is built.
# Computed by the caller instead, it is a guess with a window in it: another writing stage
# can finish between the caller deciding and this one locking, which makes the chosen
# session stale before it launches (#292). The caller's --resume is therefore advisory for
# these roles; what decides is who the record says holds this tree, read while nobody else
# can be writing it.
case "$role" in
  implement|gate-fix)
    # From zero, not from the flag: the caller's --resume is intent, and a first run on a
    # clean tree that kept resume_requested=1 would be given "Continue your work" and
    # allowed to change nothing, both of which are wrong for a run continuing nothing.
    [ -n "$(cd "$wt" && git status --porcelain -- . ':!.agent-runs' ':!QUESTIONS.md' ':!ANSWERS.md' ':!openspec/changes' 2>/dev/null)" ] && inherited=yes
    handover_plan "$wt" "$agent"
    resume=""; resume_requested=0; handover_extra=""
    if [ -n "$HANDOVER_TEXT" ]; then
      handover_extra="$HANDOVER_TEXT"
      # Cleared HERE, not after the run. Deciding not to resume this session already means
      # it is unusable, and leaving it on disk while recording this agent as the one
      # holding the tree is a state a signal can freeze: INT between the record and the
      # post-run cleanup leaves a fresh record beside a stale id, and the next decision
      # reads that pair as a session to resume (#292).
      : > "$conv_file" || {
        echo "cannot clear the stale session at $conv_file; not running." >&2
        echo "Leaving it beside a fresh record is the pairing this exists to prevent." >&2
        exit 1; }
      echo "handover: this run continues no session; it is told what it inherits." >&2
    elif [ -n "$HANDOVER_RESUME" ]; then
      resume=$(cat "$conv_file" 2>/dev/null)
      [ -n "$resume" ] && resume_requested=1
    fi ;;
esac

questions="If ANSWERS.md exists at the root of your worktree, read it first: it holds the answers to questions earlier runs asked. If something genuinely blocks you, write your questions to QUESTIONS.md at that same root and stop rather than guessing. Use it only for what you cannot decide yourself: a contradiction in what you were given, or a missing decision that changes the contract. Anything you can decide, decide it and record the assumption."

case "$role" in
  propose)
    step=$(agent_step_prompt "$agent" propose "$change") || { echo "no propose prompt for $agent" >&2; exit 2; }
    # review.md and tasks.md are explicitly withheld: the schema has tasks requiring
    # review, and the review is performed by a different agent. A propose workflow that
    # walks the whole dependency closure will otherwise write both, which is a task list
    # for an unjudged plan and, worse, a review of its own work.
    base="$step Planning only: write proposal.md, the delta specs under specs/, and design.md, and nothing else. Do NOT write review.md: a different agent reviews this plan, and writing it yourself is reviewing your own work. Do NOT write tasks.md: the task list is written after the review, by a separate stage. Do not write or edit project code. Do not commit. Do not edit docs/SPEC.md, which is frozen. $questions"
    # Neutral on purpose: a resumed propose is a revision after a review, a continuation
    # after a question, or the application of required changes, and the caller says which.
    [ "$resume_requested" -eq 1 ] && base="Continue your work on the plan for $change. The same limits still hold: planning only, no project code, no commit, and docs/SPEC.md is frozen. $questions"
    ;;
  implement)
    step=$(agent_step_prompt "$agent" apply "$change") || { echo "no apply prompt for $agent" >&2; exit 2; }
    base="$step Stop when the tasks are implemented. Do not commit. Do not archive. Do not sync specs into openspec/specs/. Do not move or delete the change folder. Do not edit docs/SPEC.md, which is frozen. Check a task only after actually performing it. $questions"
    # Neutral for the same reason: a resumed implement is a fix round, a gate fix, or a
    # continuation after a question. Telling it to "fix the findings" when the caller is
    # handing it a gate log describes the wrong task.
    [ "$resume_requested" -eq 1 ] && base="Continue your work on $change. The same limits still hold: do not commit, archive, sync specs, move the change folder or edit docs/SPEC.md. $questions"
    ;;
  tasks)
    # No tool ships a command for this step, so every agent gets plain instructions. It
    # runs after the plan review, because the schema has tasks requiring review: a task
    # list written before the verdict describes a plan that may not survive it.
    base="Write the implementation task list for $change to openspec/changes/$change/tasks.md, relative to your worktree root. Run 'openspec instructions tasks --change $change --json' and follow the instruction and the template it returns. The plan has been reviewed and approved, so the tasks must match its proposal, its delta specs and its design, and must add nothing that is not in them. Do not write or edit project code. Do not commit. $questions"
    ;;
  gate-fix)
    # Its own role because it runs AFTER archive: the change folder has moved under
    # archive/, so the apply step's prompt - which names openspec/changes/<change> or
    # invokes /opsx:apply on it - refers to a path that is no longer there.
    base="The verification gates failed on your implementation of $change. Their output is in .agent-runs/gates.log, relative to your worktree root. Read it and fix the cause in the project code; never silence a lint with an allow attribute. The change folder has already been archived, so do not look for it under openspec/changes/ and do not move or edit it. Do not commit. $questions"
    ;;
  archive)
    step=$(agent_step_prompt "$agent" archive "$change") || { echo "no archive prompt for $agent" >&2; exit 2; }
    base="$step Sync every delta into openspec/specs/. The tool will offer to skip that sync or to accept unchecked tasks; both are forbidden here, so refuse both offers. Do not commit. Do not merge. Do not edit docs/SPEC.md, which is frozen. $questions"
    ;;
  plan-review)
    # Three verdicts, and the middle one is load-bearing: APPROVE_WITH_CHANGES ends the
    # loop, so the author applies the listed changes and nobody looks at them again. A
    # reviewer that files a vague requirement there has written an unreviewed edit.
    base="Adversarially review the PLAN for $change: proposal.md, the delta specs under specs/, and design.md, judged against AGENTS.md and openspec/config.yaml. Find real problems; do not rubber-stamp. Cite file:line evidence and verify each finding against the artifacts before raising it. Report findings only: you must not edit any file, review.md included. End your output with exactly one line, on its own line, reading VERDICT: APPROVE, VERDICT: APPROVE_WITH_CHANGES or VERDICT: REVISE. APPROVE means nothing must change. APPROVE_WITH_CHANGES means the plan is sound once specific edits are made: list them under a 'Required changes' heading, state each one completely, and note that the author applies them and NO further review follows, so anything you cannot state precisely belongs in REVISE instead. REVISE means the plan needs rework and a full re-review in a fresh context. $questions"
    ;;
  commit-msg)
    base="Write the git commit message for the work implemented in $change to .agent-runs/commit-msg.txt at the root of your worktree, and change nothing else. An imperative subject line under 72 characters, a blank line, then a short body saying what changed and why. Add no Co-Authored-By line, no 'Generated with' line and no AI attribution of any kind. Do not commit, and do not edit any other file. $questions"
    ;;
  review)
    # The verdict line is what lets apply.sh decide whether to loop. Without a
    # machine-readable answer the caller has to interpret prose, which is how a
    # review that found problems gets read as one that passed.
    base="Adversarially review the implementation diff for $change against its proposal, specs, design and tasks, and against AGENTS.md. Find real problems; do not rubber-stamp. Cite file:line evidence and verify each finding against the actual code before raising it. Report findings only: you must not edit any file. End your output with exactly one line, on its own line, reading either VERDICT: APPROVE or VERDICT: REVISE. Use REVISE if any finding must be fixed before this can land; any blocking finding forbids APPROVE. $questions"
    ;;
esac
prompt="$base $extra ${handover_extra:-}"

# Every writing role takes the lock: each one must not have a commit, a merge or a
# push land underneath it mid-run. A read-only reviewer has nothing to hold back.

cmd=$(agent_command "$agent" "$role" "$prompt" "$resume") || { echo "no invocation for $agent/$role" >&2; exit 2; }
# What the worktree looks like right now, content included. The reviewer guard below
# needs a DELTA across the stage, not the absolute dirtiness of the tree: apply never
# commits, so the implementer's work is always uncommitted when the reviewer runs, and
# counting `git status` lines blamed the reviewer for the implementer's diff every time.
#
# What a stage may touch without that counting as an edit: this script's own output,
# and the two files the question protocol runs on, which every role may write.
# One list for both review roles, and openspec/changes is NOT on it. An earlier version
# excluded it for the diff reviewer, reasoning that the implementer's checked task boxes
# would otherwise trip the guard; they do not, because this is a delta across THIS stage
# and the implementer's edits are in both digests. The exclusion bought nothing and let
# a reviewer rewrite the proposal, the tasks, the delta specs or an earlier review
# without detection, which is the whole guarantee.
# .agent-runs, QUESTIONS.md and ANSWERS.md are all gitignored, so git never reports them
# and excluding them by pathspec would be a no-op dressed up as a rule. The list below
# excludes only what git WOULD report; ANSWERS.md is covered explicitly in the digest,
# because a stage is told to read it and never to write it, and a stage that rewrote a
# person's answer would otherwise be invisible to every check here.
# ':(glob)**' is the identity pathspec, and it is here rather than an empty array
# because macOS ships bash 3.2, where expanding an empty array under `set -u` aborts the
# script. Never leave either of these lists empty.
guard_excl=(':(glob)**')
# What counts as having produced something. For implement it excludes the change folder:
# checking a task box is a claim about work, not the work, and a run that only ticked
# boxes did not implement anything.
work_excl=("${guard_excl[@]}")
[ "$role" = "implement" ] && work_excl+=(':!openspec/changes')
# The tree a review judges, and the one digest a caller records. Its scope is fixed rather
# than the role's, so every stage reports the same measurement of the same thing.
# openspec/changes is excluded because apply.sh writes each round's artifact into it
# between rounds: a digest that counted those would differ every round, and the repeated
# tree refusal could never fire (#299). Excluding it costs nothing already guaranteed
# elsewhere - review.md's SPECS_SHA256 binds the contract, and review-gate-check.sh
# enforces that binding.
tree_excl=(':(glob)**' ':!openspec/changes')
# sha256sum is GNU; macOS spells it shasum -a 256, and where it is missing entirely the
# unguarded form produced an EMPTY digest on both sides of the stage, which compares
# equal and lets every reviewer edit through. A guard that fails open is worse than none.
sha() { if command -v sha256sum >/dev/null 2>&1; then sha256sum; else shasum -a 256; fi; }
worktree_digest() { # worktree_digest <pathspec>...
  (
    cd "$wt" || exit
    # HEAD is part of the state. Without it a stage can edit a file and commit only that
    # file: status comes back clean, `git diff HEAD` shows nothing, and the edit sits in
    # a commit that neither digest ever looked at.
    git rev-parse HEAD
    # Gitignored, so no git command below will mention it. Stages read it; none writes it.
    [ -f ANSWERS.md ] && sha < ANSWERS.md
    git status --porcelain -- . "$@"
    git diff HEAD -- . "$@"
    # One fixed-length hash per path. Printing the path and then the bytes runs them
    # together, and two different sets of untracked files can emit identical bytes.
    git ls-files --others --exclude-standard -- . "$@" \
      | LC_ALL=C sort | while IFS= read -r f; do
          printf '%s %s\n' "$(sha < "$f" 2>/dev/null | cut -d' ' -f1)" "$f"
        done
  ) 2>/dev/null | sha | cut -d' ' -f1
}
before_guard=$(worktree_digest "${guard_excl[@]}")
before_work=$(worktree_digest "${work_excl[@]}")

# The last point before the agent can touch anything, and past everything that could
# refuse the run: the resume check, the lock, building the command. Recorded sooner, a
# launch that never happened still named its agent, and the next run read that agent as
# the one holding the tree and resumed its stale session (#292). Failing to record is
# fatal, because the alternative is an agent editing a tree the marker attributes to
# somebody else.
case "$role" in
  implement|gate-fix)
    note_implementer "$wt" "$agent" || {
      echo "cannot record '$agent' as the implementer at $runs/implement.last; not running." >&2
      exit 1; } ;;
esac

( cd "$wt" && pty_run "$cmd" ) 2>&1 | clean_capture > "$raw"
status=$?
after_guard=$(worktree_digest "${guard_excl[@]}")
after_work=$(worktree_digest "${work_excl[@]}")
after_tree=$(worktree_digest "${tree_excl[@]}")

# Read here rather than below, because the extraction reports it: a stage that emitted
# nothing while changing the tree is the worse of the two failures below, and saying which
# one happened needs this answer.
produced="no"
[ "$before_work" != "$after_work" ] && produced="yes"

# How an answer is separated from a transcript is per-CLI knowledge, so it lives in
# agents.sh beside the invocation that produced it. Here only the outcome matters, and
# there are two outcomes, told apart by whether the agent printed anything at all (#315):
#
#   NO_ANSWER_IN_OUTPUT  the capture is a console transcript the extractor found no answer
#                        in. It becomes $log, because it is the only lead a person has.
#   NO_OUTPUT            the agent printed nothing, so there is no transcript to fall back
#                        to. This line used to copy the empty capture over $log anyway,
#                        which is a fallback to something known to be worse: during #287 it
#                        left implement-agy.log at 0 bytes for a 21-minute run that wrote
#                        1193 lines, and an empty log reads as a run with nothing to say
#                        rather than as one whose account was destroyed. Record the absence
#                        instead, since the absence is the fact.
extracted=1
if ! agent_status=$(agent_extract "$agent" "$raw" "$log" "$conv_file"); then
  extracted=0
  if [ -s "$raw" ]; then
    # NO_ANSWER_IN_OUTPUT is what this run could not do, not what went wrong. Where the
    # tool said what went wrong, say that instead: #297 was filed on a stage whose only
    # word was that a parser found nothing, and what it hid was opencode's own error
    # event. A capture carrying no error still reports the shape (#297 into #315).
    if agent_error_msg=$(agent_error "$agent" "$raw"); then
      agent_status="AGENT_ERROR ($agent_error_msg)"
    else
      agent_status="NO_ANSWER_IN_OUTPUT"
    fi
    cp "$raw" "$log"
  else
    agent_status="NO_OUTPUT"
    { printf 'run-stage.sh wrote this file. %s wrote nothing.\n\n' "$agent"
      printf 'The %s stage of %s ran %s, which exited %s having printed nothing at all:\n' \
        "$role" "$change" "$agent" "$status"
      printf 'the console capture at %s is empty, so there is no transcript to keep here.\n' "$raw"
      if [ "$produced" = "yes" ]; then
        printf '\nIt changed the worktree while saying nothing, so the work is on disk and this\n'
        printf 'file is the whole account of it. Read git status and git diff in %s.\n' "$wt"
      else
        printf '\nIt changed nothing in the worktree either.\n'
      fi
    } > "$log"
  fi
  # There WAS a truncation here, for a writing role whose extraction failed leaving a
  # stale id behind. It is gone because it became unreachable, not because it stopped
  # mattering: the decision above already clears any session it declined to resume, so by
  # the time this runs the file is either empty, or holds an id this run resumed from, or
  # holds one this run captured. A stale id that was not resumed cannot survive to here.
  # Left in place it would have been dead code with a fatal-or-warning question attached
  # to it, which is a question about nothing (#292).
fi

echo "role: $role   agent: $agent   status: $agent_status   exit: $status"
echo "changed the worktree: $produced"
echo "tree: $after_tree"
echo "log: $log"

# A failed stage that says only that it failed is what #297 was filed on. Printed here,
# beside the stage's own result and before every exit below, so it is read whichever one
# this run takes. It offers a line to run and runs nothing: this script never changes the
# model it was given, because the only model that would fix a spent allowance bills.
if [ "$status" != "0" ] && advice=$(agent_stall_advice "$agent" "$raw"); then
  echo
  echo "-- $agent could not finish this stage ---------------------------------------"
  printf '%s\n' "$advice"
  echo "-----------------------------------------------------------------------------"
fi

# Who actually wrote the code. Appended here because this is the only place that knows both
# the agent's name and whether its stage changed anything; every caller knows one or the
# other. In the change folder rather than .agent-runs/, for the reason the apply lock is not
# there either: that directory is gitignored working state these scripts create and delete,
# and a record a passing broom can carry off is not a record. It must also survive across
# separate apply.sh invocations, because an implementer swap can land in a later one than
# the work it inherits (#292, #299).
#
# Writing it here cannot make a no-op look like work: implement's own digest excludes
# openspec/changes, and every digest above was taken before this line runs.
#
# The propose stage of a spec-only change is an author for the same reason those two are:
# what lands is the delta, and propose is the stage that wrote it. Nothing else can claim
# it - implement has no code to write - so without this the ledger is empty, AUTHORS: is
# empty, and the landing gate refuses a change nobody could have finished but by hand
# (#313, and review-gate-check.sh:71-75 names this case). Re-read after the run, because the
# stage being judged is the one that wrote both the delta and the declaration; claiming
# authorship of what it just wrote is a liability it is telling the truth about, not an
# exemption it is granting itself.
authored=no
if [ "$produced" = "yes" ]; then
  case "$role" in
    implement|gate-fix) authored=yes ;;
    propose)
      resolve_change_dir
      proposed_deliverable=$(read_deliverable "$change_dir") || exit 2
      [ "$proposed_deliverable" = "spec-only" ] && authored=yes ;;
  esac
fi
if [ "$authored" = "yes" ]; then
  [ -n "$change_dir" ] || {
    echo >&2; echo "no folder for '$change' in $wt, so the author cannot be recorded." >&2
    exit 1; }
  if ! grep -qxF "$agent" "$change_dir/authors" 2>/dev/null; then
    printf '%s\n' "$agent" >> "$change_dir/authors" || {
      echo >&2; echo "cannot append to the author ledger at $change_dir/authors; stopping." >&2
      echo "An unrecorded author is the silent misattribution this ledger exists to prevent." >&2
      exit 1; }
  fi
fi
echo "--- last 30 lines ---"
tail -30 "$log"

# A stage whose answer could not be read did not report, and that is refused for EVERY
# role and whatever the agent's own exit status was (#315). For a review the damage is
# immediate: the caller would read a verdict out of a transcript, hand the transcript to
# the implementer, and commit it as the review artifact (#264). For a writing role it is
# the same failure one step later, and it happened: opencode returned no result and exit 0
# on the implement stage of #287, which is indistinguishable from a stage that ran, so the
# driver moved on to review code opencode had not written. agy's exit 2 stopped the same
# failure the same day, and the difference between the two was the agent's rather than
# this script's. One rule, so the agent no longer decides.
# Keyed on extraction having failed for THIS agent rather than on one agent's envelope
# being absent: keyed the latter way, no agent but agy could pass a review it had actually
# written (#274). Each tool withholds edits its own way, and they are not equally strong:
# codex enforces it with -s read-only, opencode by denying edit/write/bash to its reviewer
# agent, which drops those tools from the model's toolset entirely (#286), and agy with
# --mode plan, which is the agent declining to act without a Proceed rather than a harness
# refusing it (#290). The digest below is what actually decides.
if [ "$extracted" -eq 0 ]; then
  echo >&2
  if [ "$agent_status" = "NO_OUTPUT" ]; then
    echo "$agent printed nothing during the $role stage: the capture at $raw is empty, so" >&2
    echo "$log records that absence rather than anything the agent said." >&2
    [ "$produced" = "yes" ] && \
      echo "It changed the worktree while saying nothing: the work is here, the account of it is not." >&2
    echo "Refusing to report a stage that said nothing as one that ran." >&2
  elif [ "$writes" = "0" ]; then
    echo "no structured result from $agent, so $log is the raw transcript rather than the review." >&2
    echo "Refusing to treat a transcript as a review. The capture is at $raw." >&2
  else
    echo "no structured result from $agent, so $log is the raw transcript rather than its answer." >&2
    echo "Refusing to report a stage whose answer could not be read as one that ran." >&2
    echo "The capture is at $raw." >&2
  fi
  exit 7
fi
# A reviewer that changed files has broken the rule it was told to follow. Judged by
# whether this stage altered the tree, not by whether the tree was already dirty.
if [ "$guarded" = "1" ] && [ "$before_guard" != "$after_guard" ]; then
  echo >&2
  case "$role" in
    commit-msg) echo "the commit-message stage altered the worktree. It runs after the review and the gates, so anything it changed would be committed without either." >&2 ;;
    *)          echo "the reviewer altered the worktree during its stage. Reviewers report; they do not edit." >&2 ;;
  esac
  exit 5
fi
# A producing stage that exits cleanly having changed nothing did not run. Measured as a
# delta across this stage, so a fix round on an already-dirty tree is judged on what IT
# did rather than on what the tree looked like when it started.
if [ "$produces" = "1" ] && [ "$status" -eq 0 ] && [ "$produced" = "no" ]; then
  # A run that inherited work is the one case where nothing is a defensible answer: the
  # implementer may have justified every finding rather than acting on it, or verified that
  # what it was handed was already right. Said loudly, because it also looks exactly like a
  # round that did nothing at all, and the caller is about to record the findings as
  # addressed.
  if { [ "$role" = "implement" ] || [ "$role" = "gate-fix" ]; } && [ "$inherited" = "yes" ]; then
    echo >&2
    echo "warning: this fix round changed nothing. Either every finding was answered in" >&2
    echo "prose, or none was acted on. $log is the whole of what it said." >&2
  # The second case where nothing is the right answer: a change whose plan declares its
  # delta the whole deliverable has no code for this stage to write, so measuring it by
  # the code it wrote asks the wrong question. It is not exempt from being measured. The
  # measurement moves to the folder the work digest excludes: this stage must still have
  # ticked its task boxes, or left something else behind in the change, and a stage that
  # touched nothing at all anywhere is refused below exactly as it is today. That is what
  # keeps a silently failed implementer from passing here, and the declaration is read
  # before the launch so this stage cannot have written it (#313).
  elif [ "$role" = "implement" ] && [ "$deliverable" = "spec-only" ] \
       && [ "$before_guard" != "$after_guard" ]; then
    echo >&2
    echo "note: this change declares DELIVERABLE: spec-only, so its delta under specs/ is the" >&2
    echo "whole deliverable and there is no code for an implement stage to write. It ran: it" >&2
    echo "changed the change folder and nothing outside it. $log is what it said." >&2
  else
    echo >&2; echo "$role produced no changes despite a clean exit. It did not run. See $raw" >&2
    exit 3
  fi
fi
exit $status
