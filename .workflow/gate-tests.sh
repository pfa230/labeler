#!/usr/bin/env bash
# Tests for the two gate scripts (#218, #219, #223).
#
# A gate that stops firing is indistinguishable from a gate that passes, and that
# failure is silent by construction: everything goes green. Both scripts have already
# done it once each during development, one by receiving a single unsplit argument and
# one through an over-broad escape hatch, so the cases that matter here are the
# negative ones. Every case asserts an exit code, and most assert a refusal.
#
# Self-contained: builds a throwaway git repo per case, so it depends on no history
# and can run anywhere. Usage: .workflow/gate-tests.sh
set -uo pipefail

here=$(cd "$(dirname "$0")" && pwd)
GATE="$here/review-gate-check.sh"
MERGE="$here/archive-merge-check.sh"
pass=0; fail=0

expect() { # expect <want-exit> <label> <script> <args...>
  local want="$1" label="$2"; shift 2
  local out rc
  out=$("$@" 2>&1); rc=$?
  if [ "$rc" = "$want" ]; then
    pass=$((pass + 1)); printf 'ok    %s\n' "$label"
  else
    fail=$((fail + 1)); printf 'FAIL  %s (wanted exit %s, got %s)\n' "$label" "$want" "$rc"
    printf '%s\n' "$out" | sed 's/^/        /' | head -3
  fi
}

CHANGE=2026-01-01-issue-1-thing
CDIR="openspec/changes/archive/$CHANGE"

# A repo whose HEAD carries a published spec with two requirements, and a working
# tree where a change is landing: one requirement modified, one added, the untouched
# one left alone. Every case starts from that shape and breaks one thing.
setup() {
  repo=$(mktemp -d)
  cd "$repo" || exit 2
  git init -q .; git config user.email t@t; git config user.name t
  mkdir -p openspec/specs/thing
  cat > openspec/specs/thing/spec.md <<'EOF'
# thing

## Purpose

Testing.

## Requirements

### Requirement: The first thing

The first thing SHALL happen.

### Requirement: The second thing

The second thing SHALL happen.
EOF
  git add -A; git commit -qm base

  mkdir -p "$CDIR/specs/thing"
  printf '# Proposal\n' > "$CDIR/proposal.md"
  printf 'Design prose.\n' > "$CDIR/design.md"
  cat > "$CDIR/specs/thing/spec.md" <<'EOF'
## MODIFIED Requirements

### Requirement: The second thing

The second thing SHALL happen, twice.

## ADDED Requirements

### Requirement: The third thing

The third thing SHALL happen.
EOF
  cat > openspec/specs/thing/spec.md <<'EOF'
# thing

## Purpose

Testing.

## Requirements

### Requirement: The first thing

The first thing SHALL happen.

### Requirement: The second thing

The second thing SHALL happen, twice.

### Requirement: The third thing

The third thing SHALL happen.
EOF
  printf 'AUTHOR: claude\nREVIEWER: codex\nVERDICT: APPROVE\n' > "$CDIR/review.md"
  "$here/specs-digest.sh" "$CDIR" --write > /dev/null
  printf 'AUTHOR: agy\nREVIEWER: codex\nVERDICT: APPROVE\n' > "$CDIR/diff-review.md"
  FILES=(openspec/specs/thing/spec.md "$CDIR/proposal.md" "$CDIR/specs/thing/spec.md" src/main.rs)
}

teardown() {
  cd "$here" || exit 2
  [ -n "${repo:-}" ] && [ -d "$repo" ] && find "$repo" -mindepth 0 -delete 2>/dev/null
  repo=""
}

# --- the merge check: is openspec/specs/ the delta applied to the base? -----------
setup
expect 0 "merge: a faithful archive passes" "$MERGE" "$repo" "${FILES[@]}"

sed -i.bak 's/^The first thing SHALL happen./The first thing SHALL NOT happen./' openspec/specs/thing/spec.md
expect 1 "merge: a requirement no delta names was rewritten" "$MERGE" "$repo" "${FILES[@]}"
mv openspec/specs/thing/spec.md.bak openspec/specs/thing/spec.md

