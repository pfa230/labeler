#!/usr/bin/env bash
# PreToolUse gate for OpenSpec changes (#182).
#
# Refuses writes to implementation code while this working directory has an active
# OpenSpec change whose review.md has not passed. OpenSpec itself only checks that
# artifacts exist, never their contents, so this is what makes the review gate real.
#
# Scoped by working directory, which is why AGENTS.md mandates one worktree per
# change (#185): a worktree with no active change never engages this hook, so
# unrelated and hotfix work is unaffected.
set -euo pipefail

payload=$(cat)
file=$(printf '%s' "$payload" | jq -r '.tool_input.file_path // empty')
cwd=$(printf '%s' "$payload" | jq -r '.cwd // empty')
[ -n "$file" ] || exit 0
[ -n "$cwd" ] || exit 0

# Only guard implementation code. Docs, specs and the change folder stay writable so
# the review loop itself can proceed.
case "$file" in
  *"/src/"*|*"/ui/src/"*) ;;
  *) exit 0 ;;
esac

changes_dir="$cwd/openspec/changes"
[ -d "$changes_dir" ] || exit 0

deny() {
  jq -n --arg r "$1" '{hookSpecificOutput:{hookEventName:"PreToolUse",permissionDecision:"deny",permissionDecisionReason:$r}}'
  exit 0
}

for change in "$changes_dir"/*/; do
  name=$(basename "$change")
  [ "$name" = "archive" ] && continue
  [ -d "$change" ] || continue

  review="$change/review.md"
  if [ ! -f "$review" ]; then
    deny "OpenSpec change '$name' has no review.md. Adversarial review gates implementation (#182). Run the review artifact before editing $file."
  fi

  verdict=$(grep -m1 '^VERDICT:' "$review" | sed 's/^VERDICT:[[:space:]]*//' | tr -d '[:space:]' || true)
  case "$verdict" in
    APPROVE) ;;
    APPROVE_WITH_CHANGES)
      applied=$(grep -m1 '^CHANGES_APPLIED:' "$review" | sed 's/^CHANGES_APPLIED:[[:space:]]*//' | tr -d '[:space:]' || true)
      [ "$applied" = "yes" ] || deny "OpenSpec change '$name': VERDICT is APPROVE_WITH_CHANGES but CHANGES_APPLIED is '${applied:-unset}'. Apply the Required Changes and have the reviewer re-check them before editing $file."
      ;;
    REVISE)
      deny "OpenSpec change '$name': VERDICT is REVISE. Fix the artifacts and re-run the full review in a fresh context before editing $file."
      ;;
    *)
      deny "OpenSpec change '$name': review.md has no readable VERDICT line (found '${verdict:-none}'). Expected APPROVE, APPROVE_WITH_CHANGES or REVISE on its own line."
      ;;
  esac
done
exit 0
