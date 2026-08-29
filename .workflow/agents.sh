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
# Each entry below records how its form was arrived at. That is a comment, not a
# runtime check: whether an invocation works is decided by running it, and the
# runner already reports a failed launch, an unstructured result and a no-op.

# Roles: implement (may write) | review (must not write).
#
# Where a CLI can enforce read-only itself, the review role uses it. That makes
# "the reviewer does not edit" a property of how it was launched rather than an
# instruction it is asked to respect.

agent_known() {
  case "$1" in claude|agy|codex|opencode) return 0 ;; *) return 1 ;; esac
}

# agent_command <agent> <role> <prompt> [resume_id] -> command string on stdout
agent_command() {
  local agent="$1" role="$2" prompt="$3" resume="${4:-}" out=""
  local timeout="${AGY_PRINT_TIMEOUT:-120m}"
  case "$agent" in
    agy)
      local r=""
      [ -n "$resume" ] && printf -v r -- '--conversation=%q ' "$resume"
      # agy has no read-only mode; the review role relies on the prompt plus the
      # commit-time gate. Noted rather than papered over.
      printf -v out 'agy --mode accept-edits --print-timeout %q --output-format json %s-p=%q' \
        "$timeout" "$r" "$prompt"
      ;;
    codex)
      local sandbox="workspace-write"
      [ "$role" = "review" ] && sandbox="read-only"
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
      # `opencode run [message..]` per --help; no resume flag documented there.
      printf -v out 'opencode run %q' "$prompt"
      ;;
    *) return 1 ;;
  esac
  printf '%s' "$out"
}

# agent_apply_prompt <agent> <change> -> how THAT tool is told to run the apply step.
#
# OpenSpec writes a separate command set per tool and not every tool reads the same
# one, so a single spelling for all four is a command that does not exist for three
# of them: claude dies on "Unknown command" given the workflow form, and two apply
# attempts produced nothing (#274). The caller appends the limits, which are
# workflow policy and hold whatever the spelling is.
agent_apply_prompt() {
  local change="$2"
  case "$1" in
    # From .claude/commands/opsx/apply.md; colon-separated (docs/WORKFLOW.md).
    claude) printf '/opsx:apply %s.' "$change" ;;
    # agy has worked with the workflow form despite docs/WORKFLOW.md recording that
    # it reads the skill form. Left as it was found until someone runs it both ways.
    agy) printf '/opsx-apply %s.' "$change" ;;
    # From .opencode/commands/; unverified, as docs/WORKFLOW.md already says.
    opencode) printf '/opsx-apply %s.' "$change" ;;
    # codex ships no OpenSpec command at all, so it gets plain instructions.
    codex) printf 'Implement the tasks in openspec/changes/%s, following its proposal, specs, design and tasks.' "$change" ;;
    *) return 1 ;;
  esac
}

# agent_extract <agent> <raw> <log> <conv> -> status word on stdout.
# Writes the agent's own answer to <log> and its resumable id to <conv>.
# Returns non-zero when no answer could be found, which is the caller's signal that
# <raw> is a console transcript rather than a review.
#
# One shape was read for every agent before (#274): agy's envelope. So claude and
# codex reported NO_STRUCTURED_RESULT on every run and neither could ever be
# resumed. Each rule below was read off that CLI's own output, not off a table.
#
# The single-object agents are searched in the LAST FIVE LINES only: an agent that
# reads another agent's `.agent-*.json` echoes that envelope into its transcript,
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
      # No structured output documented, so `opencode run`'s stdout is the answer
      # itself and there is nothing to resume from. Copied rather than round-tripped
      # through a variable, so the bytes are the ones the agent printed.
      [ -s "$raw" ] || return 1
      cp "$raw" "$log"
      : > "$conv"
      printf 'OK'
      return 0
      ;;
    *) return 1 ;;
  esac
  [ -n "$resp" ] || return 1
  printf '%s\n' "$resp" > "$log"
  # Truncated when there is no id, so a stale one from an earlier run is never resumed.
  printf '%s' "$id" > "$conv"
  printf '%s' "$status"
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
