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

# fatal, canary, fixture_built and suite_guard_case: the fixture guard every suite here
# shares. Read why in the file itself; the short version is that a fixture write that
# fails silently leaves a case asserting against a file that was never written (#333).
. "$here/suite-lib.sh"
suite_parse_args "$@"

expect() { # expect <want-exit> <label> -- <args...>
  local want="$1" label="$2"; shift 3
  local out rc
  suite_selected "$label" || return 0
  canary
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
  canary
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
  repo=$(mktemp -d) || fatal "cannot create a fixture directory (TMPDIR=${TMPDIR:-/tmp})."
  cd "$repo" || fatal "cannot enter the fixture directory $repo."
  git init -q .; git config user.email t@t; git config user.name t
  mkdir -p openspec/changes/archive || fatal "cannot create the fixture's change directory."
  echo x > openspec/changes/archive/.gitkeep
  # The real ignore file, so the artifact test below judges the rule that ships.
  cp "$here/../.gitignore" .gitignore
  git add -A; git commit -qm base
  cwd="$repo"
  # Hermetic against the developer's own lineup (#330): every case below decides for
  # itself whether a roles file exists, and this one does not until a case writes it.
  export OPENSPEC_LOOP_ROLES_FILE="$repo/roles.local"
  rm -f "$OPENSPEC_LOOP_ROLES_FILE"
  fixture_built "$repo" openspec/changes/archive/.gitkeep .gitignore
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

# --- the machine-local lineup (#330) ----------------------------------------------
#
# Asserted on the MESSAGE wherever the refusal shares exit 2 with the path it would fall
# through to, because an exit-code assertion passes against the broken code it exists to
# catch: a lone agent read as a change name exits 2 for failing to resolve, exactly as a
# lone agent refused as half a pair does.
roles() { printf '%s\n' "$@" > "$OPENSPEC_LOOP_ROLES_FILE"; }
says() { # says <want-substring> <label> -- <args...>
  local want="$1" label="$2"; shift 3
  local out
  out=$(cd "$cwd" && "$APPLY" "$@" 2>&1)
  case "$out" in
    *"$want"*) pass=$((pass + 1)); printf 'ok    %s\n' "$label" ;;
    *) fail=$((fail + 1)); printf 'FAIL  %s (no %s in the output)\n' "$label" "$want"
       printf '%s\n' "$out" | sed 's/^/        /' | head -4 ;;
  esac
}

setup
add_change issue-1-alpha
rm -f "$OPENSPEC_LOOP_ROLES_FILE"
expect 2 "no agents and no roles file"          --
says "$OPENSPEC_LOOP_ROLES_FILE" "the refusal names the file it wanted" --

roles 'planner: claude' 'plan-reviewer: codex' 'implementer: agy' 'code-reviewer: opencode'
expect_change issue-1-alpha "no agents: the pair comes from the file" -- --dry-run
says "implementer: agy" "and it is the file's implementer"    -- --dry-run
says "reviewer: opencode" "and the file's code-reviewer"      -- --dry-run
# A lone positional is the CHANGE once the pair may be absent, which is the reading the
# old left-to-right assignment could not express.
expect_change issue-1-alpha "a lone positional is the change, not the implementer" -- issue-1-alpha --dry-run
says "implementer: agy" "with the pair still from the file"   -- issue-1-alpha --dry-run
# and a lone AGENT is half a pair, refused rather than resolved as a change name
expect 2 "a lone agent is half a pair"          -- agy
says "takes both or neither" "and says so, rather than failing to resolve 'agy'" -- agy

says "implementer: claude" "an explicit pair beats the file" -- claude codex --dry-run
says "reviewer: codex" "on both roles"                      -- claude codex --dry-run

