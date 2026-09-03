#!/usr/bin/env bash
# Agent registry: how to invoke each CLI for a given role (#224).
#
# Sourced, not executed. One place that knows each tool's invocation, because every
# one differs and each difference has already caused a silent failure here:
#   - agy's --print/-p TAKES the prompt as its value, so flags must precede it and
#     the prompt attaches as -p=...; written the obvious way the flag eats the next
#     flag and agy answers a question about it instead of working.
#   - agy's default model rejects --effort outright and still exits 0.
#   - codex blocks forever reading stdin unless it is given < /dev/null.
#   - script(1) is two incompatible programs sharing a name; see pty_run below.
# Each entry below records how its form was arrived at. That is a comment and not a
# runtime check, because whether an invocation works is decided by running it. What the
# run then says when it did not work is agent_error and agent_stall_advice below (#297).

# Roles: implement (may write) | review (must not write).
#
# Where a CLI can enforce read-only itself, the review role uses it. That makes
# "the reviewer does not edit" a property of how it was launched rather than an
# instruction it is asked to respect.

agent_known() {
  case "$1" in claude|agy|codex|opencode) return 0 ;; *) return 1 ;; esac
}

# agent_resumable <agent> — whether a later stage can continue this one's session.
#
# An AUTHOR must be resumable: every loop here sends findings back to whoever wrote the
# thing, and an author that cannot be resumed either starts over or stops. Every
# registered agent now satisfies this, so the list below is empty; the guard stays
# because it is the registry's check on a new entry, not a statement about today's.
#
# opencode was refused here until 1.18.20, on the grounds that it documented neither
# structured output nor a resume flag. Both exist: `--format json` emits one JSON event
# per line carrying sessionID, and `-s <sessionID>` continues that session (#286).
agent_resumable() {
  case "$1" in *) return 0 ;; esac
}

# last_implementer <worktree> — the agent that last ran a code-writing role here, and
# note_implementer <worktree> <agent> — record it.
#
# WHICH WAY IT FAILS, ON PURPOSE. The marker is written before the agent runs and is not
# rolled back, so a run that failed to launch, died, or wrote nothing still claims the
# tree. That over-claims, and the cost is a later return by the real author being treated
# as a swap: it is handed over to rather than resumed, and its own session is cleared. The
# alternative, recording after the run, under-claims instead - a signal between the edits
# and the record leaves the previous owner named, and the next run resumes a session that
# predates those edits. Over-claiming costs continuity; under-claiming applies stale
# reasoning to a tree it does not describe. The first is the failure worth having, and
# three attempts at a rollback to avoid both cost more than either (#292).
#
# RECORDED, not inferred. The conversation files cannot answer this and it is not a close
# call: extraction writes them AFTER the agent has already touched the tree, so their
# mtimes lag the edits, and an extraction that fails leaves no file at all for a run that
# changed everything. Ordering by mtime therefore names the wrong predecessor exactly when
# it matters most (#292). Who wrote a tree is not a property any artifact in it carries.
last_implementer() { tr -d '[:space:]' < "$1/.agent-runs/implement.last" 2>/dev/null; }
# ONCE MORE, THEN LOUDLY. The mkdir and the write are two statements and the directory
# went away between them during the #235 run: the write failed with "No such file or
# directory" on the line after its own mkdir -p had succeeded, in a worktree whose
# .agent-runs was there before the stage and after it. Nothing establishes what stepped
# into that window - the only candidate named is a process outliving the previous stage,
# whose children run-stage.sh does not wait for, and it was never reproduced - so this
# closes the window rather than explaining it. A transient loser now has to lose twice; a
# permanent one fails exactly as it did before (#296).
#
# Success means the record READS BACK, not that the write returned 0. The same window
# produces a write that reports success and records nothing: unlink the directory once the
# file is open and printf writes into an orphaned inode, exits 0, and leaves no marker and
# no error. That is the loss this record must never take silently, and no retry can see it.
#
# The first attempt is silent, because a failure that is about to be retried is not news.
# The second is not silenced, so whatever refused the write says so in its own words, and
# run-stage.sh stops the stage on the non-zero return.
note_implementer() { # note_implementer <worktree> <agent>
  local marker="$1/.agent-runs/implement.last"
  { mkdir -p "$1/.agent-runs" &&
    printf '%s\n' "$2" > "$marker" &&
    [ "$(last_implementer "$1")" = "$2" ]; } 2>/dev/null && return 0
  mkdir -p "$1/.agent-runs" || return 1
  printf '%s\n' "$2" > "$marker" || return 1
  [ "$(last_implementer "$1")" = "$2" ] || {
    printf 'wrote %s to %s and read back "%s"\n' "$2" "$marker" "$(last_implementer "$1")" >&2
    return 1; }
}