perl -0pi -e 's/### Requirement: The first thing.*?(?=### Requirement:)//s' openspec/specs/thing/spec.md
expect 1 "merge: a requirement no delta removes disappeared" "$MERGE" "$repo" "${FILES[@]}"
teardown; setup

sed -i.bak 's/^The third thing SHALL happen./The third thing SHALL happen, differently./' openspec/specs/thing/spec.md
expect 1 "merge: what landed is not what the delta said" "$MERGE" "$repo" "${FILES[@]}"
teardown; setup

expect 1 "merge: a published spec edited with no delta behind it" "$MERGE" "$repo" openspec/specs/thing/spec.md
expect 1 "merge: an ADDED+MODIFIED delta archived without syncing its capability" "$MERGE" "$repo" "$CDIR/specs/thing/spec.md"

# Each half of that check on its own, so removing either half fails a case.
cat > "$CDIR/specs/thing/spec.md" <<'EOF'
## ADDED Requirements

### Requirement: The third thing

The third thing SHALL happen.
EOF
expect 1 "merge: an ADDED-only delta, unsynced" "$MERGE" "$repo" "$CDIR/specs/thing/spec.md"

cat > "$CDIR/specs/thing/spec.md" <<'EOF'
## MODIFIED Requirements

### Requirement: The second thing

The second thing SHALL happen, twice.
EOF
expect 1 "merge: a MODIFIED-only delta, unsynced" "$MERGE" "$repo" "$CDIR/specs/thing/spec.md"

# A removal that was never synced leaves the requirement published, so it is refused
# while the capability still exists...
cat > "$CDIR/specs/thing/spec.md" <<'EOF'
## REMOVED Requirements

### Requirement: The second thing

The second thing SHALL happen.
EOF
expect 1 "merge: a REMOVED-only delta, unsynced, capability still published" "$MERGE" "$repo" "$CDIR/specs/thing/spec.md"

# ...and deleting the published file is a retirement, legitimate only if the delta
# removed every requirement it had. This delta removes one of two, so the deletion
# takes a requirement nobody named. Both file lists are exercised, because
# pre-commit's omits deletions and CI's does not.
find openspec/specs/thing -type f -name '*.md' -delete
expect 1 "merge: a retirement that removes only some of the requirements" "$MERGE" "$repo" "$CDIR/specs/thing/spec.md"
expect 1 "merge: the same, seen the way CI sees it, with the deletion in the list" "$MERGE" "$repo" openspec/specs/thing/spec.md "$CDIR/specs/thing/spec.md"

cat > "$CDIR/specs/thing/spec.md" <<'EOF'
## REMOVED Requirements

### Requirement: The first thing

The first thing SHALL happen.

### Requirement: The second thing

The second thing SHALL happen.
EOF
expect 0 "merge: a retirement that removes every requirement" "$MERGE" "$repo" "$CDIR/specs/thing/spec.md"
expect 0 "merge: the same, with the deletion in the list as CI sends it" "$MERGE" "$repo" openspec/specs/thing/spec.md "$CDIR/specs/thing/spec.md"

# A missing published spec is only ever a retirement. It must not become a way for
# an addition to go unchecked.
cat >> "$CDIR/specs/thing/spec.md" <<'EOF'

## ADDED Requirements

### Requirement: The fourth thing

The fourth thing SHALL happen.
EOF
expect 1 "merge: a retirement that also ADDs, with nowhere for the addition to land" "$MERGE" "$repo" "$CDIR/specs/thing/spec.md"
teardown; setup

find openspec/specs/thing -type f -name '*.md' -delete
git add -A; git commit -qm "retire thing"          # so the capability is absent at the base ref too
cat > "$CDIR/specs/thing/spec.md" <<'EOF'
## ADDED Requirements

### Requirement: A brand new thing

The brand new thing SHALL happen.
EOF
expect 1 "merge: a brand-new capability whose delta was never synced anywhere" "$MERGE" "$repo" "$CDIR/specs/thing/spec.md"
teardown; setup

# A requirement under some other level-two header inherits no operation from the
# section above it, so a retirement cannot smuggle one in as a removal.
find openspec/specs/thing -type f -name '*.md' -delete
cat > "$CDIR/specs/thing/spec.md" <<'EOF'
## REMOVED Requirements