# The file must not be a way to reach a pairing the command line refuses.
roles 'planner: claude' 'plan-reviewer: codex' 'implementer: agy' 'code-reviewer: agy'
expect 2 "a self-reviewing pair from the file"  -- --dry-run
says "Fix 'code-reviewer' in" "pointing at the file, not at the usage" -- --dry-run
rm -f "$OPENSPEC_LOOP_ROLES_FILE"
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

# Everything below drives run-stage.sh, which runs its agent under pty_run. A shell
# whose stdin is not a terminal cannot allocate one, and script(1) then fails before
# the stand-in agent runs at all, so every case fails for the same reason and none of
# them is telling you anything. Say so once, rather than reporting a dozen identical
# failures as findings. CI has a terminal, so this is not a way to pass quietly:
# there the block runs.
. "$here/agents.sh"
pty_available=1
if ! pty_run true >/dev/null 2>&1; then
  pty_available=0
  printf 'SKIP  the run-stage cases: this shell cannot allocate a pty (script: tcgetattr)\n'
fi

if [ "$pty_available" = "1" ]; then
# --- run-stage.sh reads each agent's own result envelope (#274) ---------------------
# Every CLI wraps its answer differently, and reading one shape for all of them left
# claude and codex yielding no answer on every run, unresumable, and
# unable to review at all. Stand-ins print the shapes those CLIs really emit, so the
# assertion is that the log holds the ANSWER rather than the console capture, and
# that the id a later --resume would need was recorded.
setup
add_change issue-8-envelope
bin=$(mktemp -d)
cat > "$bin/claude" <<'FAKE'
#!/usr/bin/env bash
echo '{"type":"result","subtype":"success","is_error":false,"result":"REVIEW BODY","session_id":"11111111-2222-3333-4444-555555555555"}'
FAKE
cat > "$bin/codex" <<'FAKE'
#!/usr/bin/env bash
echo '{"type":"thread.started","thread_id":"01a04dc2-867a-7293-9777-5a2d07e4dbac"}'
echo '{"type":"turn.started"}'
echo '{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"reading the diff now"}}'
# A previous run's transcript, read back by this one: escaped inside one event, so
# line-anchored parsing never mistakes its thread id for this run's (#264).
echo '{"type":"item.completed","item":{"id":"item_1","type":"command_execution","aggregated_output":"{\"type\":\"thread.started\",\"thread_id\":\"99999999-9999-9999-9999-999999999999\"}"}}'
echo '{"type":"item.completed","item":{"id":"item_2","type":"agent_message","text":"REVIEW BODY"}}'
echo '{"type":"turn.completed"}'
FAKE
cat > "$bin/agy" <<'FAKE'
#!/usr/bin/env bash
echo '{"conversation_id":"conv-abc","status":"COMPLETED","response":"REVIEW BODY"}'
FAKE
cat > "$bin/opencode" <<'FAKE'
#!/usr/bin/env bash
echo '{"type":"text","sessionID":"ses_fake001","part":{"type":"text","text":"REVIEW BODY"}}'
echo '{"type":"step_finish","sessionID":"ses_fake001","part":{"type":"step-finish","reason":"stop"}}'
FAKE
chmod +x "$bin/claude" "$bin/codex" "$bin/agy" "$bin/opencode"

for agent in claude codex agy opencode; do
  case "$agent" in
    claude) want_id="11111111-2222-3333-4444-555555555555" ;;
    codex) want_id="01a04dc2-867a-7293-9777-5a2d07e4dbac" ;;
    agy) want_id="conv-abc" ;;
    opencode) want_id="ses_fake001" ;;
  esac
  out=$(cd "$repo" && PATH="$bin:$PATH" "$here/run-stage.sh" review "$agent" issue-8-envelope 2>&1); rc=$?
  got_log=$(cat "$repo/.worktrees/issue-8/.agent-runs/review-$agent.log" 2>/dev/null)
  got_id=$(cat "$repo/.worktrees/issue-8/.agent-runs/review-$agent.conversation" 2>/dev/null)
  if [ "$rc" = "0" ] && [ "$got_log" = "REVIEW BODY" ]; then
    pass=$((pass + 1)); printf "ok    %s's answer is extracted as the review\n" "$agent"
  else
    fail=$((fail + 1)); printf "FAIL  %s's answer is extracted as the review (exit %s, log %s)\n" "$agent" "$rc" "${got_log:-empty}"
    printf '%s\n' "$out" | sed 's/^/        /' | head -4
  fi
  if [ "$got_id" = "$want_id" ]; then
    pass=$((pass + 1)); printf "ok    and %s's resumable id is recorded as '%s'\n" "$agent" "$want_id"
  else
    fail=$((fail + 1)); printf "FAIL  %s's resumable id is '%s', wanted '%s'\n" "$agent" "$got_id" "$want_id"
  fi