# handover_plan <worktree> <incoming> — decide how a code-writing run should start here.
# Sets HANDOVER_RESUME and HANDOVER_TEXT, which run-stage.sh reads under its own lock. A
# non-empty HANDOVER_TEXT is what makes a run one that continues work it did not write;
# a separate flag saying the same thing was two names for one fact. It lives here rather than there because the tests drive it directly, and
# because deciding this in a caller is a guess with a window in it (#292).
#
# One function, called from one place, because two copies of this reasoning in two callers
# is how they would stop agreeing. That was the shape before: apply.sh and run-change.sh
# each decided, and each could decide differently and before the lock (#292).
# shellcheck disable=SC2034  # both are read by run-stage.sh, which sources this
handover_plan() {
  local wt="$1" incoming="$2" previous own dirty
  HANDOVER_RESUME=""; HANDOVER_TEXT=""
  own="$wt/.agent-runs/implement-$incoming.conversation"
  previous=$(last_implementer "$wt")
  # Is there work here to inherit at all? Everything below turns on this: "verify what you
  # were handed" has no referent on a clean tree, and a handover claimed over one lets a
  # run that does nothing pass as a run that checked.
  dirty=$( (cd "$wt" && git status --porcelain -- . ':!.agent-runs' ':!QUESTIONS.md' ':!ANSWERS.md' ':!openspec/changes') 2>/dev/null )

  # The gate comes FIRST, before any resume. A clean tree has nothing to continue, whatever
  # the record and the session say: a run that changed nothing still leaves both behind, and
  # resuming on that basis tells the next attempt to "continue your work" when there is no
  # work (#292).
  [ -n "$dirty" ] || return 0

  if [ -n "$previous" ] && [ "$previous" = "$incoming" ] && [ -s "$own" ]; then
    HANDOVER_RESUME="--resume"
    return 0
  fi

  if [ -z "$previous" ]; then
    HANDOVER_TEXT="This worktree has work in it and no record of who wrote it. Nothing establishes that any earlier session of yours matches what is on disk, so you are not continuing one. Read 'git diff' before changing anything, and treat every checked box in tasks.md as a claim to verify rather than as fact."
  elif [ "$previous" = "$incoming" ]; then
    HANDOVER_TEXT="Your own earlier run left work in this worktree and its session could not be recovered, so you are starting without that reasoning. Read 'git diff' before changing anything, and treat every checked box in tasks.md as a claim to verify rather than as fact."
  else
    HANDOVER_TEXT="A previous implementer, $previous, worked on this change and its work is in your worktree, uncommitted. You are NOT continuing anyone's session and do not have $previous's reasoning. Read 'git diff' before you change anything, and $previous's own log under .agent-runs/, which is implement-$previous.log or gate-fix-$previous.log depending on which stage it ran. Treat every checked box in tasks.md as $previous's claim rather than as fact: the task text may have been revised after it was ticked, so verify each one against what the task now says and against the code, and redo any whose requirement is not actually met. If you find the inherited work already correct and complete, say so and change nothing."
  fi
}

# agent_model <agent> — the model this agent is launched with, or non-zero where its
# invocation names none. One place, because agent_command below reads it rather than
# spelling the default a second time: two spellings is how a check ends up asserting a
# model nothing runs.
#
# The default is free. Nothing here moves off it on its own; when it stops answering the
# run stops and says what to set, because spending money is not a runner's decision to
# make quietly (#297).
agent_model() {
  case "$1" in
    opencode) printf '%s' "${OPENCODE_MODEL:-opencode/muse-spark-1.2-contributor-free}" ;;
    *) return 1 ;;
  esac
}

