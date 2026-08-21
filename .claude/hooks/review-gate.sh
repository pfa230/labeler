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

deny() {
  jq -n --arg r "$1" '{hookSpecificOutput:{hookEventName:"PreToolUse",permissionDecision:"deny",permissionDecisionReason:$r}}'
  exit 0
}

payload=$(cat)
tool=$(printf '%s' "$payload" | jq -r '.tool_name // empty')
cwd=$(printf '%s' "$payload" | jq -r '.cwd // empty')
[ -n "$cwd" ] || exit 0

# Only guard implementation code. Docs, specs and the change folder stay writable so
# the review loop itself can proceed.
guarded() { case "$1" in *src/*) return 0 ;; *) return 1 ;; esac; }

case "$tool" in
  Edit|Write|NotebookEdit)
    target=$(printf '%s' "$payload" | jq -r '.tool_input.file_path // empty')
    [ -n "$target" ] || exit 0
    guarded "$target" || exit 0
    ;;
  Bash)
    # A shell can write a file a hundred ways (redirection, tee, sed -i, mv, cp,
    # heredocs, python -c). Matching them exactly is a losing game, so this is
    # deliberately broad: if the command names a guarded path AND looks like it
    # mutates something, refuse while the gate is closed. Best effort only; the
    # model-agnostic backstop is the CI check, since this hook governs Claude Code
    # alone and never sees another CLI's writes.
    target=$(printf '%s' "$payload" | jq -r '.tool_input.command // empty')
    [ -n "$target" ] || exit 0
    guarded "$target" || exit 0
    printf '%s' "$target" | grep -Eq '>>?|[[:space:]]tee[[:space:]]|sed[[:space:]]+-i|[[:space:]](mv|cp|dd|truncate|install|patch|rm)[[:space:]]|<<' || exit 0
    ;;
  *) exit 0 ;;
esac

changes_dir="$cwd/openspec/changes"
[ -d "$changes_dir" ] || exit 0

for change in "$changes_dir"/*/; do
  name=$(basename "$change")
  [ "$name" = "archive" ] && continue
  [ -d "$change" ] || continue

  review="$change/review.md"
  if [ ! -f "$review" ]; then
    deny "OpenSpec change '$name' has no review.md. Adversarial review gates implementation (#182). Run the review artifact before editing this path."
  fi

  # Exactly one VERDICT line, or we refuse. Taking the first match let a verdict
  # quoted in prose (a finding, an example, a rebuttal) override the real one.
  vcount=$(grep -c '^VERDICT:' "$review" || true)
  [ "$vcount" = "1" ] || deny "OpenSpec change '$name': review.md has $vcount lines starting with VERDICT:, expected exactly 1. A verdict quoted in prose is ambiguous; keep one canonical line."
  verdict=$(grep '^VERDICT:' "$review" | sed 's/^VERDICT:[[:space:]]*//' | tr -d '[:space:]' || true)
  case "$verdict" in
    APPROVE) ;;
    APPROVE_WITH_CHANGES)
      acount=$(grep -c '^CHANGES_APPLIED:' "$review" || true)
      [ "$acount" = "1" ] || deny "OpenSpec change '$name': review.md has $acount lines starting with CHANGES_APPLIED:, expected exactly 1."
      applied=$(grep '^CHANGES_APPLIED:' "$review" | sed 's/^CHANGES_APPLIED:[[:space:]]*//' | tr -d '[:space:]' || true)
      [ "$applied" = "yes" ] || deny "OpenSpec change '$name': VERDICT is APPROVE_WITH_CHANGES but CHANGES_APPLIED is '${applied:-unset}'. Apply the Required Changes and have the reviewer re-check them before editing this path."
      ;;
    REVISE)
      deny "OpenSpec change '$name': VERDICT is REVISE. Fix the artifacts and re-run the full review in a fresh context before editing this path."
      ;;
    *)
      deny "OpenSpec change '$name': review.md has no readable VERDICT line (found '${verdict:-none}'). Expected APPROVE, APPROVE_WITH_CHANGES or REVISE on its own line."
      ;;
  esac
done
exit 0
