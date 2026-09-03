#!/usr/bin/env bash
# The three cargo gates, and the one thing the driver could not decide about them: whose
# failure is this? (#298)
#
# run-change.sh read any non-zero from fmt, clippy or test as the change's fault, so on a
# machine where the suite does not pass at HEAD it could never finish: one fix round, the
# same failures again, and a stop reading "that is a defect, not a lint". During #235 that
# threw away a reviewed, approved, archived change over 16 failures that fail identically
# at the base commit (#288).
#
# The diff reviewer that hit the same 16 answered it by hand: check out the base commit in
# a throwaway worktree, run the suite there, subtract. That is what this does.
#
# WHEN IT RUNS. Only after the gates have already failed, so the passing path - which is
# almost every run - costs nothing, and the extra suite is paid for on a path that was
# going to stop anyway. The baseline is cached against the commit it was measured at, so
# the fix round's re-run does not pay for it a second time. It builds in a target directory
# of its own, for the reason gate_tests_at gives, so the first one is a cold build.
#
# WHAT IS NEVER SUBTRACTED. fmt and clippy. They are deterministic and a pre-existing lint
# is not a thing this repo tolerates, so they fail outright, exactly as before.
#
# WHAT IT REFUSES TO GUESS. Every question it cannot answer - no base commit to compare
# against, a suite that failed without naming a single test, a failure set missing a target
# that died mid-run or a failure cargo counted, a baseline that would not build - is
# answered "this change's", which is the answer that stops the run. A driver
# that waved a change through because it could not tell would be worse than the stop this
# replaces, not better.

# Which gate failed, as run_gates reports it. Defined here because the mapping is used on
# both sides: run-change.sh exits with these and gate_attribute branches on them.
GATE_FMT_FAILED=1
GATE_CLIPPY_FAILED=2
GATE_TEST_FAILED=3
GATE_UI_FAILED=4

# Every line goes to the run's transcript AND to the gate log, because that log is what the
# gate-fix prompt hands the implementer: a fixer told which failures are not its to fix
# does not spend its one round on them.
GATE_ECHO_LOG=/dev/null
gate_out() { printf '%s\n' "$1" >> "$GATE_ECHO_LOG" 2>/dev/null; printf '%s\n' "$1"; }
gate_err() { printf '%s\n' "$1" >> "$GATE_ECHO_LOG" 2>/dev/null; printf '%s\n' "$1" >&2; }

# The failing tests in a cargo test log, one "<target> :: <test>" per line, sorted.
#
# Keyed by target as well as by name, because cargo runs several binaries and a name is
# only unique within one. Merged on the name alone, a new failure in tests/http_tests.rs
# would be subtracted away by a pre-existing failure of the same name in the unit tests.
# The target is taken from cargo's own "Running <target> (<path>)" banner with the
# parenthesised path dropped, since that path carries a build hash that differs between
# two worktrees and would make every entry incomparable.
#
# The `test <name> ... FAILED` shape is matched exactly, with nothing allowed after FAILED.
# That is what libtest prints on the toolchain rust-toolchain.toml pins: the variants that
# append to the line (`--report-time`, a time limit) are nightly-only, and stable rejects
# the flag outright ("only accepted on the nightly compiler"). Verified against a real
# 109-failure run of this suite, where the count below agrees exactly.
#
# The tightness is deliberate but not trusted: a suffix a future toolchain adds would drop
# that failure from the set, and a dropped failure on the change's side is read as
# pre-existing, which is the misattribution this file exists to end. gate_parse_complete
# below turns that silent drop into a stop.
gate_failed_tests() { # gate_failed_tests <cargo-test-log>
  [ -f "$1" ] || return 0
  awk '
    /^[[:space:]]*Running / || /^[[:space:]]*Doc-tests / {
      line = $0
      sub(/^[[:space:]]*/, "", line)
      sub(/^Running /, "", line)
      sub(/[[:space:]]*\([^()]*\)[[:space:]]*$/, "", line)
      target = line
      next
    }
    /^test .* \.\.\. FAILED[[:space:]]*$/ {
      name = $0
      sub(/^test /, "", name)
      sub(/[[:space:]]*\.\.\.[[:space:]]*FAILED[[:space:]]*$/, "", name)
      printf "%s :: %s\n", (target == "" ? "?" : target), name
    }
  ' "$1" | LC_ALL=C sort -u
}

