#!/usr/bin/env bash
# Tests for apply.sh's argument handling and change resolution (#256).
#
# Resolution is the part worth testing: picking the wrong change would point an
# implementer at somebody else's work, and the failure is silent. Everything here
# runs through --dry-run, so no agent is ever launched.
#
# Self-contained: builds a throwaway repo with real worktrees. Usage:
#   .workflow/apply-tests.sh
set -uo pipefail

here=$(cd "$(dirname "$0")" && pwd)
APPLY="$here/apply.sh"
pass=0; fail=0

expect() { # expect <want-exit> <label> -- <args...>
  local want="$1" label="$2"; shift 3
  local out rc
  out=$(cd "$cwd" && "$APPLY" "$@" 2>&1); rc=$?
  if [ "$rc" = "$want" ]; then
    pass=$((pass + 1)); printf 'ok    %s\n' "$label"
  else
    fail=$((fail + 1)); printf 'FAIL  %s (wanted exit %s, got %s)\n' "$label" "$want" "$rc"
    printf '%s\n' "$out" | sed 's/^/        /' | head -4
  fi
}

expect_change() { # expect_change <want-change> <label> -- <args...>
  local want="$1" label="$2"; shift 3
  local out rc got
  out=$(cd "$cwd" && "$APPLY" "$@" 2>&1); rc=$?
  got=$(printf '%s\n' "$out" | sed -n 's/^change: \([^ ]*\).*/\1/p' | tail -1)
  if [ "$rc" = "0" ] && [ "$got" = "$want" ]; then
    pass=$((pass + 1)); printf 'ok    %s\n' "$label"
  else
    fail=$((fail + 1)); printf 'FAIL  %s (wanted change %s, got %s, exit %s)\n' "$label" "$want" "${got:-none}" "$rc"
    printf '%s\n' "$out" | sed 's/^/        /' | head -4
  fi
}

setup() {
  repo=$(mktemp -d)
  cd "$repo" || exit 2
  git init -q .; git config user.email t@t; git config user.name t
  mkdir -p openspec/changes/archive
  echo x > openspec/changes/archive/.gitkeep
  # The real ignore file, so the artifact test below judges the rule that ships.
  cp "$here/../.gitignore" .gitignore
  git add -A; git commit -qm base
  cwd="$repo"
}
teardown() { cd "$here" || exit 2; [ -n "${repo:-}" ] && [ -d "$repo" ] && find "$repo" -mindepth 0 -delete 2>/dev/null; repo=""; }

# A worktree carrying one live change folder, exactly as /opsx:propose leaves it.
add_change() { # add_change <issue-slug>
  local name="$1" n
  n=$(printf '%s' "$name" | sed -n 's/^\(issue-[0-9]\{1,\}\).*/\1/p')
  git worktree add -q ".worktrees/$n" -b "$name" 2>/dev/null
  mkdir -p ".worktrees/$n/openspec/changes/$name"
  printf '# Proposal\n' > ".worktrees/$n/openspec/changes/$name/proposal.md"
}

# --- argument handling ------------------------------------------------------------
setup
expect 2 "no arguments"                        --
expect 2 "implementer only"                    -- agy
expect 2 "implementer equals reviewer"         -- agy agy --dry-run
expect 2 "unknown option"                      -- agy codex --wat
expect 2 "--rounds without a number"           -- agy codex --rounds x --dry-run
expect 2 "--rounds zero"                       -- agy codex --rounds 0 --dry-run
expect 2 "a fourth positional"                 -- agy codex issue-1-a issue-2-b
expect 2 "a change name that is not issue-N"   -- agy codex not-an-issue --dry-run

# The old order fails loudly rather than resolving to something plausible.
expect 2 "the old <change> <impl> <rev> order" -- issue-1-a agy codex --dry-run
teardown

# --- resolution -------------------------------------------------------------------
setup
expect 2 "nothing in flight, nothing named"    -- agy codex --dry-run
add_change issue-1-alpha
expect_change issue-1-alpha "one change in flight resolves from the main checkout" -- agy codex --dry-run
expect_change issue-1-alpha "an explicit change still wins"                        -- agy codex issue-1-alpha --dry-run

add_change issue-2-beta
expect 2 "two in flight refuses rather than guessing" -- agy codex --dry-run
expect_change issue-2-beta "naming one of the two resolves it" -- agy codex issue-2-beta --dry-run

