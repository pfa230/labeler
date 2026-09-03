#!/usr/bin/env bash
# Shared fixture guard for the three workflow suites (#333).
#
# Every fixture the suites build is a write that nothing checks, and there are around 250
# of them. A write that fails leaves a case asserting against a file that was never
# written, and for a refusal case that reads as a gate which stopped firing: the exact
# signature gate-tests.sh exists to detect, arriving for a reason that has nothing to do
# with the gate. It has happened. /tmp on the development machine carries a per-user
# quota, which `df` does not report because `df` reports the filesystem, and one run alone
# with 142KiB of headroom came back 40 passed, 13 failed, two of the failures being
# refusals that never fired. change-tests.sh at 400KiB came back 213 passed, 68 failed.
#
# Checking each write would mean a helper at 250 call sites and would still miss the next
# one, so the condition is checked instead: setup() proves the fixture it just built is on
# disk, and every assertion first proves the filesystem still takes a fixture-sized write.
# Either failing ends the run, because a suite that cannot build what it asserts against
# has no verdict, and reporting one anyway is worse than reporting nothing at all.
#
# Sourced, never executed: it reads $pass and $fail from the suite that sourced it.

# Exit 3, distinct from the 2 a suite already uses for its own argument errors, so a
# caller can tell "this suite could not run" from "this suite ran and something failed".
fatal() { # fatal <what went wrong>
  printf '\nFATAL %s\n' "$1" >&2
  printf 'The %s passed and %s failed above are not a verdict: this suite could not build what it asserts against.\n' "${pass:-0}" "${fail:-0}" >&2
  exit 3
}

# Sized like a fixture rather than like a token: the filesystem that has been breaking
# these runs is one with a few hundred KiB left, where a one-byte probe passes with room
# for nothing. It proves a write is possible now, never that the last one landed, which is
# why the setup functions check their own output too.
canary() {
  local c
  c=$(mktemp 2>/dev/null) || fatal "the fixture filesystem will not create a file at all (TMPDIR=${TMPDIR:-/tmp})."
  dd if=/dev/zero of="$c" bs=1024 count=64 2>/dev/null
  if [ "$(wc -c < "$c" 2>/dev/null || echo 0)" != "65536" ]; then
    rm -f "$c"
    fatal "the fixture filesystem stopped accepting a 64KiB write under $(dirname "$c"). Fixtures are being truncated or lost."
  fi
  rm -f "$c"
}

# What a setup() built, proved present before any case reads it. The repo argument is the
# fixture's root, and it must carry a base commit: a `git commit` that could not write is
# what turned three merge cases green, because a base ref that does not resolve makes
# every capability read as new and every requirement compare against nothing.
fixture_built() { # fixture_built <repo> <file>...
  local repo="$1" f; shift
  git -C "$repo" rev-parse -q --verify HEAD >/dev/null 2>&1 \
    || fatal "the fixture repo at $repo has no base commit, so git could not write there."
  for f in "$@"; do
    [ -s "$f" ] || fatal "the fixture is incomplete: $f was not written, or was written empty."
  done
}

# The guard on the guard. What it asserts is the stop and what the stop says, through the
# one condition a test can create portably: a TMPDIR nothing can be made under. The
# condition that actually caused #333 - a filesystem that takes the directory and then
# loses the writes - needs a full disk to reproduce and no test here can make one, so
# canary() and fixture_built() are covered by this only as far as the exit they share.
# The inner run stops in its own first setup(), well before reaching its copy of this.
suite_guard_case() { # suite_guard_case <suite-path>
  local out rc
  out=$(TMPDIR=/nonexistent-fixture-root "$1" 2>&1); rc=$?
  if [ "$rc" = "3" ] && printf '%s\n' "$out" | grep -qF 'are not a verdict'; then
    pass=$((pass + 1)); printf 'ok    %s\n' "suite: a fixture filesystem that will not take a write ends the run"
  else
    fail=$((fail + 1))
    printf 'FAIL  suite: a fixture filesystem that will not take a write ends the run (wanted exit 3, got %s)\n' "$rc"
    printf '%s\n' "$out" | sed 's/^/        /' | tail -3
  fi
}