### Requirement: The first thing

The first thing SHALL happen.

### Requirement: The second thing

The second thing SHALL happen.

## Notes

### Requirement: A stray thing

The stray thing SHALL happen.
EOF
expect 1 "merge: a requirement under an unrelated header is not a removal" "$MERGE" "$repo" "$CDIR/specs/thing/spec.md"
teardown

# --- the review gate: plan verdict, staleness, diff review ------------------------
setup
expect 0 "gate: a complete landing commit passes" "$GATE" "$repo" "${FILES[@]}"

printf 'Corrected sentence.\n' >> "$CDIR/design.md"
expect 0 "gate: correcting design.md is free, it is context not contract" "$GATE" "$repo" "${FILES[@]}"

printf '\nAnd more.\n' >> "$CDIR/specs/thing/spec.md"
expect 1 "gate: specs/ edited after the verdict is stale" "$GATE" "$repo" "${FILES[@]}"
teardown; setup

grep -v '^SPECS_SHA256:' "$CDIR/review.md" > r.tmp && mv r.tmp "$CDIR/review.md"
expect 1 "gate: a verdict with no digest cannot be checked" "$GATE" "$repo" "${FILES[@]}"
teardown; setup

sed -i.bak 's/^REVIEWER: codex/REVIEWER: Claude/' "$CDIR/review.md"
expect 1 "gate: the plan's author is its reviewer" "$GATE" "$repo" "${FILES[@]}"
teardown; setup

mv "$CDIR/diff-review.md" "$CDIR/dr.bak"
expect 1 "gate: landing with no diff review" "$GATE" "$repo" "${FILES[@]}"
expect 1 "gate: still refused when the commit carries no code at all" "$GATE" "$repo" openspec/specs/thing/spec.md "$CDIR/proposal.md"
expect 0 "gate: --plan-only exempts the diff review" "$GATE" --plan-only "$repo" "${FILES[@]}"
mv "$CDIR/dr.bak" "$CDIR/diff-review.md"
printf '\nAnd more.\n' >> "$CDIR/specs/thing/spec.md"
expect 1 "gate: --plan-only still checks the plan, or it exempts everything" "$GATE" --plan-only "$repo" "${FILES[@]}"
teardown; setup

printf 'AUTHOR: codex\nREVIEWER: codex\nVERDICT: APPROVE\n' > "$CDIR/diff-review.md"
expect 1 "gate: the diff's author is its reviewer" "$GATE" "$repo" "${FILES[@]}"
printf 'AUTHOR: agy\nREVIEWER: codex\nVERDICT: REVISE\n' > "$CDIR/diff-review.md"
expect 1 "gate: the diff review did not pass" "$GATE" "$repo" "${FILES[@]}"
teardown; setup

expect 0 "gate: a commit touching neither code nor a landing change is not its business" "$GATE" "$repo" docs/WORKFLOW.md
teardown

# --- the other population: a change still in flight, not yet archived -------------
setup
LIVE=openspec/changes/issue-2-live
mkdir -p "$LIVE/specs/thing"
cp "$CDIR/specs/thing/spec.md" "$LIVE/specs/thing/spec.md"
printf 'AUTHOR: claude\nREVIEWER: codex\nVERDICT: REVISE\n' > "$LIVE/review.md"
"$here/specs-digest.sh" "$LIVE" --write > /dev/null
expect 1 "gate: code committed while a live change has not passed its plan review" "$GATE" "$repo" src/main.rs
expect 0 "gate: the same live change does not block a docs commit" "$GATE" "$repo" docs/WORKFLOW.md
printf 'AUTHOR: claude\nREVIEWER: codex\nVERDICT: APPROVE\n' > "$LIVE/review.md"
"$here/specs-digest.sh" "$LIVE" --write > /dev/null
expect 0 "gate: an approved live change lets code through" "$GATE" "$repo" src/main.rs
printf '\nAnd more.\n' >> "$LIVE/specs/thing/spec.md"
expect 1 "gate: a live change whose specs moved after the verdict" "$GATE" "$repo" src/main.rs
teardown

printf '\n%s passed, %s failed\n' "$pass" "$fail"
[ "$fail" = "0" ]
