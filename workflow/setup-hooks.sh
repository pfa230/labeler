#!/usr/bin/env bash
# Point git at the committed hooks, keeping the ones this repo already ran. Run once per
# clone, and once per worktree is unnecessary: core.hooksPath is repo-wide.
#
# core.hooksPath is one directory and not a search path, so setting it does not add these
# hooks to a consumer's: it replaces them, silently. Whatever was configured is therefore
# recorded in hooks.displacedPath before it is taken over, and chain-hook.sh runs it after
# these hooks' own checks. That is pre-commit's migration - it renames the hook it displaces
# to <hook>.legacy and calls it - done once for a directory rather than once per event.
set -euo pipefail

root=$(git rev-parse --show-toplevel)
# The hooks live beside this script inside the subtree, so point git at them there
# rather than at a copy in the consumer: one spelling, and no drift between them.
ours=$(cd "$(dirname "$0")/../.githooks" && pwd | sed "s|^$root/||")
current=$(git config --get --type=path core.hooksPath 2>/dev/null || true)

resolve() { # resolve <path> - absolute, or empty when there is no such directory
  case $1 in
    '') ;;
    /*) (cd "$1" 2>/dev/null && pwd) ;;
    *) (cd "$root" && cd "$1" 2>/dev/null && pwd) ;;
  esac
}

# Installing over an install records nothing and leaves the first one's record standing.
# Recording ourselves would point the chain at the hooks doing the chaining, and every
# commit would then run this repo's pre-commit from inside itself.
if [ -n "$current" ] && [ "$(resolve "$current")" != "$(resolve "$ours")" ]; then
  git config --local hooks.displacedPath "$current"
  echo "displaced: $current, which these hooks keep running after their own checks"
fi

git config core.hooksPath "$ours"
echo "core.hooksPath = $(git config core.hooksPath)"
