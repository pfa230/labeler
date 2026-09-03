#!/usr/bin/env bash
# The blocking-question protocol (#283).
#
# Sourced, not executed. Any stage may write QUESTIONS.md at its worktree root and
# stop, rather than guess at something it cannot decide. The driver that ran it stops
# too, with exit 8, and whoever launched the driver relays the questions. Answers come
# back as ANSWERS.md beside it, and every stage prompt tells the agent to read that
# file first, so no stage needs to be resumed for the answers to reach it.
#
# Why a file and not a prompt: the stages run headless, detached from any terminal, so
# there is nobody to ask. The alternative is an agent that guesses and buries the guess
# in an artifact a later reader trusts, which costs more than the wait does.
#
# Both files are gitignored. A question is working state, not record.

questions_file() { printf '%s/QUESTIONS.md' "$1"; }
answers_file()   { printf '%s/ANSWERS.md' "$1"; }

# The stage that asked is recorded, because artifact state alone cannot say who asked.
# A stage can write its output and THEN discover the question, and the next run would
# then read that output as "this stage is done" and skip the very stage the answers were
# for. The pending file is still an artifact, not a ledger of what ran: it exists only
# while a question is outstanding, and answering it is what clears it.
pending_file() { printf '%s/.agent-runs/pending-question' "$1"; }

# questions_pending <worktree> — true when a stage has asked something.
questions_pending() { [ -s "$(questions_file "$1")" ]; }

# questions_outstanding <worktree> — true when an earlier run stopped on a question.
questions_outstanding() { [ -s "$(pending_file "$1")" ]; }

# questions_asker <worktree> — the role that asked, or nothing.
questions_asker() { sed -n 's/^STAGE: //p' "$(pending_file "$1")" 2>/dev/null | head -1; }

# questions_park <worktree> — move a question aside before a stage runs, so that a
# file found afterwards was written by THAT stage. Moved rather than deleted: an
# unanswered question is still the only record of why a run stopped.
questions_park() {
  local wt="$1" q; q="$(questions_file "$wt")"
  [ -f "$q" ] || return 0
  mkdir -p "$wt/.agent-runs"
  mv "$q" "$wt/.agent-runs/questions-$(date -u +%Y%m%dT%H%M%SZ).md" 2>/dev/null || : > "$q"
}

# questions_record <worktree> <role> — take the question the stage just wrote and record
# which stage wrote it. Called when a driver is about to stop.
questions_record() {
  local wt="$1" role="$2" q; q="$(questions_file "$wt")"
  mkdir -p "$wt/.agent-runs"
  { printf 'STAGE: %s\n\n' "$role"; cat "$q" 2>/dev/null; } > "$(pending_file "$wt")"
}

# questions_clear <worktree> — the question is answered; the stage that asked has run.
questions_clear() {
  local wt="$1" p; p="$(pending_file "$wt")"
  [ -f "$p" ] || return 0
  mv "$p" "$wt/.agent-runs/answered-$(date -u +%Y%m%dT%H%M%SZ).md" 2>/dev/null || : > "$p"
}

# questions_report <worktree> <stage-label> — print them and say what happens next.
questions_report() {
  local wt="$1" label="$2"
  echo >&2
  echo "== $label stopped to ask ==" >&2
  # >&2 last: putting 2>/dev/null first points stderr at the bin, and then >&2 sends
  # stdout there too, which silently swallowed the question this exists to print.
  sed 's/^/  /' "$(questions_file "$wt")" >&2
  echo >&2
  echo "Answer them in $(answers_file "$wt") and re-run; the stage that asked runs again," >&2
  echo "and every stage reads that file." >&2
}
