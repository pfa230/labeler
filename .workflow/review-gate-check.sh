#!/usr/bin/env bash
# Single source of truth for the review gate (#191).
#
# Called by .githooks/pre-commit and by CI, so the two cannot drift. Tool-agnostic:
# it inspects files, never which agent produced them, so Claude Code, agy, codex and
# opencode are gated identically.
#
# Usage: review-gate-check.sh <repo-root> <changed-file>...
# Exit 0 = allowed. Exit 1 = refused, reason on stderr.
set -uo pipefail

root="${1:?repo root required}"; shift || true
[ "$#" -gt 0 ] || exit 0

# Only implementation code is gated. Docs, specs and the change folder stay writable
# so the review loop itself can proceed.
guarded=0
for f in "$@"; do
  case "$f" in src/*|ui/src/*) guarded=1 ;; esac
done
[ "$guarded" = "1" ] || exit 0

changes_dir="$root/openspec/changes"
[ -d "$changes_dir" ] || exit 0

fail() { printf 'review gate: %s\n' "$1" >&2; exit 1; }

field() { # field <file> <name> -> value, or the literal "__AMBIGUOUS__"/"__MISSING__"
  local file="$1" name="$2" n
  n=$(grep -c "^${name}:" "$file" || true)
  [ "$n" = "1" ] || { printf '__%s__' "$([ "$n" = "0" ] && echo MISSING || echo AMBIGUOUS)"; return; }
  grep "^${name}:" "$file" | sed "s/^${name}:[[:space:]]*//" | tr -d '[:space:]'
}

for change in "$changes_dir"/*/; do
  name=$(basename "$change")
  [ "$name" = "archive" ] && continue
  [ -d "$change" ] || continue

  review="$change/review.md"
  [ -f "$review" ] || fail "change '$name' has no review.md. The plan must be adversarially reviewed before implementation."

  verdict=$(field "$review" VERDICT)
  case "$verdict" in
    __MISSING__|__AMBIGUOUS__)
      fail "change '$name': review.md needs exactly one line starting 'VERDICT:'. A verdict quoted in prose is ambiguous." ;;
    APPROVE) ;;
    APPROVE_WITH_CHANGES)
      applied=$(field "$review" CHANGES_APPLIED)
      [ "$applied" = "yes" ] || fail "change '$name': verdict is APPROVE_WITH_CHANGES but CHANGES_APPLIED is '${applied}'. Apply the Required Changes and have the reviewer re-check them." ;;
    REVISE)
      fail "change '$name': verdict is REVISE. Fix the artifacts and re-run the full review in a fresh context." ;;
    *)
      fail "change '$name': unreadable verdict '${verdict}'. Expected APPROVE, APPROVE_WITH_CHANGES or REVISE." ;;
  esac

  # Role flexibility is only safe if the reviewer is not the author. Any agent may
  # propose or apply; nobody reviews their own work.
  reviewer=$(field "$review" REVIEWER)
  author=$(field "$review" AUTHOR)
  case "$reviewer" in __MISSING__|__AMBIGUOUS__) fail "change '$name': review.md needs exactly one 'REVIEWER:' line naming the model or CLI that wrote it." ;; esac
  case "$author"   in __MISSING__|__AMBIGUOUS__) fail "change '$name': review.md needs exactly one 'AUTHOR:' line naming who wrote the artifacts under review." ;; esac
  [ -n "$reviewer" ] && [ "$reviewer" != "<VALUE>" ] || fail "change '$name': 'REVIEWER:' is unfilled."
  [ -n "$author" ]   && [ "$author"   != "<VALUE>" ] || fail "change '$name': 'AUTHOR:' is unfilled."
  if [ "$(printf '%s' "$reviewer" | tr '[:upper:]' '[:lower:]')" = "$(printf '%s' "$author" | tr '[:upper:]' '[:lower:]')" ]; then
    fail "change '$name': reviewer and author are both '${reviewer}'. Nobody reviews their own work; use a different agent or a fresh-context subagent."
  fi
done
exit 0