# Called from inside a worktree, only that worktree counts: the session that
# proposed is standing in the answer, even with another change in flight elsewhere.
cwd="$repo/.worktrees/issue-2"
expect_change issue-2-beta "from inside a worktree, its own change wins" -- agy codex --dry-run
cwd="$repo/.worktrees/issue-1"
expect_change issue-1-alpha "and the other worktree resolves to the other change" -- agy codex --dry-run
cwd="$repo"

# The archive folder is not a change in flight.
expect 2 "archive/ is not counted as a live change" -- agy codex --dry-run
teardown

# --- the dry run reports what it would do -----------------------------------------
setup
add_change issue-7-gamma
out=$(cd "$repo" && "$APPLY" agy codex --rounds 5 --dry-run 2>&1)
for want in "implementer: agy" "reviewer: codex" "change: issue-7-gamma" "rounds: 5"; do
  if printf '%s\n' "$out" | grep -qF "$want"; then
    pass=$((pass + 1)); printf 'ok    dry run reports "%s"\n' "$want"
  else
    fail=$((fail + 1)); printf 'FAIL  dry run omits "%s"\n' "$want"
  fi
done
# Matched on the tail: on macOS mktemp says /var/... where git resolves /private/var/...
if printf '%s\n' "$out" | grep -q '^worktree: .*/\.worktrees/issue-7$'; then
  pass=$((pass + 1)); printf 'ok    dry run names the worktree it would use\n'
else
  fail=$((fail + 1)); printf 'FAIL  dry run does not name the worktree\n'
fi
teardown

# --- run-stage.sh refuses a review it could not extract -----------------------------
# The one case that can be driven without a real agent: a stand-in on PATH that
# prints something and never emits the JSON envelope, which is exactly the
# NO_STRUCTURED_RESULT shape that used to hand a transcript back as a review (#264).
setup
add_change issue-9-delta
bin=$(mktemp -d)
cat > "$bin/codex" <<'FAKE'
#!/usr/bin/env bash
echo "OpenAI Codex v0.0.0"
echo "some console noise, and no structured result anywhere"
echo "VERDICT: APPROVE"
FAKE
chmod +x "$bin/codex"
out=$(cd "$repo" && PATH="$bin:$PATH" "$here/run-stage.sh" review codex issue-9-delta 2>&1); rc=$?
if [ "$rc" = "7" ]; then
  pass=$((pass + 1)); printf 'ok    a review with no structured result exits 7\n'
else
  fail=$((fail + 1)); printf 'FAIL  a review with no structured result exits 7 (got %s)\n' "$rc"
  printf '%s\n' "$out" | sed 's/^/        /' | head -4
fi
if printf '%s\n' "$out" | grep -q 'Refusing to treat a transcript as a review'; then
  pass=$((pass + 1)); printf 'ok    and says why\n'
else
  fail=$((fail + 1)); printf 'FAIL  and says why\n'
fi

# The same run answers where its artifacts went (#255). The landing commit stages the
# worktree wholesale, so a merely untracked transcript gets committed; both halves are
# asserted, because "nothing untracked" passes just as well when nothing was written.
if [ -s "$repo/.worktrees/issue-9/.agent-runs/review-codex.log" ]; then
  pass=$((pass + 1)); printf 'ok    the run writes its log under .agent-runs/\n'
else
  fail=$((fail + 1)); printf 'FAIL  no log at .worktrees/issue-9/.agent-runs/review-codex.log\n'
  (cd "$repo/.worktrees/issue-9" && ls -A) | sed 's/^/        /'
fi
stray=$(cd "$repo/.worktrees/issue-9" && git ls-files --others --exclude-standard | grep -c 'agent\|agy')
if [ "$stray" = "0" ]; then
  pass=$((pass + 1)); printf 'ok    and a git add -A stages none of it\n'
else
  fail=$((fail + 1)); printf 'FAIL  %s run artifact(s) would be staged\n' "$stray"
  (cd "$repo/.worktrees/issue-9" && git ls-files --others --exclude-standard) | sed 's/^/        /'
fi
find "$bin" -mindepth 0 -delete 2>/dev/null
teardown

printf '\n%s passed, %s failed\n' "$pass" "$fail"
[ "$fail" = "0" ]