# agent_command <agent> <role> <prompt> [resume_id] -> command string on stdout
agent_command() {
  local agent="$1" role="$2" prompt="$3" resume="${4:-}" out=""
  local timeout="${AGY_PRINT_TIMEOUT:-120m}"
  case "$agent" in
    agy)
      local r=""
      [ -n "$resume" ] && printf -v r -- '--conversation=%q ' "$resume"
      # plan mode is agy's answer to "the reviewer must not edit", and it is an APPROVAL
      # GATE rather than a sandbox: told plainly to write a file it replies that it needs
      # a Proceed first, and writes nothing. codex's -s read-only is enforced by the
      # harness and cannot be talked past; this is the agent declining, which is stronger
      # than a sentence in the prompt and weaker than codex's. Either way run-stage.sh's
      # before/after digest is what actually decides whether a reviewer edited, and this
      # does not make it unnecessary (#290).
      local mode="accept-edits"
      case "$role" in review|plan-review) mode="plan" ;; esac
      printf -v out 'agy --mode %q --print-timeout %q --output-format json %s-p=%q' \
        "$mode" "$timeout" "$r" "$prompt"
      ;;
    codex)
      local sandbox="workspace-write"
      # Both review roles are read-only where the CLI can enforce it: a reviewer that
      # cannot write is a reviewer that cannot be talked into fixing what it found.
      case "$role" in review|plan-review) sandbox="read-only" ;; esac
      # --json turns the transcript into JSONL events, which is the only form
      # carrying the thread id and the final message as data rather than prose.
      if [ -n "$resume" ]; then
        printf -v out 'codex exec --ignore-user-config --json -s %q -c model_reasoning_effort=high resume %q %q < /dev/null' \
          "$sandbox" "$resume" "$prompt"
      else
        printf -v out 'codex exec --ignore-user-config --json -s %q -c model_reasoning_effort=high %q < /dev/null' \
          "$sandbox" "$prompt"
      fi
      ;;
    claude)
      # Flags read from --help; claude is normally the orchestrator and reviews
      # in-session, so this subprocess form has had less exercise than the others.
      # --output-format json only works alongside -p, and prints one object.
      local r=""
      [ -n "$resume" ] && printf -v r -- '--resume %q ' "$resume"
      printf -v out 'claude -p --output-format json %s%q' "$r" "$prompt"
      ;;
    opencode)
      # --pure is opencode's --ignore-user-config: the run must not depend on whatever
      # plugins a given machine has installed globally.
      # < /dev/null for the same reason codex needs it above: `opencode run` reads stdin
      # and blocks forever without it. Omitting it produced a silent multi-minute hang
      # with zero bytes of output, which looks identical to a slow model (#286).
      local ro="" r=""
      # Asked for, not enforced. .opencode/agents/reviewer.md denies edit, write, patch
      # and bash, and opencode 1.18.25 honours none of them: a reviewer configured with
      # all four denied was told to write two files and wrote them, reaching for bash to
      # do it. The flag is still passed, because it costs nothing and a later opencode may
      # mean it, but it buys no guarantee. What stops a reviewer that edits is the
      # worktree digest in run-stage.sh, and for this agent it is the only thing that
      # does. #286 claimed the deny block removed the tools from the model's toolset;
      # that was true of nothing this repo has ever run against.
      case "$role" in review|plan-review) ro='--agent reviewer ' ;; esac
      [ -n "$resume" ] && printf -v r -- '-s %q ' "$resume"
      printf -v out 'opencode run --pure --format json -m %q %s%s%q < /dev/null' \
        "$(agent_model opencode)" "$ro" "$r" "$prompt"
      ;;
    *) return 1 ;;
  esac
  printf '%s' "$out"
}

