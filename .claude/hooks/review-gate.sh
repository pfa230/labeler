#!/usr/bin/env bash
# Early local signal for the review gate. NOT the gate itself (#191).
#
# The gate is scripts/review-gate-check.sh, called by .githooks/pre-commit and by
# CI, so it applies to every agent equally. This hook calls the same script so the
# rules cannot drift, and exists only because failing at edit time is friendlier
# than failing at commit time. It sees Claude Code alone.
#
# Repo root is resolved from the TARGET FILE, not from cwd (#190): a session sitting
# at the repo root editing a worktree's file by absolute path must still be judged
# against that worktree's change.
#
# Bash is deliberately NOT matched. A shell command's text is not its write target:
# `cat > design.md` quoting src/api.rs was refused as if it wrote to src/. Guessing
# targets out of shell text produces false positives that block legitimate planning
# work, and the git hook already catches the real thing by inspecting staged paths.
set -uo pipefail

payload=$(cat)
tool=$(printf '%s' "$payload" | jq -r '.tool_name // empty')

case "$tool" in
  Edit|Write|NotebookEdit) target=$(printf '%s' "$payload" | jq -r '.tool_input.file_path // empty') ;;
  *) exit 0 ;;
esac
[ -n "$target" ] || exit 0

# Walk up from the target to the repo that owns it.
dir=$(dirname "$target")
root=""
while [ "$dir" != "/" ] && [ -n "$dir" ]; do
  if [ -e "$dir/.git" ]; then root="$dir"; break; fi
  dir=$(dirname "$dir")
done
[ -n "$root" ] || exit 0

rel="${target#"$root"/}"
reason=$("$root/scripts/review-gate-check.sh" "$root" "$rel" 2>&1 >/dev/null) && exit 0
jq -n --arg r "$reason" '{hookSpecificOutput:{hookEventName:"PreToolUse",permissionDecision:"deny",permissionDecisionReason:$r}}'
exit 0
