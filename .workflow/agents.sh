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
      if [ -n "$resume" ]; then
        printf -v out 'codex exec --ignore-user-config -s %q -c model_reasoning_effort=high resume %q %q < /dev/null' \
          "$sandbox" "$resume" "$prompt"
      else
        printf -v out 'codex exec --ignore-user-config -s %q -c model_reasoning_effort=high %q < /dev/null' \
          "$sandbox" "$prompt"
      fi
      ;;
    claude)
      # Flags read from --help; claude is normally the orchestrator and reviews
      # in-session, so this subprocess form has had less exercise than the others.
      local r=""
      [ -n "$resume" ] && printf -v r -- '--resume %q ' "$resume"
      printf -v out 'claude -p %s%q' "$r" "$prompt"
      ;;
    opencode)
      # `opencode run [message..]` per --help; no resume flag documented there.
      printf -v out 'opencode run %q' "$prompt"
      ;;
    *) return 1 ;;
  esac
  printf '%s' "$out"
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
