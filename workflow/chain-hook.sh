#!/usr/bin/env bash
# Run the hook this kit displaced, after this kit's own checks have passed.
#
# core.hooksPath names ONE directory, so pointing it at .githooks does not add these hooks
# to the ones a consumer already ran: it replaces them. Their .git/hooks, or their husky,
# lefthook or dotfiles path, stops being consulted and git says nothing about it, which is
# a hook that has stopped firing looking exactly like a hook that passes. So setup-hooks.sh
# records what it displaced and every hook here ends by calling this. It is pre-commit's
# <hook>.legacy chain, with one directory recorded at install time instead of one rename
# per event.
#
# LAST, never first. A displaced hook that formats, stages or asks a question would be
# doing that work for a commit the gates have already refused.
#
# stdin is inherited and never read here: git hands pre-push its refs on stdin, and a
# displaced hook that receives none is deciding about a push it cannot see. Callers exec
# this as their last act, so its status is the hook's status - a refusal that is discarded
# refuses nothing.
set -euo pipefail

event="${1:?usage: chain-hook.sh <event> [hook-args...]}"; shift

# A snapshot taken at install time, so a consumer who later moves their own hooks re-runs
# setup-hooks.sh. Unset means nothing but $GIT_DIR/hooks was displaced, because that is
# what core.hooksPath replaces when it was not already set to something else.
displaced=$(git config --get --type=path hooks.displacedPath 2>/dev/null || true)
if [ -z "$displaced" ]; then
  # The common dir, not this worktree's: .git/hooks is shared, and a worktree has no
  # hooks directory of its own for git to have been running.
  displaced="$(git rev-parse --path-format=absolute --git-common-dir)/hooks"
else
  # A relative value resolves against the worktree root, which is what git itself does
  # with a relative core.hooksPath. Resolved against $PWD it would find nothing from a
  # subdirectory, and finding nothing is this file's silent pass.
  case $displaced in
    /*) ;;
    *) displaced="$(git rev-parse --show-toplevel)/$displaced" ;;
  esac
fi

hook="$displaced/$event"
[ -x "$hook" ] || exit 0
exec "$hook" "$@"