# agent_step_prompt <agent> <step> <change> -> how THAT tool is told to run <step>.
# step is one of propose | apply | archive: the three OpenSpec workflow steps a stage
# hands to an agent. plan-review and review get no workflow command, because neither
# tool ships one and the review prompt is written in full by run-stage.sh.
#
# OpenSpec writes a separate command set per tool and not every tool reads the same
# one, so a single spelling for all four is a command that does not exist for three
# of them: claude dies on "Unknown command" given the workflow form, and two apply
# attempts produced nothing (#274). The caller appends the limits, which are
# workflow policy and hold whatever the spelling is.
#
# The spellings were read off the generated trees: .claude/commands/opsx/ is
# colon-separated, .agent/workflows/ and .opencode/commands/ are hyphenated. codex
# ships no OpenSpec command at all, so it gets plain instructions.
agent_step_prompt() {
  local agent="$1" step="$2" change="$3"
  case "$step" in propose|apply|archive) ;; *) return 1 ;; esac
  case "$agent" in
    claude) printf '/opsx:%s %s.' "$step" "$change" ;;
    # agy has worked with the workflow form despite docs/WORKFLOW.md recording that
    # it reads the skill form. Left as it was found until someone runs it both ways.
    agy) printf '/opsx-%s %s.' "$step" "$change" ;;
    # From .opencode/commands/, and verified: a slash command in the `opencode run`
    # message is expanded and executed, not passed through as text (#286).
    opencode) printf '/opsx-%s %s.' "$step" "$change" ;;
    codex)
      case "$step" in
        propose) printf 'Create the OpenSpec change openspec/changes/%s: write its proposal, its delta specs, its design and its tasks, following openspec/config.yaml and the schema under openspec/schemas/labeler/. Planning only.' "$change" ;;
        apply)   printf 'Implement the tasks in openspec/changes/%s, following its proposal, specs, design and tasks.' "$change" ;;
        archive) printf 'Archive the completed change openspec/changes/%s: sync every delta spec into openspec/specs/, then move the change folder to openspec/changes/archive/ prefixed with today'"'"'s date.' "$change" ;;
      esac ;;
    *) return 1 ;;
  esac
}

# The apply step by its old name. run-stage.sh and apply-tests.sh call this; keeping
# it means the generalisation above changed no caller.
agent_apply_prompt() { agent_step_prompt "$1" apply "$2"; }