# How many tests cargo itself says failed, summed over every target's summary line. Read
# only as a check on the parse above, never as a source of names.
gate_reported_failures() { # gate_reported_failures <cargo-test-log>
  # The count is the field before "failed;", or before "failed" when it ends the line.
  [ -f "$1" ] || { printf '0'; return 0; }
  awk '/^[[:space:]]*test result:/ {
         for (i = 2; i <= NF; i++) if ($i ~ /^failed;?$/) total += $(i - 1)
       }
       END { printf "%d", total + 0 }' "$1"
}

# The test targets cargo started, and the ones that reported a result. Every target that
# runs prints a "Running <target>" or "Doc-tests" banner first and a "test result:" summary
# last, so a banner with no summary is a binary that died in the middle of its run.
gate_targets_started() { # gate_targets_started <cargo-test-log>
  local n; n=$(grep -cE '^[[:space:]]*(Running|Doc-tests)' "$1" 2>/dev/null) || n=0
  printf '%s' "${n:-0}"
}
gate_targets_finished() { # gate_targets_finished <cargo-test-log>
  local n; n=$(grep -cE '^[[:space:]]*test result:' "$1" 2>/dev/null) || n=0
  printf '%s' "${n:-0}"
}

# Whether the failure set is the whole set, asked two ways, because a set that is missing a
# failure is subtracted as if that failure had passed at the base.
#
# Every target reported. A binary that dies mid-run prints its banner and no summary, and
# the failures it never got to is invisible in the count: measured, not imagined. An
# abort() in one test file of this repo, run with --no-fail-fast, left 8 banners against 7
# summaries while another target reported 2 failures and this read exactly those 2. The
# count arm below calls that complete, so without this arm the subtraction would have run
# on a set missing a whole binary, and a change that crashed the harness would have been
# reported as not this change's.
#
# Every failure read. Fewer read than cargo counted means a result line this does not
# recognise was dropped; more means something that is not one was read as a result and will
# excuse a real failure at the base. Both corrupt the comparison.
#
# A target that fails to compile is neither case: cargo builds every target before it runs
# any, so a build error means nothing ran at all - 0 banners, 0 summaries, 0 failures,
# verified on this repo - and that falls through to the caller's "named no failing test"
# refusal.
gate_parse_complete() { # gate_parse_complete <cargo-test-log> <failures-file> <where>
  local started finished reported parsed
  started=$(gate_targets_started "$1")
  finished=$(gate_targets_finished "$1")
  if [ "$started" != "$finished" ]; then
    gate_err "cargo started $started test target(s) $3 and $finished reported a result, so one died"
    gate_err "mid-run and whatever it would have failed on is missing from the set. Nothing can be"
    gate_err "subtracted from it. Every failure counts as this change's. The output is in $1."
    return 1
  fi
  reported=$(gate_reported_failures "$1")
  parsed=$(grep -c . "$2" 2>/dev/null) || parsed=0
  [ "$reported" = "$parsed" ] && return 0
  gate_err "cargo counted $reported failing test(s) $3 and this read $parsed of them, so the two"
  gate_err "sets are not comparable and nothing can be subtracted. Every failure counts as this"
  gate_err "change's. The output is in $1."
  return 1
}

