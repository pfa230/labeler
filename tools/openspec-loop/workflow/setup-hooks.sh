#!/usr/bin/env bash
# Point git at the committed hooks. Run once per clone, and once per worktree is
# unnecessary: core.hooksPath is repo-wide.
set -euo pipefail
git config core.hooksPath .githooks
echo "core.hooksPath = $(git config core.hooksPath)"