done
find "$bin" -mindepth 0 -delete 2>/dev/null
teardown

# --- run-stage.sh refuses a review it could not extract -----------------------------
# A stand-in that prints console noise and no envelope at all: the NO_ANSWER_IN_OUTPUT
# shape that used to hand a transcript back as a review (#264). Checked for both agents
# whose answer is an envelope, because the guard used to key on agy's shape alone, so
# these two could not pass a review even when they had written one (#274).
setup
add_change issue-9-delta
bin=$(mktemp -d)
for agent in codex claude; do
  cat > "$bin/$agent" <<'FAKE'
#!/usr/bin/env bash
echo "some console noise, and no structured result anywhere"
echo "VERDICT: APPROVE"
FAKE
  chmod +x "$bin/$agent"
  out=$(cd "$repo" && PATH="$bin:$PATH" "$here/run-stage.sh" review "$agent" issue-9-delta 2>&1); rc=$?
  if [ "$rc" = "7" ]; then
    pass=$((pass + 1)); printf 'ok    a %s review with no structured result exits 7\n' "$agent"
  else
    fail=$((fail + 1)); printf 'FAIL  a %s review with no structured result exits 7 (got %s)\n' "$agent" "$rc"
    printf '%s\n' "$out" | sed 's/^/        /' | head -4
  fi
  if printf '%s\n' "$out" | grep -q 'Refusing to treat a transcript as a review'; then
    pass=$((pass + 1)); printf 'ok    and says why\n'
  else
    fail=$((fail + 1)); printf 'FAIL  and says why\n'
  fi
done

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

# --- the repeated-round refusal is keyed on the tree AND the delta (#362) -----------
# A review judges code against the delta specs, and tree_excl keeps openspec/changes out
# of TREE_SHA256, so a finding answered in the delta leaves the tree byte-identical. Keyed
# on the tree alone the refusal fired on a review that had never happened, with nothing
# that could ever move the digest, and #338 could not land. All three directions are
# asserted here: a refusal that stops firing looks exactly like one that passes.
setup
add_change issue-10-guard
gdir="$repo/.worktrees/issue-10/openspec/changes/issue-10-guard"
mkdir -p "$gdir/specs/thing" || fatal "cannot create the fixture's delta specs directory."
printf '# thing\n\n## ADDED Requirements\n\n### Requirement: One\n\nIt SHALL hold.\n' \
  > "$gdir/specs/thing/spec.md" || fatal "cannot write the fixture's delta spec."
# A passing plan verdict, because run-stage.sh refuses to start implement without one.
# Its digest is rewritten wherever this case moves the delta below, which is what the
# driver does for real: a plan re-review is how a moved contract gets a verdict again,
# and that is the state the guard has to cope with.
printf 'AUTHOR: agy\nREVIEWER: claude\nVERDICT: APPROVE\n' > "$gdir/review.md" \
  || fatal "cannot write the fixture's plan verdict."
"$here/specs-digest.sh" "$gdir" --write > /dev/null \
  || fatal "cannot record the fixture's SPECS_SHA256."
