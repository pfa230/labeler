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
# --filter <substring>: run only the cases whose label contains it. The suites are linear
# and change-tests.sh is 304 cases at about eight minutes, so a one-line fixture fix used
# to cost a full pass to re-check. Matching is on the label an assertion already carries,
# because that is what a reader has in front of them when a case fails.
#
# Skipped cases are counted and reported, so a filter that matches nothing says so instead
# of printing a clean zero that reads like a pass.
SUITE_FILTER=""
skipped=0
suite_parse_args() { # suite_parse_args "$@"
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --filter) SUITE_FILTER="${2:?--filter needs a substring}"; shift 2 ;;
      --filter=*) SUITE_FILTER="${1#*=}"; shift ;;
      *) shift ;;
    esac
  done
}
suite_selected() { # suite_selected <label>
  [ -z "$SUITE_FILTER" ] && return 0
  case "$1" in *"$SUITE_FILTER"*) return 0 ;; esac
  skipped=$((skipped + 1)); return 1
}

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
  # The injection the guard below uses. TMPDIR cannot serve: macOS mktemp resolves its
  # directory through confstr(_CS_DARWIN_USER_TEMP_DIR) and ignores TMPDIR entirely, so
  # the guard's unwritable root was silently ignored, the inner run ran to completion,
  # reached its own copy of the guard, and recursed until the machine was out of
  # processes (#356).
  [ -n "${SUITE_FIXTURE_FAIL:-}" ] && fatal "the fixture filesystem is failing writes (injected by SUITE_FIXTURE_FAIL)."
  c=$(mktemp 2>/dev/null) || fatal "the fixture filesystem will not create a file at all (TMPDIR=${TMPDIR:-/tmp})."
  dd if=/dev/zero of="$c" bs=1024 count=64 2>/dev/null
  # Arithmetic, not string equality: BSD `wc` pads its count to a column width, so
  # `wc -c` returns "   65536" on macOS and "65536" on the GNU coreutils CI runs. Compared
  # as strings the two never match, and this guard then aborted every local run on a
  # filesystem with hundreds of gigabytes free, which is the inverse of the failure it was
  # written for and hid it just as thoroughly (#356).
  if [ "$(( $(wc -c < "$c" 2>/dev/null || echo 0) ))" -ne 65536 ]; then
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
  # Checked after the real ones, so an injected run still exercises them first.
  [ -n "${SUITE_FIXTURE_FAIL:-}" ] && fatal "the fixture filesystem is failing writes (injected by SUITE_FIXTURE_FAIL)."
  return 0
}

# The guard on the guard. What it asserts is the stop and what the stop says. The condition
# that actually caused #333 - a filesystem that takes the directory and then loses the
# writes - needs a full disk to reproduce and no test here can make one, so canary() and
# fixture_built() are covered by this only as far as the exit they share. Since that was
# already true, the failure is injected and says so, rather than being staged through an
# environment variable whose effect turns out to be per-platform.
#
# It was staged that way: TMPDIR=/nonexistent-fixture-root, on the assumption that the
# inner run would stop in its own first setup(). macOS mktemp ignores TMPDIR, so on a Mac
# the inner run instead completed, reached its own copy of this function, and recursed
# without bound. A guard that hangs the machine it is meant to protect is worse than the
# phantom pass it was written to catch (#356).
#
# SUITE_FIXTURE_FAIL is unset for the inner run's own descendants because the inner run
# stops before it launches any, and the stop is what is under test.
suite_guard_case() { # suite_guard_case <suite-path>
  local out rc
  suite_selected "suite: a fixture filesystem that will not take a write ends the run" || return 0
  out=$(SUITE_FIXTURE_FAIL=1 "$1" 2>&1); rc=$?
  if [ "$rc" = "3" ] && printf '%s\n' "$out" | grep -qF 'are not a verdict'; then
    pass=$((pass + 1)); printf 'ok    %s\n' "suite: a fixture filesystem that will not take a write ends the run"
  else
    fail=$((fail + 1))
    printf 'FAIL  suite: a fixture filesystem that will not take a write ends the run (wanted exit 3, got %s)\n' "$rc"
    printf '%s\n' "$out" | sed 's/^/        /' | tail -3
  fi
}
