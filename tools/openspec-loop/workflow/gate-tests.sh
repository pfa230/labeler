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

# fatal, canary, fixture_built and suite_guard_case: the fixture guard every suite here
# shares. Read why in the file itself; the short version is that a fixture write that
# fails silently turns a refusal case into a gate that appears to have stopped firing.
. "$here/suite-lib.sh"

expect() { # expect <want-exit> <label> <script> <args...>
  local want="$1" label="$2"; shift 2
  local out rc
  canary
  out=$("$@" 2>&1); rc=$?
  if [ "$rc" = "$want" ]; then
    pass=$((pass + 1)); printf 'ok    %s\n' "$label"
  else
    fail=$((fail + 1)); printf 'FAIL  %s (wanted exit %s, got %s)\n' "$label" "$want" "$rc"
    printf '%s\n' "$out" | sed 's/^/        /' | head -3
  fi
}

# The same, plus what the refusal said. For a case that a broken script also refuses,
# for its own wrong reason: the exit code alone would pass against the bug.
expect_says() { # expect_says <want-exit> <pattern> <label> <script> <args...>
  local want="$1" pat="$2" label="$3"; shift 3
  local out rc
  canary
  out=$("$@" 2>&1); rc=$?
  if [ "$rc" = "$want" ] && printf '%s\n' "$out" | grep -qF -- "$pat"; then
    pass=$((pass + 1)); printf 'ok    %s\n' "$label"
  else
    fail=$((fail + 1)); printf 'FAIL  %s (wanted exit %s saying "%s", got %s)\n' "$label" "$want" "$pat" "$rc"
    printf '%s\n' "$out" | sed 's/^/        /' | head -3
  fi
}

CHANGE=2026-01-01-issue-1-thing
CDIR="openspec/changes/archive/$CHANGE"
# A wellformed tree digest. It matches no tree anywhere, which is the point: the gate checks
# this field's shape and never compares it to the committed tree (#299).
TREE=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef

