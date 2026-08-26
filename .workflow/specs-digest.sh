#!/usr/bin/env bash
# Digest of a change's delta specs, for the staleness gate (#219).
#
#   specs-digest.sh <change-dir>            print the digest
#   specs-digest.sh <change-dir> --write    record it in the change's review.md
#
# A verdict covers the contract that was reviewed. The contract is specs/; proposal.md
# and design.md are context, and correcting a wrong sentence in them changes nothing
# about what gets built, so they are deliberately not hashed. Narrowing the rule this
# way is what makes it safe to enforce: before, a factual fix to design.md cost a full
# re-review, so the cheap move was to leave the plan wrong.
#
# What this bounds: an edit to specs/ after the verdict now fails the gate instead of
# being undetectable. It does not stop someone re-running --write to launder a stale
# verdict; that leaves a visible edit to review.md in the diff a human reads, which
# a silent edit to specs/ never did.
set -uo pipefail

change="${1:?usage: specs-digest.sh <change-dir> [--write]}"
mode="${2:-}"
[ -d "$change" ] || { echo "no such change directory: $change" >&2; exit 2; }

sha() { if command -v sha256sum >/dev/null 2>&1; then sha256sum; else shasum -a 256; fi; }

# Paths are hashed alongside contents, so renaming a capability is a change too.
digest=$(
  find "$change/specs" -type f -name '*.md' 2>/dev/null | LC_ALL=C sort | while IFS= read -r f; do
    printf '%s\n' "${f#"$change"/}"
    cat "$f"
  done | sha | cut -d' ' -f1
)

[ "$mode" = "--write" ] || { printf '%s\n' "$digest"; exit 0; }

review="$change/review.md"
[ -f "$review" ] || { echo "no review.md in $change; nothing to record against." >&2; exit 2; }
tmp="$review.tmp.$$"
grep -v '^SPECS_SHA256:' "$review" > "$tmp"
printf 'SPECS_SHA256: %s\n' "$digest" >> "$tmp"
mv "$tmp" "$review"
printf 'recorded SPECS_SHA256: %s\n' "$digest"
