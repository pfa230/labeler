#!/usr/bin/env bash
# A change branch rebases onto main; it never merges into itself (#341).
#
# Every check that reads history reads it through one base ref, and a merge leaves two
# previous commits for that one ref to explain. archive-merge-check.sh then reports the
# parent it was not pointed at as a hand-edit: on 3bbd2bf that was five complaints against
# requirements both sides had archived correctly, and the branch could not land at all,
# because CI failed the same check that --no-verify had skipped locally. Refused here
# rather than left to memory, because the workaround it teaches is --no-verify, which
# switches off the review gate in the same stroke.
#
# main is where a merge is legitimate: integration is --ff-only and leaves no commit at
# all, and a deliberate --no-ff there merges a branch already rebased onto main, which is
# the one merge shape the checks' model does handle.
#
# Two hooks call this, because git splits the merge commit between them and neither sees
# both halves. An automatic merge runs pre-merge-commit and never pre-commit; a merge that
# conflicted runs neither, and then pre-commit when the resolution is committed by hand.
# Checked on git 2.53: a clean `git merge` fires pre-merge-commit, prepare-commit-msg,
# commit-msg and post-merge, and MERGE_HEAD does not exist yet at pre-merge-commit, which
# is why the caller says a merge is in progress rather than this script asking.
#
# Usage: merge-shape-check.sh
set -euo pipefail

branch=$(git symbolic-ref --quiet --short HEAD || true)
[ "$branch" = "main" ] && exit 0

# A subtree update is a merge, and refusing it here left `git subtree pull` workable only
# on main: the branch that wants the new loop is exactly the branch this would stop. It is
# also not the shape the rule guards against. A back-merge gives the history checks a
# second previous commit carrying the same paths they read; a subtree merge carries only
# the prefix, and openspec/specs/ is not in it, so archive-merge-check.sh reads the one
# parent it was pointed at and is right to.
#
# Recognised by the prefix, not by the commit message: `git subtree` records
# git-subtree-dir: on every add and pull it makes, so the prefixes are already in history,
# and a merge touching nothing outside one of them is a subtree update whatever its
# message says. Read from the index rather than from MERGE_HEAD, which does not exist yet
# at pre-merge-commit.
prefixes=$(git log --format=%B | sed -n 's/^git-subtree-dir:[[:space:]]*//p' | sed 's#/*$##' | sort -u)
if [ -n "$prefixes" ]; then
  changed=$(git diff --cached --name-only HEAD 2>/dev/null || true)
  if [ -n "$changed" ]; then
    outside=0
    while IFS= read -r f; do
      [ -n "$f" ] || continue
      inside=0
      while IFS= read -r pfx; do
        [ -n "$pfx" ] || continue
        case "$f" in "$pfx"/*) inside=1 ;; esac
      done <<EOF
$prefixes
EOF
      [ "$inside" = "1" ] || { outside=1; break; }
    done <<EOF
$changed
EOF
    [ "$outside" = "0" ] && exit 0
  fi
fi

# A detached HEAD is refused too, and is told something different, because it is not a
# change branch and rebasing is not the advice it needs. The rule is where merges happen,
# not what this HEAD is: main is the only place, so anything else stops here.
if [ -z "$branch" ]; then
  echo "merge-shape: this is a merge on a detached HEAD, and merges happen on main (#341)." >&2
  echo "merge-shape: git merge --abort, then check out the branch this belongs on." >&2
  exit 1
fi

echo "merge-shape: this is a merge on '$branch', and a change branch does not merge into itself (#341)." >&2
echo "merge-shape: rebase instead. git merge --abort, then git rebase origin/main, and push with --force-with-lease." >&2
exit 1