# The commit this branch forked from: the tree as it is without this change. Never HEAD,
# which on a re-run after the commit already carries the change. No default branch found
# means no baseline, and that is reported rather than papered over.
gate_base_commit() { # gate_base_commit <worktree>
  local wt="$1" head ref base
  head=$(git -C "$wt" rev-parse --verify HEAD 2>/dev/null) || return 1
  for ref in "$(git -C "$wt" symbolic-ref --short refs/remotes/origin/HEAD 2>/dev/null)" \
             origin/main origin/master main master; do
    [ -n "$ref" ] || continue
    git -C "$wt" rev-parse --verify --quiet "$ref" >/dev/null 2>&1 || continue
    base=$(git -C "$wt" merge-base "$head" "$ref" 2>/dev/null) || continue
    [ -n "$base" ] && { printf '%s' "$base"; return 0; }
  done
  return 1
}

# Whether this change touches ui/. Two distinct questions: did the committed range
# touch ui/, and does the working tree touch ui/. The first covers a change that has
# already committed its ui/ edits; the second covers unstaged, staged and untracked
# edits, which `status --porcelain` already reports. One `diff` against the base and
# one `status` answer both. A fresh worktree has no ui/node_modules (gitignored), so
# running the ui gate unconditionally would charge every Rust or harness change an
# npm ci (#354).
gate_ui_touches() { # gate_ui_touches <worktree> -> 0 if ui/ is touched
  local wt="$1" base
  base=$(gate_base_commit "$wt" 2>/dev/null) || base=""
  if [ -n "$base" ] && git -C "$wt" diff --name-only "$base" HEAD -- ui/ 2>/dev/null | grep -q .; then return 0; fi
  if git -C "$wt" status --porcelain -- ui/ 2>/dev/null | grep -q .; then return 0; fi
  return 1
}

# The suite at the base commit, in a worktree that is NOT the one being written: the change
# is uncommitted working state, so checking the base out over it would destroy the very
# work being judged. Scratch lives outside the repository, so no .worktrees/ scan and no
# apply.sh change resolution can trip over it.
#
# Returns cargo's own status, or 125 when the run never happened at all. The caller needs
# that distinction: a suite that failed is what we came to measure, a suite that could not
# be started measures nothing.
gate_tests_at() { # gate_tests_at <worktree> <commit> <logfile>
  local wt="$1" sha="$2" log="$3" tmp scratch rc
  : > "$log" 2>/dev/null || return 125
  tmp=$(mktemp -d 2>/dev/null) || return 125
  scratch="$tmp/base"
  if ! git -C "$wt" worktree add --detach "$scratch" "$sha" >> "$log" 2>&1; then
    rm -rf "$tmp" 2>/dev/null
    return 125
  fi
  # A target directory of its own: cargo is handed CARGO_TARGET_DIR pointing inside the
  # change's target/, so neither the change's own build directory nor whatever the
  # environment named is what it builds into. The value cargo is given is the guarantee;
  # what a cargo wrapper on PATH does with it afterwards is beyond this. Cargo does not key this package's artifacts on the tree they were built in, so a
  # shared directory lets each tree run the other's binaries: two checkouts with identical
  # Rust sources - which is every change that touches only docs or .workflow/ - are read as
  # fresh, and whichever compiled last leaves its `env!("CARGO_MANIFEST_DIR")` baked into
  # the binary both then run. src/errors.rs:653 reads docs/SPEC.md through exactly that, and
  # sharing the directory while building this change left `cargo test` in its own worktree
  # failing with `read SPEC.md: Os { code: 2, kind: NotFound }` against a scratch path that
  # had already been removed. A baseline measuring the change's own binary, or a change
  # stopped by the baseline's, is the misattribution this file exists to end.
  #
  # It costs a cold build of every dependency the first time. Kept under the change's
  # target/, which is gitignored and disposable, so that is paid once per worktree rather
  # than once per attempt.
  ( cd "$scratch" && CARGO_TARGET_DIR="$wt/target/baseline" cargo test --no-fail-fast ) >> "$log" 2>&1
  rc=$?
  git -C "$wt" worktree remove --force "$scratch" >/dev/null 2>&1
  rm -rf "$tmp" 2>/dev/null
  git -C "$wt" worktree prune >/dev/null 2>&1
  return "$rc"
}

