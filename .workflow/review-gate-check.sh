#!/usr/bin/env bash
# Single source of truth for the review gate (#191, #219, #223).
#
# Called by .githooks/pre-commit and by CI, so the two cannot drift. Tool-agnostic:
# it inspects files, never which agent produced them, so Claude Code, agy, codex and
# opencode are gated identically.
#
# Two populations, checked differently because they are at different points:
#
#   A change LANDING in this commit (its folder arrives under openspec/changes/archive/)
#   is checked whatever it touches. Its plan verdict must pass, specs/ must match the
#   digest that verdict recorded, and its diff review must pass. This is the last
#   moment anything can be checked, so nothing is exempt.
#
#   A change IN FLIGHT (a live folder under openspec/changes/) is checked only when the
#   commit touches src/ or ui/src/, which keeps the planning loop itself writable.
#
# Usage: review-gate-check.sh [--plan-only] <repo-root> <changed-file>...
#
#   --plan-only    skip the diff-review check. For callers that fire DURING
#                  implementation, when no diff review can exist yet: run-stage.sh's
#                  pre-flight probe and the Claude Code edit-time hook.
#   GATE_BASE_REF  ref the commit is measured against. Default HEAD, which is right
#                  for pre-commit. CI must set it to the push or PR base, because
#                  there HEAD is the commit under test.
#
# Exit 0 = allowed. Exit 1 = refused, reason on stderr.
set -uo pipefail

# The digest tool that ships beside this script is the one it calls: resolving it
# through the repo under check works only while that repo is this one.
here=$(cd "$(dirname "$0")" && pwd)

plan_only=0
if [ "${1:-}" = "--plan-only" ]; then plan_only=1; shift; fi

root="${1:?repo root required}"; shift || true
[ "$#" -gt 0 ] || exit 0
base_ref="${GATE_BASE_REF:-HEAD}"

changes_dir="$root/openspec/changes"
[ -d "$changes_dir" ] || exit 0

failed=0
fail() { printf 'review gate: %s\n' "$1" >&2; failed=1; }

field() { # field <file> <name> -> value, or the literal "__AMBIGUOUS__"/"__MISSING__"
  local file="$1" name="$2" n
  n=$(grep -c "^${name}:" "$file" || true)
  [ "$n" = "1" ] || { printf '__%s__' "$([ "$n" = "0" ] && echo MISSING || echo AMBIGUOUS)"; return; }
  grep "^${name}:" "$file" | sed "s/^${name}:[[:space:]]*//" | tr -d '[:space:]'
}

# Both reviews carry the same canonical fields, so they are judged by the same code.
# Role flexibility is only safe if the reviewer is not the author: any agent may
# propose or apply, and nobody reviews their own work.
check_roles() { # check_roles <file> <label>
  local file="$1" label="$2" reviewer author
  reviewer=$(field "$file" REVIEWER)
  author=$(field "$file" AUTHOR)
  case "$reviewer" in __MISSING__|__AMBIGUOUS__) fail "$label needs exactly one 'REVIEWER:' line naming the model or CLI that wrote it."; return ;; esac
  case "$author"   in __MISSING__|__AMBIGUOUS__) fail "$label needs exactly one 'AUTHOR:' line naming who wrote what is under review."; return ;; esac
  [ -n "$reviewer" ] && [ "$reviewer" != "<VALUE>" ] || { fail "$label: 'REVIEWER:' is unfilled."; return; }
  [ -n "$author" ]   && [ "$author"   != "<VALUE>" ] || { fail "$label: 'AUTHOR:' is unfilled."; return; }
  if [ "$(printf '%s' "$reviewer" | tr '[:upper:]' '[:lower:]')" = "$(printf '%s' "$author" | tr '[:upper:]' '[:lower:]')" ]; then
    fail "$label: reviewer and author are both '${reviewer}'. Nobody reviews their own work; use a different agent or a fresh-context subagent."
  fi
}