# A repo whose HEAD carries a published spec with two requirements, and a working
# tree where a change is landing: one requirement modified, one added, the untouched
# one left alone. Every case starts from that shape and breaks one thing.
setup() {
  repo=$(mktemp -d) || fatal "cannot create a fixture directory (TMPDIR=${TMPDIR:-/tmp})."
  cd "$repo" || fatal "cannot enter the fixture directory $repo."
  git init -q .; git config user.email t@t; git config user.name t
  mkdir -p openspec/specs/thing || fatal "cannot create the fixture's spec directory."
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

  mkdir -p "$CDIR/specs/thing" || fatal "cannot create the fixture's change directory."
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
  printf 'AUTHORS: agy, opencode\nREVIEWER: codex\nVERDICT: APPROVE\nTREE_SHA256: %s\n' "$TREE" > "$CDIR/diff-review.md"
  FILES=(openspec/specs/thing/spec.md "$CDIR/proposal.md" "$CDIR/specs/thing/spec.md" src/main.rs)
  fixture_built "$repo" openspec/specs/thing/spec.md "$CDIR/proposal.md" "$CDIR/design.md" \
                "$CDIR/specs/thing/spec.md" "$CDIR/review.md" "$CDIR/diff-review.md"
  grep -q '^SPECS_SHA256:' "$CDIR/review.md" \
    || fatal "specs-digest.sh recorded no digest in the fixture's review.md, so every staleness case below would pass for the wrong reason."
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

# A capability may be nested (`identity/user-auth`), so its name is the whole path
# under openspec/specs/. Two nested capabilities sharing a last segment are the case
# that matters: the name has to round-trip, or a delta is checked against the wrong
# published spec (#329).
setup_nested() {
  repo=$(mktemp -d) || fatal "cannot create a fixture directory (TMPDIR=${TMPDIR:-/tmp})."
  cd "$repo" || fatal "cannot enter the fixture directory $repo."
  git init -q .; git config user.email t@t; git config user.name t
  for cap in identity/user-auth billing/user-auth; do
    mkdir -p "openspec/specs/$cap" || fatal "cannot create the fixture's spec directory."
    cat > "openspec/specs/$cap/spec.md" <<EOF
# $cap

## Requirements

### Requirement: The $cap thing

The $cap thing SHALL happen.
EOF
  done
  git add -A; git commit -qm base

  mkdir -p "$CDIR/specs/identity/user-auth"
  cat > "$CDIR/specs/identity/user-auth/spec.md" <<'EOF'
## ADDED Requirements

### Requirement: Another identity thing

Another identity thing SHALL happen.
EOF
  cat >> openspec/specs/identity/user-auth/spec.md <<'EOF'

### Requirement: Another identity thing

Another identity thing SHALL happen.
EOF
  fixture_built "$repo" openspec/specs/identity/user-auth/spec.md openspec/specs/billing/user-auth/spec.md \
                "$CDIR/specs/identity/user-auth/spec.md"
}

setup_nested
expect 0 "merge: a nested capability archived faithfully" \
  "$MERGE" "$repo" openspec/specs/identity/user-auth/spec.md "$CDIR/specs/identity/user-auth/spec.md"

# The sibling's published spec was hand-edited, and this commit carries no delta for
# it. Reducing both names to "user-auth" lets the identity delta answer for it.
perl -pi -e 's/^The billing.user-auth thing SHALL happen\./The billing thing SHALL NOT happen./' openspec/specs/billing/user-auth/spec.md
expect_says 1 "openspec/specs/billing/user-auth/spec.md changed" \
  "merge: a sibling capability sharing a last segment is a different capability" \
  "$MERGE" "$repo" openspec/specs/identity/user-auth/spec.md openspec/specs/billing/user-auth/spec.md "$CDIR/specs/identity/user-auth/spec.md"
teardown

# The other half of that collision: two deltas in one commit, for capabilities differing
# only above their last segment. Reduced to one name, one delta answers for both, and the
# requirements of the other are checked against the wrong published spec.
setup_nested
mkdir -p "$CDIR/specs/billing/user-auth"
cat > "$CDIR/specs/billing/user-auth/spec.md" <<'EOF'
## ADDED Requirements

### Requirement: Another billing thing

Another billing thing SHALL happen.
EOF
cat >> openspec/specs/billing/user-auth/spec.md <<'EOF'

### Requirement: Another billing thing

Another billing thing SHALL happen.
EOF
expect 0 "merge: two sibling capabilities archived in the same commit" \
  "$MERGE" "$repo" openspec/specs/identity/user-auth/spec.md openspec/specs/billing/user-auth/spec.md \
  "$CDIR/specs/identity/user-auth/spec.md" "$CDIR/specs/billing/user-auth/spec.md"
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

dr() { printf 'AUTHORS: %s\nREVIEWER: %s\nVERDICT: %s\nTREE_SHA256: %s\n' "$1" "$2" "$3" "$4" > "$CDIR/diff-review.md"; }

dr codex codex APPROVE "$TREE"
expect 1 "gate: the diff's author is its reviewer" "$GATE" "$repo" "${FILES[@]}"
dr agy codex REVISE "$TREE"
expect 1 "gate: the diff review did not pass" "$GATE" "$repo" "${FILES[@]}"
teardown; setup

# --- who wrote it: AUTHORS is a list, and every name in it counts (#299) -----------
# The pairing rule read against a list. Compared as whole strings, "agy, codex" differs from
# "codex" and a reviewer that wrote half the code approves its own work.
dr "agy, codex" codex APPROVE "$TREE"
expect 1 "gate: the reviewer is the second name in AUTHORS" "$GATE" "$repo" "${FILES[@]}"
dr "codex, agy" codex APPROVE "$TREE"
expect 1 "gate: the reviewer is the first name in AUTHORS" "$GATE" "$repo" "${FILES[@]}"
dr "agy, Codex" codex APPROVE "$TREE"
expect 1 "gate: and case does not launder it" "$GATE" "$repo" "${FILES[@]}"
dr "agy, opencode" codex APPROVE "$TREE"
expect 0 "gate: a reviewer that wrote none of it is fine" "$GATE" "$repo" "${FILES[@]}"

# An empty list on a change that lands code claims nobody wrote it. Reachable exactly once:
# a change whose every implement stage no-opped, which is #291's own run.
dr "" codex APPROVE "$TREE"
expect 1 "gate: AUTHORS names nobody" "$GATE" "$repo" "${FILES[@]}"
dr "<VALUE>" codex APPROVE "$TREE"
expect 1 "gate: AUTHORS is still the template placeholder" "$GATE" "$repo" "${FILES[@]}"
dr "agy,,opencode" codex APPROVE "$TREE"
expect 1 "gate: AUTHORS has an empty entry between commas" "$GATE" "$repo" "${FILES[@]}"
dr "agy," codex APPROVE "$TREE"
expect 1 "gate: AUTHORS ends with a comma" "$GATE" "$repo" "${FILES[@]}"
printf 'REVIEWER: codex\nVERDICT: APPROVE\nTREE_SHA256: %s\n' "$TREE" > "$CDIR/diff-review.md"
expect 1 "gate: no AUTHORS line at all" "$GATE" "$repo" "${FILES[@]}"
printf 'AUTHORS: agy\nAUTHORS: opencode\nREVIEWER: codex\nVERDICT: APPROVE\nTREE_SHA256: %s\n' "$TREE" > "$CDIR/diff-review.md"
expect 1 "gate: two AUTHORS lines, so which one is the record is a guess" "$GATE" "$repo" "${FILES[@]}"
teardown; setup

# The fields did not swap: review.md still names one AUTHOR, and the two files are judged by
# the same code with different field names rather than by two code paths.
sed -i.bak 's/^AUTHOR: claude/AUTHORS: claude/' "$CDIR/review.md"
expect 1 "gate: review.md renamed its author field" "$GATE" "$repo" "${FILES[@]}"
teardown; setup

# --- what was judged: TREE_SHA256 is checked for shape, never for a match (#299) ---
# It cannot be compared to the committed tree, because archive, the gate fix and the commit
# message all write after the approving review. A match check would refuse every change.
dr "agy" codex APPROVE 1111111111111111111111111111111111111111111111111111111111111111
expect 0 "gate: a digest matching no tree in this repo still passes" "$GATE" "$repo" "${FILES[@]}"
printf 'AUTHORS: agy\nREVIEWER: codex\nVERDICT: APPROVE\n' > "$CDIR/diff-review.md"
expect 1 "gate: no TREE_SHA256, so nothing says what was judged" "$GATE" "$repo" "${FILES[@]}"
dr "agy" codex APPROVE "<VALUE>"
expect 1 "gate: TREE_SHA256 is still the template placeholder" "$GATE" "$repo" "${FILES[@]}"
dr "agy" codex APPROVE "${TREE%?}"
expect 1 "gate: TREE_SHA256 is 63 characters" "$GATE" "$repo" "${FILES[@]}"
dr "agy" codex APPROVE "${TREE}0"
expect 1 "gate: TREE_SHA256 is 65 characters" "$GATE" "$repo" "${FILES[@]}"
dr "agy" codex APPROVE "$(printf '%s' "$TREE" | tr 'a-f' 'A-F')"
expect 1 "gate: TREE_SHA256 is uppercase, so it is not what sha256sum writes" "$GATE" "$repo" "${FILES[@]}"
printf 'AUTHORS: agy\nREVIEWER: codex\nVERDICT: APPROVE\nTREE_SHA256: %s\nTREE_SHA256: %s\n' "$TREE" "$TREE" > "$CDIR/diff-review.md"
expect 1 "gate: two TREE_SHA256 lines" "$GATE" "$repo" "${FILES[@]}"
teardown; setup

# --- the one writer after the review that CAN be compared: the gate fix (#328) -----
# It edits src/ after the diff review approved the tree, so run-change.sh records what it
# left behind. The standing approval must be the one that judged that, or the edit lands
# unread. Absent the file no gate fix wrote anything, and there is nothing to compare.
dr "agy" codex APPROVE "$TREE"
expect 0 "gate: no gate fix, so nothing to compare" "$GATE" "$repo" "${FILES[@]}"
printf '%s\n' "$TREE" > "$CDIR/gate-fix.tree"
expect 0 "gate: the approval judged the tree the gate fix left" "$GATE" "$repo" "${FILES[@]}"
printf '2222222222222222222222222222222222222222222222222222222222222222\n' > "$CDIR/gate-fix.tree"
expect 1 "gate: a gate fix the standing approval never judged" "$GATE" "$repo" "${FILES[@]}"
rm -f "$CDIR/gate-fix.tree"
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

# --- what could not be read is not what was found to be right (#333) --------------
# Both scripts used to reach their permissive branch through a failure they could not
# see. A base ref that does not resolve is the shape a fixture takes when its commit
# failed to write, and it is what turned three merge cases and a block of gate cases
# green while saying nothing at all: every capability read as new, so nothing had a
# predecessor to be compared against.
setup_uncommitted() {
  setup
  rm -rf .git
  git init -q . && git config user.email t@t && git config user.name t \
    || fatal "cannot re-init the fixture repo without a commit."
  # A violation only the base ref can catch: this requirement is in no delta, so it is
  # checked against its published predecessor and nothing else.
  sed -i.bak 's/^The first thing SHALL happen./The first thing SHALL NOT happen./' openspec/specs/thing/spec.md
  rm -f openspec/specs/thing/spec.md.bak
}
setup_uncommitted
expect_says 2 "does not resolve to a commit" \
  "merge: an unresolvable base ref is refused, not read as a capability with nothing to displace" \
  "$MERGE" "$repo" "${FILES[@]}"
expect_says 2 "does not resolve to a commit" \
  "gate: an unresolvable base ref is refused, not read as a change that is already archived" \
  "$GATE" "$repo" "${FILES[@]}"
teardown

# The gate's own version of the same hole: a commit naming a change while the directory
# that would hold it is absent. That is a tree nothing can be read out of, and it used to
# exit 0 before a single file was opened.
setup
find openspec/changes -mindepth 0 -delete 2>/dev/null
expect_says 2 "does not exist" \
  "gate: a commit naming a change with no openspec/changes to hold it is refused" \
  "$GATE" "$repo" "${FILES[@]}"
teardown

# --- a merge has two previous commits, and one base ref cannot read them (#341) ----
# The shape that breaks the check is the back-merge: main into a change branch that has
# already archived work of its own, so both parents changed published specs and neither
# reading of the base is right about the other's. The two shapes beside it must stay
# silent or the refusal costs more than it buys, so both are asserted here too.
CDIR2=openspec/changes/archive/2026-01-02-issue-2-other

# Leaves the fixture mid-merge on a change branch, where the pre-commit caller stands.
# The argument says what main's line changed: its own published spec, or only code.
setup_back_merge() { # setup_back_merge <specs|src>
  setup
  local base; base=$(git rev-parse HEAD)
  git checkout -q -b issue-1-thing || fatal "cannot create the fixture's change branch."
  git add -A && git commit -qm "the branch archives a change of its own" \
    || fatal "cannot commit the branch's change in the fixture."
  git checkout -q -b mainline "$base" || fatal "cannot branch main's line in the fixture."
  if [ "$1" = specs ]; then
    mkdir -p "$CDIR2/specs/thing" || fatal "cannot create the second change's directory."
    printf '# Proposal\n' > "$CDIR2/proposal.md"
    cat > "$CDIR2/specs/thing/spec.md" <<'EOF'
## MODIFIED Requirements

### Requirement: The first thing

The first thing SHALL happen, once.
EOF
    sed -i.bak 's/^The first thing SHALL happen./The first thing SHALL happen, once./' openspec/specs/thing/spec.md
    rm -f openspec/specs/thing/spec.md.bak
    fixture_built "$repo" "$CDIR2/specs/thing/spec.md"
  else
    mkdir -p src || fatal "cannot create the fixture's src directory."
    printf 'fn main() {}\n' > src/main.rs
    fixture_built "$repo" src/main.rs
  fi
  git add -A && git commit -qm "main's line lands a change of its own" \
    || fatal "cannot commit main's change in the fixture."
  git checkout -q issue-1-thing || fatal "cannot return to the fixture's change branch."
  git merge --no-commit --no-ff mainline > /dev/null 2>&1
  [ -f .git/MERGE_HEAD ] || fatal "the fixture is not mid-merge: git merge left no MERGE_HEAD."
  fixture_built "$repo" openspec/specs/thing/spec.md
}

setup_back_merge specs
expect_says 2 "rebase onto main" \
  "merge: a back-merge whose parents both changed published specs has no answer, which is not a refusal" \
  "$MERGE" "$repo" "${FILES[@]}"
teardown

setup_back_merge src
expect 0 "merge: a merge whose other side changed no published spec is judged as any commit is" \
  "$MERGE" "$repo" "${FILES[@]}"
teardown

# The legitimate merge: a branch already rebased onto what it merges into, so the first
# parent is its own merge base and contributed nothing the base ref cannot see.
setup
base=$(git rev-parse HEAD)
git checkout -q -b issue-1-thing || fatal "cannot create the fixture's change branch."
git add -A && git commit -qm "the branch archives a change of its own" \
  || fatal "cannot commit the branch's change in the fixture."
git checkout -q -b mainline "$base" || fatal "cannot branch main's line in the fixture."
git merge --no-commit --no-ff issue-1-thing > /dev/null 2>&1
[ -f .git/MERGE_HEAD ] || fatal "the fixture is not mid-merge: git merge left no MERGE_HEAD."
expect 0 "merge: a branch rebased onto what it merges into is the merge shape the model handles" \
  "$MERGE" "$repo" "${FILES[@]}"
teardown

# A capability name is the whole path under the specs root, so `identity/user-auth` is one
# name (#329). The predicate's pathspec has to see that file, and this is the case that
# says so: it fails the moment the pathspec stops crossing a slash. No delta and no review
# artifacts, because the refusal happens before any of them is read.
setup_nested_back_merge() {
  repo=$(mktemp -d) || fatal "cannot create a fixture directory (TMPDIR=${TMPDIR:-/tmp})."
  cd "$repo" || fatal "cannot enter the fixture directory $repo."
  git init -q .; git config user.email t@t; git config user.name t
  mkdir -p openspec/specs/identity/user-auth || fatal "cannot create the nested capability."
  printf '# user-auth\n\n## Requirements\n\n### Requirement: A\n\nA SHALL happen.\n\n### Requirement: B\n\nB SHALL happen.\n' \
    > openspec/specs/identity/user-auth/spec.md
  git add -A && git commit -qm base || fatal "cannot write the nested fixture's base commit."
  local base; base=$(git rev-parse HEAD)
  git checkout -q -b issue-1-thing || fatal "cannot create the nested fixture's change branch."
  sed -i.bak 's/^A SHALL happen./A SHALL happen, twice./' openspec/specs/identity/user-auth/spec.md
  rm -f openspec/specs/identity/user-auth/spec.md.bak
  git add -A && git commit -qm "the branch changes the nested capability" \
    || fatal "cannot commit the nested fixture's branch change."
  git checkout -q -b mainline "$base" || fatal "cannot branch main's line in the nested fixture."
  sed -i.bak 's/^B SHALL happen./B SHALL happen, twice./' openspec/specs/identity/user-auth/spec.md
  rm -f openspec/specs/identity/user-auth/spec.md.bak
  git add -A && git commit -qm "main's line changes it too" \
    || fatal "cannot commit the nested fixture's main change."
  git checkout -q issue-1-thing || fatal "cannot return to the nested fixture's change branch."
  git merge --no-commit --no-ff mainline > /dev/null 2>&1
  [ -f .git/MERGE_HEAD ] || fatal "the nested fixture is not mid-merge: git merge left no MERGE_HEAD."
  fixture_built "$repo" openspec/specs/identity/user-auth/spec.md
}
setup_nested_back_merge
expect_says 2 "rebase onto main" \
  "merge: a back-merge over a nested capability is seen, not read as no published spec at all" \
  "$MERGE" "$repo" openspec/specs/identity/user-auth/spec.md
teardown

# --- and the hook refuses that merge where it is made (#341) ----------------------
# Against the real hooks with the real scripts beside them: a hook asserted through a copy
# of its logic asserts the copy. git splits the merge commit between two hooks, running
# pre-merge-commit for a merge it resolved itself and pre-commit for one that conflicted,
# so both paths are here, and so is the merge that must still go through.
setup_hooked() {
  repo=$(mktemp -d) || fatal "cannot create a fixture directory (TMPDIR=${TMPDIR:-/tmp})."
  cd "$repo" || fatal "cannot enter the fixture directory $repo."
  git init -q . && git symbolic-ref HEAD refs/heads/main \
    || fatal "cannot init the fixture repo on a branch named main."
  git config user.email t@t; git config user.name t
  mkdir -p .workflow .githooks src || fatal "cannot create the fixture's harness directories."
  cp "$here"/*.sh .workflow/ || fatal "cannot copy the workflow scripts into the fixture."
  cp "$here/../.githooks/pre-commit" "$here/../.githooks/pre-merge-commit" .githooks/ \
    || fatal "cannot copy the hooks into the fixture."
  chmod +x .githooks/pre-commit .githooks/pre-merge-commit .workflow/*.sh
  printf 'fn main() {}\n' > src/main.rs
  git add -A && git commit -qm base || fatal "cannot write the fixture's base commit."
  git config core.hooksPath .githooks
  git checkout -q -b other && printf 'fn other() {}\n' > src/other.rs \
    && git add -A && git commit -qm "the other side" --no-verify \
    || fatal "cannot build the fixture's other branch."
  git checkout -q main || fatal "cannot return to the fixture's main."
  fixture_built "$repo" .githooks/pre-commit .githooks/pre-merge-commit \
                .workflow/merge-shape-check.sh src/main.rs
}

# The path pre-commit never sees: git resolves the merge itself and runs no pre-commit
# hook at all, which is why this rule needs a second caller.
setup_hooked
git checkout -q -b issue-1-thing && printf 'fn thing() {}\n' > src/thing.rs \
  && git add -A && git commit -qm "the branch" --no-verify \
  || fatal "cannot build the fixture's change branch."
expect_says 1 "does not merge into itself" \
  "hook: a clean merge on a change branch is refused, and pre-commit never runs for it" \
  git merge --no-ff --no-edit other
teardown

# The other path: git could not resolve it, so no pre-merge-commit ran, and the hand
# commit is where the rule has to be. It is also what a refused clean merge leaves behind,
# since git keeps that merge in progress and tells you to finish it with git commit.
setup_hooked
git checkout -q -b issue-1-thing && printf 'fn other() { /* branch */ }\n' > src/other.rs \
  && git add -A && git commit -qm "the branch" --no-verify \
  || fatal "cannot build the fixture's change branch."
git merge --no-ff --no-edit other > /dev/null 2>&1
[ -f .git/MERGE_HEAD ] || fatal "the fixture's merge did not conflict, so this case would assert nothing."
printf 'fn other() { /* resolved */ }\n' > src/other.rs
git add src/other.rs || fatal "cannot stage the fixture's resolution."
expect_says 1 "does not merge into itself" \
  "hook: a conflicted merge committed by hand on a change branch is refused" \
  git commit -qm "resolve the merge"
teardown

setup_hooked
expect 0 "hook: a merge on main is what integration is, and still goes through" \
  git merge --no-ff --no-edit other
teardown

# A detached HEAD is not main either, and is told something else: rebasing is not the
# advice for a HEAD that is on no branch.
setup_hooked
git checkout -q --detach || fatal "cannot detach the fixture's HEAD."
expect_says 1 "on a detached HEAD" \
  "hook: a merge on a detached HEAD is refused, and not told to rebase a branch it has not got" \
  .workflow/merge-shape-check.sh
teardown

# --- and the hook actually calls the gates (#356) ----------------------------------
# The four lines that connect git to the gates were untested. gate-tests.sh proved both
# scripts thoroughly and change-tests.sh drove the hooks, but no case ever reached
# pre-commit:56-57, because no fixture had a .workflow/ for $root to point at and every
# hook case exited at the apply-lock or the merge shape first. Deleting both lines left
# all 410 assertions passing.
#
# That is this suite's own signature arriving through the wiring instead of the script: a
# gate that has stopped firing, looking exactly like one that passes. So these drive
# `git commit` itself, never the hook and never the scripts, because what is in doubt is
# only whether git reaches them.
#
# setup_hooked already copies the real hooks and the real scripts and sets core.hooksPath;
# what it lacks is a change folder for the gate to have an opinion about. This adds one
# whose plan review fails, which is the cheapest refusal review-gate-check.sh has.
setup_gated() { # setup_gated <verdict-line...> - a live change, reviewed as given
  setup_hooked
  mkdir -p "openspec/changes/issue-1-thing/specs/thing" \
    || fatal "cannot create the fixture's live change directory."
  printf '# Proposal\n' > openspec/changes/issue-1-thing/proposal.md
  printf '## ADDED Requirements\n\n### Requirement: A thing\n\nIt SHALL happen.\n' \
    > openspec/changes/issue-1-thing/specs/thing/spec.md
  [ "$#" -gt 0 ] && printf '%s\n' "$@" > openspec/changes/issue-1-thing/review.md
  fixture_built "$repo" openspec/changes/issue-1-thing/proposal.md \
                openspec/changes/issue-1-thing/specs/thing/spec.md
}

# The refusal, through git. A commit touching src/ while the live change has no passing
# plan review is what the in-flight half of review-gate-check.sh exists to stop.
setup_gated
printf 'fn added() {}\n' > src/added.rs
git add -A || fatal "cannot stage the fixture's code change."
expect_says 1 "review" \
  "hook: git commit is refused when the gate refuses, so the hook does reach it" \
  git commit -qm "code while the plan is unreviewed"
teardown

# And it is refused for the RIGHT reason. A hook that fails on its own error - a missing
# script, a bad path - also refuses the commit, and would satisfy the case above while
# proving the opposite of what it claims.
setup_gated
printf 'fn added() {}\n' > src/added.rs
git add -A || fatal "cannot stage the fixture's code change."
expect_says 1 "has no review.md" \
  "hook: and the refusal is the gate's own words, not a hook that broke on its way there" \
  git commit -qm "code while the plan is unreviewed"
teardown

# The other side of the same wire: a gate with no objection must let the commit through.
# Without this, a hook that refused everything would pass the two cases above.
setup_gated "AUTHOR: claude" "REVIEWER: codex" "VERDICT: APPROVE"
.workflow/specs-digest.sh openspec/changes/issue-1-thing --write > /dev/null \
  || fatal "cannot write the fixture's specs digest."
printf 'fn added() {}\n' > src/added.rs
git add -A || fatal "cannot stage the fixture's code change."
expect 0 "hook: an approved plan lets the same commit through" \
  git commit -qm "code with the plan approved"
teardown

# The guard on this suite's own fixtures.
suite_guard_case "$here/gate-tests.sh"

printf '\n%s passed, %s failed\n' "$pass" "$fail"
[ "$fail" = "0" ]