# Whether a failed gate run is this change's. Zero means it is not: every failure fails
# identically at the base commit, and the run may go on. One means it is, or that it could
# not be told apart from one, which is the same answer here.
gate_attribute() { # gate_attribute <worktree> <gates-exit> <gates-log> <base-log>
  local wt="$1" rc="$2" log="$3" blog="$4"
  local base brc mine theirs new n

  GATE_ECHO_LOG="$log"
  case "$rc" in
    "$GATE_TEST_FAILED") ;;
    "$GATE_FMT_FAILED")
      gate_err "cargo fmt --check failed: the tree is not formatted, or the formatter itself"
      gate_err "errored. Neither is a thing a baseline excuses."
      return 1 ;;
    "$GATE_CLIPPY_FAILED")
      gate_err "clippy failed. It is deterministic, and a pre-existing lint is not a thing this"
      gate_err "repo tolerates, so it is never measured against the base."
      return 1 ;;
    "$GATE_UI_FAILED")
      gate_err "ui gate failed (npm run lint or npm test in ui/). It is deterministic, and a"
      gate_err "pre-existing ui lint is not a thing this repo tolerates, so it is never measured"
      gate_err "against the base."
      return 1 ;;
    *)
      gate_err "the gates failed before any test ran (exit $rc), so there is nothing to attribute."
      return 1 ;;
  esac

  mine="$log.failures"
  gate_failed_tests "$log" > "$mine"
  gate_parse_complete "$log" "$mine" "here" || return 1
  if [ ! -s "$mine" ]; then
    gate_err "cargo test failed without naming a single failing test: a build error, a panic"
    gate_err "outside a test, or a harness that died. There is nothing to match against the"
    gate_err "base, so this counts as this change's. The output is in $log."
    return 1
  fi

  base=$(gate_base_commit "$wt") || {
    gate_err "cannot find the commit this branch forked from, so there is no baseline and no"
    gate_err "way to tell a failure this change caused from one that was already there."
    gate_err "Every failure counts as this change's."
    return 1; }

  theirs="$blog.failures"
  if [ "$(cat "$blog.commit" 2>/dev/null)" = "$base" ] && [ -f "$theirs" ]; then
    gate_out "baseline: reusing the suite already run at $base ($blog)"
  else
    gate_out "baseline: running the suite at $base, in a scratch worktree"
    gate_tests_at "$wt" "$base" "$blog"; brc=$?
    gate_failed_tests "$blog" > "$theirs"
    if [ "$brc" = "125" ]; then
      gate_err "could not run the suite at $base at all, so nothing can be attributed to it."
      gate_err "Every failure counts as this change's. Why it failed is in $blog."
      rm -f "$theirs" 2>/dev/null
      return 1
    fi
    if [ "$brc" != "0" ] && [ ! -s "$theirs" ]; then
      gate_err "the suite at $base failed without naming a failing test, so what it would have"
      gate_err "reported cannot be read and nothing can be subtracted from it. Every failure"
      gate_err "counts as this change's. Its output is in $blog."
      rm -f "$theirs" 2>/dev/null
      return 1
    fi
    gate_parse_complete "$blog" "$theirs" "at $base" || {
      rm -f "$theirs" 2>/dev/null
      return 1; }
    printf '%s\n' "$base" > "$blog.commit" 2>/dev/null
  fi

  new=$(LC_ALL=C comm -23 "$mine" "$theirs")
  if [ -n "$new" ]; then
    n=$(printf '%s\n' "$new" | grep -c .)
    gate_err "$n failure(s) here do not fail at $base, so they are this change's:"
    printf '%s\n' "$new" | while IFS= read -r t; do gate_err "  $t"; done
    return 1
  fi

  n=$(grep -c . "$mine")
  gate_out "$n failure(s) fail identically at $base, so they predate this change:"
  while IFS= read -r t; do gate_out "  $t"; done < "$mine"
  gate_out "Nothing failed here that passes there. The gates pass."
  return 0
}
