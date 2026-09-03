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

lower() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]'; }

# Both reviews carry the same canonical fields, so they are judged by the same code. Only
# the author field's name differs, and it is passed in: a plan has one author, so review.md
# names AUTHOR, while code can have several, so diff-review.md names AUTHORS and carries a
# comma-separated list (#299).
#
# Role flexibility is only safe if the reviewer is not an author: any agent may propose or
# apply, and nobody reviews their own work. With a list that reads "the reviewer appears
# nowhere in it", which is the same rule counted properly rather than a weaker one.
check_roles() { # check_roles <file> <label> <author-field>
  local file="$1" label="$2" afield="$3" reviewer author rest one
  reviewer=$(field "$file" REVIEWER)
  author=$(field "$file" "$afield")
  case "$reviewer" in __MISSING__|__AMBIGUOUS__) fail "$label needs exactly one 'REVIEWER:' line naming the model or CLI that wrote it."; return ;; esac
  case "$author"   in __MISSING__|__AMBIGUOUS__) fail "$label needs exactly one '$afield:' line naming who wrote what is under review."; return ;; esac
  [ -n "$reviewer" ] && [ "$reviewer" != "<VALUE>" ] || { fail "$label: 'REVIEWER:' is unfilled."; return; }
  # An empty author list on a change that lands code claims nobody wrote it. It is
  # reachable in one place: a change whose every implement stage no-opped, which
  # run-stage.sh permits for a handover. There is no default and no grace period, because a
  # default here is the same silent pass the empty list already is.
  [ -n "$author" ] && [ "$author" != "<VALUE>" ] || { fail "$label: '$afield:' is unfilled. Code that lands was written by somebody; name them."; return; }
  case "$author" in
    ,*|*,) fail "$label: '$afield:' begins or ends with a comma, so one entry is empty."; return ;;
  esac
  # Split on commas so one path serves a single name and a list. field() has already
  # stripped the whitespace, so an entry is empty only if the list really carries one.
  rest="$author"
  while [ -n "$rest" ]; do
    case "$rest" in
      *,*) one="${rest%%,*}"; rest="${rest#*,}" ;;
      *)   one="$rest"; rest="" ;;
    esac
    [ -n "$one" ] || { fail "$label: '$afield:' has an empty entry between commas."; return; }
    if [ "$(lower "$one")" = "$(lower "$reviewer")" ]; then
      fail "$label: '${reviewer}' is both the reviewer and named in '$afield'. Nobody reviews their own work; use a different agent or a fresh-context subagent."
      return
    fi
  done
}

# 64 lowercase hex, exactly once. Shape only, and deliberately: see check_diff_review.
check_digest() { # check_digest <file> <name> <field> <what>
  local file="$1" name="$2" fname="$3" what="$4" v
  v=$(field "$file" "$fname")
  case "$v" in
    __MISSING__)   fail "change '$name': $(basename "$file") needs exactly one '$fname:' line, naming $what."; return ;;
    __AMBIGUOUS__) fail "change '$name': $(basename "$file") carries more than one '$fname:' line, so which one is the record is a guess."; return ;;
  esac
  case "$v" in
    ''|*[!0-9a-f]*) fail "change '$name': '$fname:' is not a digest: '${v}'. Expected 64 lowercase hex characters."; return ;;
  esac
  [ "${#v}" -eq 64 ] || fail "change '$name': '$fname:' is ${#v} characters, not 64."
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
      [ "$applied" = "yes" ] || fail "change '$name': verdict is APPROVE_WITH_CHANGES but CHANGES_APPLIED is '${applied}'. Apply the Required Changes and record CHANGES_APPLIED: yes." ;;
    REVISE)
      fail "change '$name': verdict is REVISE. Fix the artifacts and re-run the full review in a fresh context."; return ;;
    *)
      fail "change '$name': unreadable verdict '${verdict}'. Expected APPROVE, APPROVE_WITH_CHANGES or REVISE."; return ;;
  esac

  check_roles "$review" "change '$name': review.md" AUTHOR

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
  local change="$1" name="$2" dr verdict recorded approved
  dr="$change/diff-review.md"
  [ -f "$dr" ] || { fail "change '$name' lands with no diff-review.md. The implementation diff is reviewed by an agent that did not write it; .workflow/apply.sh records the verdict."; return; }
  verdict=$(field "$dr" VERDICT)
  case "$verdict" in
    APPROVE) ;;
    __MISSING__|__AMBIGUOUS__) fail "change '$name': diff-review.md needs exactly one line starting 'VERDICT:'."; return ;;
    *) fail "change '$name': diff review verdict is '$verdict', not APPROVE."; return ;;
  esac
  check_roles "$dr" "change '$name': diff-review.md" AUTHORS
  # The tree the approving review was given. Checked for shape and never against the
  # committed tree, which is a check that cannot hold: archive moves the folder and syncs
  # openspec/specs/, and the commit-message stage runs after that, so the committed tree is
  # never the reviewed tree and a match check would refuse every change. The value is
  # compared where the failure actually happens: round to round, live, in apply.sh (#299),
  # and against the gate fix below.
  check_digest "$dr" "$name" TREE_SHA256 "the tree the review judged"

  # The one stage that edits code after this review is the gate fix, and it used to land
  # unread (#328). run-change.sh records the digest that round left behind, so the approval
  # can be checked against it here: the review that stands must be the one that judged what
  # the gate fix wrote. Absent the file no gate fix edited anything, and there is nothing to
  # check - which is also all this can say, since a fix made outside the driver records
  # nothing and the shape-only rule above is what covers the rest.
  [ -f "$change/gate-fix.tree" ] || return 0
  approved=$(field "$dr" TREE_SHA256)
  case "$approved" in __MISSING__|__AMBIGUOUS__) return 0 ;; esac   # check_digest said so already
  recorded=$(tr -d '[:space:]' < "$change/gate-fix.tree")
  [ "$recorded" = "$approved" ] || fail "change '$name': a gate fix left the tree at ${recorded:0:12}, and the approving diff review judged ${approved:0:12}. That edit was never reviewed; run the diff review over it before this lands."
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