fixture_built "$repo" "$gdir/specs/thing/spec.md" "$gdir/review.md"
grep -q '^SPECS_SHA256:' "$gdir/review.md" \
  || fatal "specs-digest.sh recorded no digest in the fixture's review.md."
bin=$(mktemp -d)
# Writes the code once and never again, which is the #338 shape: the first stage has work
# to do, and every later one is a resumed round that correctly finds the code already
# right. run-stage.sh permits a no-op only on a resumed round, so both halves are needed
# to reach the guard at all.
cat > "$bin/agy" <<'FAKE'
#!/usr/bin/env bash
[ -e src/thing.txt ] || { mkdir -p src && echo done > src/thing.txt; }
echo '{"conversation_id":"conv-guard","status":"COMPLETED","response":"nothing left to do"}'
FAKE
cat > "$bin/claude" <<'FAKE'
#!/usr/bin/env bash
echo '{"type":"result","subtype":"success","is_error":false,"result":"one finding, in the delta text\nVERDICT: REVISE","session_id":"11111111-2222-3333-4444-555555555555"}'
FAKE
chmod +x "$bin/agy" "$bin/claude"
guard_run() { (cd "$repo" && PATH="$bin:$PATH" "$APPLY" agy claude issue-10-guard --rounds 1 2>&1); }

# Round 1: no prior artifact, so the review runs and records what it judged.
out=$(guard_run); rc=$?
if [ "$rc" = "6" ] && [ -e "$gdir/diff-review-1.md" ]; then
  pass=$((pass + 1)); printf 'ok    the first round runs and writes diff-review-1.md\n'
else
  fail=$((fail + 1)); printf 'FAIL  the first round runs and writes diff-review-1.md (exit %s)\n' "$rc"
  printf '%s\n' "$out" | sed 's/^/        /' | tail -6
fi
if grep -qE '^SPECS_SHA256: [0-9a-f]{64}$' "$gdir/diff-review-1.md" 2>/dev/null; then
  pass=$((pass + 1)); printf 'ok    and records the contract it judged, not the tree alone\n'
else
  fail=$((fail + 1)); printf 'FAIL  diff-review-1.md carries no SPECS_SHA256\n'
  head -3 "$gdir/diff-review-1.md" 2>/dev/null | sed 's/^/        /'
fi

# Neither has moved: still refused, which is what #299 bought and this must not undo.
out=$(guard_run); rc=$?
if [ "$rc" = "10" ]; then
  pass=$((pass + 1)); printf 'ok    an unmoved tree and unmoved delta still exit 10\n'
else
  fail=$((fail + 1)); printf 'FAIL  an unmoved tree and unmoved delta still exit 10 (got %s)\n' "$rc"
  printf '%s\n' "$out" | sed 's/^/        /' | tail -6
fi

# The delta moves and the code does not: the #338 shape. A review is owed, and the
# assertion is that one actually ran, never merely that the exit differs.
printf 'It SHALL also supersede the third frozen site.\n' >> "$gdir/specs/thing/spec.md" \
  || fatal "cannot amend the fixture's delta spec."
"$here/specs-digest.sh" "$gdir" --write > /dev/null \
  || fatal "cannot re-record the fixture's SPECS_SHA256."
out=$(guard_run); rc=$?
if [ "$rc" != "10" ] && [ -e "$gdir/diff-review-2.md" ]; then
  pass=$((pass + 1)); printf 'ok    a delta-only fix gets its review instead of exit 10\n'
else
  fail=$((fail + 1)); printf 'FAIL  a delta-only fix gets its review instead of exit 10 (exit %s, round 2 %s)\n' \
    "$rc" "$([ -e "$gdir/diff-review-2.md" ] && echo written || echo missing)"
  printf '%s\n' "$out" | sed 's/^/        /' | tail -6
fi

