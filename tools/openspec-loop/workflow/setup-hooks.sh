#!/usr/bin/env bash
# Point git at the committed hooks. Run once per clone, and once per worktree is
# unnecessary: core.hooksPath is repo-wide.
set -euo pipefail
# The hooks live beside this script inside the subtree, so point git at them there
# rather than at a copy in the consumer: one spelling, and no drift between them.
git config core.hooksPath "$(cd "$(dirname "$0")/../.githooks" && pwd | sed "s|^$(git rev-parse --show-toplevel)/||")"
echo "core.hooksPath = $(git config core.hooksPath)"