check_plan_review() { # check_plan_review <change-dir> <name>
  local change="$1" name="$2" review verdict applied recorded actual
  review="$change/review.md"
  [ -f "$review" ] || { fail "change '$name' has no review.md. The plan must be adversarially reviewed before implementation."; return; }

  verdict=$(field "$review" VERDICT)
  case "$verdict" in
    __MISSING__|__AMBIGUOUS__)
      fail "change '$name': review.md needs exactly one line starting 'VERDICT:'. A verdict quoted in prose is ambiguous."; return ;;
    APPROVE) ;;
    APPROVE_WITH_CHANGES)
      applied=$(field "$review" CHANGES_APPLIED)
      [ "$applied" = "yes" ] || fail "change '$name': verdict is APPROVE_WITH_CHANGES but CHANGES_APPLIED is '${applied}'. Apply the Required Changes and have the reviewer re-check them." ;;
    REVISE)
      fail "change '$name': verdict is REVISE. Fix the artifacts and re-run the full review in a fresh context."; return ;;
    *)
      fail "change '$name': unreadable verdict '${verdict}'. Expected APPROVE, APPROVE_WITH_CHANGES or REVISE."; return ;;
  esac

  check_roles "$review" "change '$name': review.md"

  # A verdict covers the contract it read. specs/ is the contract; proposal.md and
  # design.md are context, and correcting a wrong sentence in them is free on purpose.
  recorded=$(field "$review" SPECS_SHA256)
  case "$recorded" in
    __MISSING__|__AMBIGUOUS__)
      fail "change '$name': review.md needs exactly one 'SPECS_SHA256:' line. Run .workflow/specs-digest.sh <change-dir> --write once the review has a verdict." ;;
    *)
      actual=$("$here/specs-digest.sh" "$change" 2>/dev/null)
      [ "$recorded" = "$actual" ] || fail "change '$name': specs/ has changed since the verdict (recorded ${recorded:0:12}, now ${actual:0:12}). A change to the contract voids the review; re-run it in a fresh context." ;;
  esac
}

# The diff review is the last check before code lands, so it is gated where it lands.
check_diff_review() { # check_diff_review <change-dir> <name>
  local change="$1" name="$2" dr verdict
  dr="$change/diff-review.md"
  [ -f "$dr" ] || { fail "change '$name' lands with no diff-review.md. The implementation diff is reviewed by an agent that did not write it; .workflow/apply.sh records the verdict."; return; }
  verdict=$(field "$dr" VERDICT)
  case "$verdict" in
    APPROVE) ;;
    __MISSING__|__AMBIGUOUS__) fail "change '$name': diff-review.md needs exactly one line starting 'VERDICT:'."; return ;;
    *) fail "change '$name': diff review verdict is '$verdict', not APPROVE."; return ;;
  esac
  check_roles "$dr" "change '$name': diff-review.md"
}

# Changes landing in this commit: an archived folder that did not exist at the base
# ref. Checked whatever the commit touches, because there is no later moment.
archived=""
for f in "$@"; do
  case "$f" in
    openspec/changes/archive/*/*)
      dir=$(printf '%s' "$f" | cut -d/ -f1-4)
      case " $archived " in *" $dir "*) ;; *) archived="$archived $dir" ;; esac ;;
  esac
done
for dir in $archived; do
  git -C "$root" show "$base_ref:$dir/proposal.md" >/dev/null 2>&1 && continue   # already archived, not landing now
  name=$(basename "$dir")
  check_plan_review "$root/$dir" "$name"
  [ "$plan_only" = "1" ] || check_diff_review "$root/$dir" "$name"
done

# Changes in flight: gated only when the commit carries implementation code, so that
# the planning and review loop itself stays writable.
guarded=0
for f in "$@"; do
  case "$f" in src/*|ui/src/*) guarded=1 ;; esac
done
if [ "$guarded" = "1" ]; then
  for change in "$changes_dir"/*/; do
    name=$(basename "$change")
    [ "$name" = "archive" ] && continue
    [ -d "$change" ] || continue
    check_plan_review "$change" "$name"
  done
fi

exit "$failed"