# A round artifact from before this change records no contract, so nothing can show it
# judged this one. It must not deadlock the change it belongs to.
rm -f "$gdir/diff-review-2.md"
grep -v '^SPECS_SHA256:' "$gdir/diff-review-1.md" > "$gdir/diff-review-1.md.tmp" \
  && mv "$gdir/diff-review-1.md.tmp" "$gdir/diff-review-1.md" \
  || fatal "cannot rewrite the fixture's round artifact."
out=$(guard_run); rc=$?
if [ "$rc" != "10" ] && [ -e "$gdir/diff-review-2.md" ]; then
  pass=$((pass + 1)); printf 'ok    a round artifact with no recorded contract does not deadlock\n'
else
  fail=$((fail + 1)); printf 'FAIL  a round artifact with no recorded contract does not deadlock (exit %s, round 2 %s)\n' \
    "$rc" "$([ -e "$gdir/diff-review-2.md" ] && echo written || echo missing)"
  printf '%s\n' "$out" | sed 's/^/        /' | tail -6
fi
find "$bin" -mindepth 0 -delete 2>/dev/null
teardown
fi

# --- the pty capture is filtered down to what the agent actually said ---------------
# Asserted on bytes rather than through a run, so it holds where no pty can be had.
# Every case here is something a real capture contained and no answer ever should.
capture_case() { # capture_case <label> <input> <want>
  local label="$1" got
  got=$(printf '%s' "$2" | clean_capture)
  if [ "$got" = "$3" ]; then
    pass=$((pass + 1)); printf 'ok    capture: %s\n' "$label"
  else
    fail=$((fail + 1)); printf 'FAIL  capture: %s -> %s\n' "$label" "$(printf '%s' "$got" | od -c | head -2 | tr '\n' ' ')"
  fi
}

capture_case "BSD script's EOF echo is dropped" "$(printf '^D\b\bREVIEW BODY')" "REVIEW BODY"
capture_case "a bare ^D an agent typed survives" "he pressed ^D there" "he pressed ^D there"
capture_case "ANSI colour is dropped" "$(printf '\033[31mred\033[0m')" "red"
capture_case "carriage returns are dropped" "$(printf 'a\rb')" "ab"
capture_case "plain text is untouched" '{"type":"thread.started"}' '{"type":"thread.started"}'

# --- each agent is sent its own spelling of the apply step (#274) -------------------
# One spelling for all four is a command three of them do not have: claude dies on
# "Unknown command" given the workflow form, which is two apply runs that did nothing.
for pair in "claude:/opsx:apply issue-3-c" "agy:/opsx-apply issue-3-c" \
            "opencode:/opsx-apply issue-3-c" "codex:openspec/changes/issue-3-c"; do
  agent="${pair%%:*}"; want="${pair#*:}"
  got=$(agent_apply_prompt "$agent" issue-3-c)
  if printf '%s' "$got" | grep -qF "$want"; then
    pass=$((pass + 1)); printf "ok    %s is told to apply with '%s'\n" "$agent" "$want"
  else
    fail=$((fail + 1)); printf "FAIL  %s was told '%s', wanted '%s'\n" "$agent" "$got" "$want"
  fi
done
if ! agent_apply_prompt nosuchagent issue-3-c >/dev/null 2>&1; then
  pass=$((pass + 1)); printf 'ok    an unknown agent gets no apply prompt at all\n'
else
  fail=$((fail + 1)); printf 'FAIL  an unknown agent got an apply prompt\n'
fi

# The guard on this suite's own fixtures.
suite_guard_case "$here/apply-tests.sh"

if [ -n "$SUITE_FILTER" ]; then
  printf '\n%s passed, %s failed, %s skipped by --filter %s\n' "$pass" "$fail" "$skipped" "$SUITE_FILTER"
  [ "$((pass + fail))" -gt 0 ] || { printf 'no case matched --filter %s\n' "$SUITE_FILTER" >&2; exit 2; }
else
  printf '\n%s passed, %s failed\n' "$pass" "$fail"
fi
[ "$fail" = "0" ]