# agent_extract <agent> <raw> <log> <conv> -> status word on stdout.
# Writes the agent's own answer to <log> and its resumable id to <conv>.
# Returns non-zero when no answer could be found. That is the caller's signal that <raw>
# is a console transcript rather than an answer, or that it is empty and the agent said
# nothing at all; run-stage.sh tells those two apart and reports them differently (#315).
#
# One shape was read for every agent before (#274): agy's envelope. So claude and
# codex yielded no answer on every run and neither could ever be
# resumed. Each rule below was read off that CLI's own output, not off a table.
#
# The single-object agents are searched in the LAST FIVE LINES only: an agent that
# reads another agent's `.agent-runs/*.json` echoes that envelope into its transcript,
# and a whole-file search then recorded the wrong agent's id as its own (#264).
# codex needs the whole file, because its id is printed first and its answer last;
# it is safe there because its output is one JSON event per line and an echoed file
# arrives escaped inside a single event, so no foreign line survives line-anchored
# parsing. The last agent_message and the first thread id are therefore its own.
#
# The status word is informational. What decides the run is the exit code, the files
# touched and whether this function found an answer at all.
agent_extract() {
  local agent="$1" raw="$2" log="$3" conv="$4" json="" resp="" id="" status=""
  case "$agent" in
    agy)
      json=$(tail -5 "$raw" | grep -o '{"conversation_id".*}' | tail -1)
      [ -n "$json" ] || return 1
      resp=$(printf '%s' "$json" | jq -r '.response // empty' 2>/dev/null)
      id=$(printf '%s' "$json" | jq -r '.conversation_id // empty' 2>/dev/null)
      status=$(printf '%s' "$json" | jq -r '.status // "UNKNOWN"' 2>/dev/null)
      ;;
    claude)
      # `--output-format json` prints one object and nothing else, so any brace-
      # delimited tail line is it; a line that does not parse yields no answer.
      json=$(tail -5 "$raw" | grep -o '{.*}' | tail -1)
      [ -n "$json" ] || return 1
      resp=$(printf '%s' "$json" | jq -r '.result // empty' 2>/dev/null)
      id=$(printf '%s' "$json" | jq -r '.session_id // empty' 2>/dev/null)
      status=$(printf '%s' "$json" | jq -r '.subtype // "UNKNOWN"' 2>/dev/null)
      ;;
    codex)
      # `--json` prints JSONL events: thread.started carries the id, and the answer
      # is the last item.completed of type agent_message.
      id=$(grep -m1 '^{"type":"thread.started"' "$raw" | jq -r '.thread_id // empty' 2>/dev/null)
      # Recorded before the answer is required: a run that died mid-turn still has a
      # thread, and --resume continuing it beats starting the round from scratch.
      printf '%s' "$id" > "$conv"
      resp=$(jq -sRr '[splits("\n") | fromjson?]
                      | map(select(.type == "item.completed" and .item.type == "agent_message"))
                      | last | .item.text // empty' "$raw" 2>/dev/null)
      status="OK"
      ;;
    opencode)
      # `--format json` prints one JSON event per line. sessionID rides every event,
      # the answer is the concatenation of the text parts, and step_finish carries the
      # terminal reason. Whole-file parsing is safe here for the reason codex's is:
      # one event per line, so an echoed foreign file arrives escaped inside a single
      # event and no foreign line survives line-anchored parsing.
      id=$(jq -sRr '[splits("\n") | fromjson?] | map(.sessionID // empty) | first // empty' "$raw" 2>/dev/null)
      # Written before the answer is required, as for codex: a run that died mid-turn
      # still has a session, and resuming it beats restarting the round.
      printf '%s' "$id" > "$conv"
      resp=$(jq -sRr '[splits("\n") | fromjson?]
                      | map(select(.type == "text") | .part.text // empty) | join("")' "$raw" 2>/dev/null)
      status=$(jq -sRr '[splits("\n") | fromjson?]
                        | map(select(.type == "step_finish")) | last | .part.reason // "UNKNOWN"' "$raw" 2>/dev/null)
      ;;
    *) return 1 ;;
  esac
  [ -n "$resp" ] || return 1
  printf '%s\n' "$resp" > "$log"
  # Truncated when there is no id, so a stale one from an earlier run is never resumed.
  printf '%s' "$id" > "$conv"
  printf '%s' "$status"
}

# agent_error <agent> <raw> — the tool's OWN error from a capture that yielded no answer.
# Prints one line and returns 0 when the CLI said what went wrong; returns non-zero when
# it did not, which leaves the caller with a transcript and nothing more.
#
# "we could not parse an answer" and "the tool reported an error" are different facts, and
# run-stage.sh reported the first for the second: an opencode model it cannot reach prints
# one {"type":"error"} event and exits 1, agent_extract finds no text parts in that, and the
# run came back NO_STRUCTURED_RESULT with nothing about a model in it (#297).
#
# Only opencode has a shape to read. agy and claude print a single envelope whose status
# and result agent_extract already reports, and codex ends a failed turn with an
# agent_message like any other, so neither has an error object this could name.
agent_error() {
  local agent="$1" raw="$2" msg=""
  case "$agent" in
    opencode)
      # Line-anchored, for the reason agent_extract's opencode branch is: one JSON event
      # per line, so a foreign file echoed into the transcript arrives escaped inside a
      # single event and cannot be read as an error of this run's own.
      msg=$(jq -sRr '[splits("\n") | fromjson?]
                     | map(select(.type == "error")) | last
                     | if . == null then empty
                       else (.error.name // "error") + ": "
                            + (.error.data.message // "no message")
                            + (if (.error.data.ref // "") == "" then ""
                               else " (ref " + .error.data.ref + ")" end)
                       end' "$raw" 2>/dev/null | tr '\n' ' ') ;;
    *) return 1 ;;
  esac
  [ -n "$msg" ] || return 1
  printf '%s' "$msg"
}

# agent_quota_exhausted <agent> <raw> — whether the tool said its FREE allowance ran out.
#
# Wording only. Nothing branches on this except which sentence agent_stall_advice prints,
# so a wrong answer costs precision and never a charge; that is the whole reason the run
# stops here rather than moving to a paid model on the strength of a regex.
#
# It is not invented. opencode's own retry code tests
# `e.data.responseBody?.includes("FreeUsageLimitError")` for exactly this, and this asks
# the same question of the same field. What no amount of reading settles, and running it
# did: that the field reaches `--format json` at all. Pointed at a provider returning 429
# with that body, opencode puts the whole APIError on stdout and exits 1:
#   {"type":"error","error":{"name":"APIError","data":{"message":"Free usage limit
#    exceeded","statusCode":429,"isRetryable":true,"responseBody":"{...
#    FreeUsageLimitError...}"}}}
#
# Scoped to that event's own data fields rather than the line, so a model writing the word
# in prose does not trip it. `message` is read too, because opencode falls back to the body
# when the message is empty. `GoUsageLimitError`, the PAID account's rate limit, is left
# unmatched: it is not a free allowance and saying so would be wrong.
agent_quota_exhausted() {
  local agent="$1" raw="$2"
  case "$agent" in
    opencode)
      [ -f "$raw" ] || return 1
      jq -e -sRr '[splits("\n") | fromjson?]
                  | map(select(.type == "error") | .error.data
                        | [(.responseBody // ""), (.message // "")])
                  | flatten | any(contains("FreeUsageLimitError"))' \
         "$raw" >/dev/null 2>&1 ;;
    *) return 1 ;;
  esac
}

# agent_stall_advice <agent> <raw> — what a person does next after a stage failed.
# Prints to stdout and returns non-zero where there is nothing tool-specific to say.
#
# The run STOPS either way. #297 was filed on a run that spent an hour dying a minute at
# a time with nothing naming a model, and the fix is that the failure says which model it
# was on and what to set to move off it. It is NOT that the runner moves off it: an
# allowance running out is durable account state and paying to get past it is a decision
# with a bill attached, so it is offered as a line to run and never taken automatically.
#
# The quota case is separated only to word the message accurately. If the matcher is
# wrong the cost is a less precise sentence, never an unexpected charge, which is the
# whole reason the switch this replaced is not here.
agent_stall_advice() {
  local agent="$1" raw="$2" model err
  case "$agent" in
    opencode)
      model=$(agent_model opencode)
      if agent_quota_exhausted opencode "$raw"; then
        echo "opencode's free allowance on '$model' is gone."
        echo "That is account state, not a hiccup: re-running the stage as it is will"
        echo "fail the same way until the allowance resets."
      else
        err=$(agent_error opencode "$raw") || err="the tool reported no error"
        echo "opencode failed on '$model': $err"
        echo "That is the failure the tool itself reported, whatever its cause."
      fi
      echo
      echo "To run this stage on a model that BILLS this account, re-run it with:"
      echo "  OPENCODE_MODEL=meta/muse-spark-1.2-contributor"
      echo "Nothing changes model on its own. Spending is yours to decide." ;;
    *) return 1 ;;
  esac
}

# clean_capture — filter a pty capture on stdin into plain text on stdout.
#
# Three artefacts, none of them the agent's answer:
#   ^D\b\b  BSD script(1) opens the stream with the terminal's echo of EOF, the two
#           literal characters ^D and two backspaces that erase them on a screen. It
#           broke a line-anchored match for codex's first event and prefixed
#           opencode's verbatim stdout. Stripped FIRST, as the whole four-byte
#           sequence: once the backspaces are gone a bare ^D is indistinguishable
#           from text an agent wrote.
#   ESC[…   ANSI colour and cursor sequences.
#   \r      carriage returns from the pty's line discipline.
clean_capture() {
  local bs=$'\b'
  sed "s/\\^D${bs}${bs}//g" | sed 's/\x1B\[[0-9;]*[A-Za-z]//g' | tr -d '\r'
}

# pty_run <command-string> — run under a pseudo-TTY.
# script(1) is util-linux (`-c CMD FILE`) or BSD (`FILE CMD ARGS`, no -c), and each
# form is an error on the other platform. This repo is developed on macOS with
# ubuntu CI, so assuming either breaks half the time. -e propagates the child status.
pty_run() {
  if script -q -e -c true /dev/null >/dev/null 2>&1; then
    script -q -e -c "$1" /dev/null
  else
    script -q -e /dev/null "${BASH:-/bin/bash}" -c "$1"
  fi
}

# --- machine-local role defaults (#330) -------------------------------------------
#
# Which CLIs are installed and authenticated is a property of this machine, not of
# labeler, so the four role names live in a gitignored file beside these scripts and
# never arrive by checkout. Same reasoning as CLAUDE.local.md, same suffix.
#
# ALL FOUR KEYS OR NONE. run-change.sh fills four roles and apply.sh fills two, and a
# file that satisfies one caller and not the other is a file whose meaning depends on
# who read it. One complete lineup, read the same way by both.
#
# Errors name the file and the key rather than printing a usage line: a value that came
# from a file is not fixed by reading the command's synopsis, and sending the reader
# there is sending them to the wrong place.
# roles_path <workflow-dir> - the lineup file. LABELER_ROLES_FILE overrides it, and that
# override is what keeps change-tests.sh and apply-tests.sh hermetic: both run the real
# scripts out of .workflow/, so without it whichever lineup a developer happens to have
# written would decide what the suite asserts.
roles_path() { printf '%s' "${LABELER_ROLES_FILE:-$1/roles.local}"; }

roles_template='planner: claude
plan-reviewer: codex
implementer: agy
code-reviewer: opencode'

roles_missing() { # roles_missing <path> - say where the file should be, and stop.
  echo "no agents named, and no $1 to read them from." >&2
  echo "Name all four on the command line, or write that file:" >&2
  printf '\n%s\n\n' "$roles_template" >&2
  echo "It is gitignored: it records which CLIs work on this machine, not repo policy." >&2
}

# roles_load <path> - set ROLE_PLANNER, ROLE_PLAN_REVIEWER, ROLE_IMPLEMENTER and
# ROLE_CODE_REVIEWER from <path>. Non-zero having named the file and the line.
#
# Written for bash 3.2, which is what macOS ships: no associative arrays, no ${x,,}.
# shellcheck disable=SC2034  # every ROLE_* below is read by the callers
roles_load() {
  local f="$1" n=0 line key val
  ROLE_PLANNER=""; ROLE_PLAN_REVIEWER=""; ROLE_IMPLEMENTER=""; ROLE_CODE_REVIEWER=""
  [ -f "$f" ] || { roles_missing "$f"; return 1; }
  while IFS= read -r line || [ -n "$line" ]; do
    n=$((n + 1))
    line="${line%$'\r'}"
    line="$(printf '%s' "$line" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')"
    case "$line" in ''|'#'*) continue ;; esac
    case "$line" in
      *:*) ;;
      *) echo "$f:$n: expected 'key: value', got '$line'" >&2; return 1 ;;
    esac
    key="$(printf '%s' "${line%%:*}" | sed 's/[[:space:]]//g')"
    val="$(printf '%s' "${line#*:}" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')"
    [ -n "$val" ] || { echo "$f:$n: '$key' has no value" >&2; return 1; }
    case "$val" in
      *[[:space:]]*) echo "$f:$n: '$key' takes one agent name, got '$val'" >&2; return 1 ;;
    esac
    # An unknown key is refused rather than ignored, for the reason every raw template
    # struct carries deny_unknown_fields: a key read and dropped is a preference that
    # silently did nothing, and the typo that caused it is invisible.
    case "$key" in
      planner)       [ -z "$ROLE_PLANNER" ]       || { roles_dup "$f" "$n" "$key"; return 1; }; ROLE_PLANNER="$val" ;;
      plan-reviewer) [ -z "$ROLE_PLAN_REVIEWER" ] || { roles_dup "$f" "$n" "$key"; return 1; }; ROLE_PLAN_REVIEWER="$val" ;;
      implementer)   [ -z "$ROLE_IMPLEMENTER" ]   || { roles_dup "$f" "$n" "$key"; return 1; }; ROLE_IMPLEMENTER="$val" ;;
      code-reviewer) [ -z "$ROLE_CODE_REVIEWER" ] || { roles_dup "$f" "$n" "$key"; return 1; }; ROLE_CODE_REVIEWER="$val" ;;
      *) echo "$f:$n: unknown key '$key'." >&2
         echo "The keys are planner, plan-reviewer, implementer, code-reviewer." >&2
         return 1 ;;
    esac
  done < "$f"
  roles_require "$f" planner "$ROLE_PLANNER" || return 1
  roles_require "$f" plan-reviewer "$ROLE_PLAN_REVIEWER" || return 1
  roles_require "$f" implementer "$ROLE_IMPLEMENTER" || return 1
  roles_require "$f" code-reviewer "$ROLE_CODE_REVIEWER" || return 1
}

roles_dup() { # roles_dup <file> <line> <key>
  echo "$1:$2: '$3' is set twice. One value per role." >&2
}

roles_require() { # roles_require <file> <key> <value>
  [ -n "$3" ] && return 0
  echo "$1: no '$2'. All four roles are required, even by a command that fills two:" >&2
  printf '\n%s\n\n' "$roles_template" >&2
  return 1
}
