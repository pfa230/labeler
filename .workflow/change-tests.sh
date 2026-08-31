#!/usr/bin/env bash
# Tests for run-change.sh and the roles it drives (#283).
#
# The end-to-end loop cannot be tested end to end without launching four agents, so what
# is asserted here is everything that decides WHETHER a stage runs and WHO runs it:
# argument handling, the self-review and non-resumable refusals, the guards in
# run-stage.sh that a role change could silently unkey (the apply lock, the
# reviewer-must-not-edit digest, the implement no-op check, the live-or-archived folder
# check), and the blocking-question protocol. Those are the parts whose failure is
# silent. Everything that can run through --dry-run does, so no agent is launched there.
#
# Not covered, and worth knowing: the plan-review gate preflight and the
# no-structured-result refusal are asserted by apply-tests.sh; the driver's own top -
# the scope file, the parking of an unattributed review.md, and the gates-reached
# assertion - is not, because reaching it means stubbing gh and then every agent in turn.
#
# Self-contained: builds a throwaway repo with real worktrees. Usage:
#   .workflow/change-tests.sh
set -uo pipefail

here=$(cd "$(dirname "$0")" && pwd)
RUN="$here/run-change.sh"
STAGE="$here/run-stage.sh"
APPLY="$here/apply.sh"
pass=0; fail=0

ok()   { pass=$((pass + 1)); printf 'ok    %s\n' "$1"; }
bad()  { fail=$((fail + 1)); printf 'FAIL  %s\n' "$1"; }

expect() { # expect <want-exit> <label> -- <args...>
  local want="$1" label="$2"; shift 3
  local out rc
  out=$(cd "$cwd" && "$RUN" "$@" 2>&1); rc=$?
  if [ "$rc" = "$want" ]; then ok "$label"; else
    bad "$label (wanted exit $want, got $rc)"
    printf '%s\n' "$out" | sed 's/^/        /' | head -4
  fi
}

setup() {
  repo=$(mktemp -d)
  cd "$repo" || exit 2
  git init -q .; git config user.email t@t; git config user.name t
  mkdir -p openspec/changes/archive
  echo x > openspec/changes/archive/.gitkeep
  cp "$here/../.gitignore" .gitignore
  git add -A; git commit -qm base
  cwd="$repo"
}
teardown() { cd "$here" || exit 2; [ -n "${repo:-}" ] && [ -d "$repo" ] && find "$repo" -mindepth 0 -delete 2>/dev/null; repo=""; }

add_change() { # add_change <issue-slug>
  local name="$1" n
  n=$(printf '%s' "$name" | sed -n 's/^\(issue-[0-9]\{1,\}\).*/\1/p')
  git worktree add -q ".worktrees/$n" -b "$name" 2>/dev/null
  mkdir -p ".worktrees/$n/openspec/changes/$name"
  printf '# Proposal\n' > ".worktrees/$n/openspec/changes/$name/proposal.md"
}

# A plan review the gate accepts. Every implement stage probes the gate before it
# starts, so a change without one cannot reach the code roles at all.
# Work on the tree, which is what a handover is about. Without it there is nothing to
# inherit and handover_plan rightly says so.
dirty_tree() { # dirty_tree <issue-slug>
  local n
  n=$(printf '%s' "$1" | sed -n 's/^\(issue-[0-9]\{1,\}\).*/\1/p')
  printf 'inherited work\n' >> "$repo/.worktrees/$n/inherited.txt"
}

add_passing_review() { # add_passing_review <issue-slug>
  local name="$1" n d
  n=$(printf '%s' "$name" | sed -n 's/^\(issue-[0-9]\{1,\}\).*/\1/p')
  d="$repo/.worktrees/$n/openspec/changes/$name"
  printf '# Plan review\n\nAUTHOR: claude\nREVIEWER: codex\nVERDICT: APPROVE\n' > "$d/review.md"
  "$here/specs-digest.sh" "$d" --write >/dev/null 2>&1
}

# --- argument handling ------------------------------------------------------------
setup
expect 2 "no arguments"                          --
expect 2 "four names is one short"               -- 1 claude codex agy
expect 2 "a sixth positional"                    -- 1 claude codex agy codex extra
expect 2 "an issue number that is not a number"  -- abc claude codex agy codex --dry-run
expect 2 "an unknown agent"                      -- 1 claude codex nosuch codex --dry-run
expect 2 "unknown option"                        -- 1 claude codex agy codex --wat
expect 2 "--rounds without a number"             -- 1 claude codex agy codex --rounds x --dry-run
expect 2 "--rounds zero"                         -- 1 claude codex agy codex --rounds 0 --dry-run

# The two pairings the commit gate refuses, refused here instead of four agent runs later.
expect 2 "a planner reviewing its own plan"      -- 1 claude claude agy codex --dry-run
expect 2 "an implementer reviewing its own code" -- 1 claude codex agy agy --dry-run
# One agent may hold a role in both pairs: the gate judges each pair on its own.
expect 0 "one agent may plan and implement"      -- 1 claude codex claude agy --dry-run

# opencode authored nothing until 1.18.20 gave it --format json and -s <sessionID>;
# it now takes every role (#286). The resumability guard itself is exercised below
# against a synthetic entry, because no registered agent fails it any more.
expect 0 "opencode may plan"                     -- 1 opencode codex agy codex --dry-run
expect 0 "opencode may implement"                -- 1 claude codex opencode codex --dry-run
expect 0 "and opencode may review"               -- 1 claude opencode agy opencode --dry-run
out=$(cd "$repo" && "$APPLY" opencode codex issue-1-a --dry-run 2>&1); rc=$?
if [ "$rc" = "0" ]; then ok "apply.sh accepts opencode as the implementer"
else bad "apply.sh refused a resumable implementer (exit $rc)"; fi

# The guard is what stops a future non-resumable entry from authoring, and a guard that
# no longer fires looks exactly like one that passes. Asserted directly against the
# function rather than through a run, since every registered agent satisfies it.
( . "$here/agents.sh"; agent_resumable notanagent ) \
  && ok "agent_resumable answers for an unregistered name" \
  || bad "agent_resumable rejected an unregistered name"
teardown

# --- the dry run reports what it would do -----------------------------------------
setup
out=$(cd "$repo" && "$RUN" 7 claude codex agy codex --rounds 5 --dry-run 2>&1)
for want in "issue: 7" "planner: claude" "plan-reviewer: codex" "implementer: agy" \
            "code-reviewer: codex" "rounds: 5"; do
  if printf '%s\n' "$out" | grep -qF "$want"; then ok "dry run reports \"$want\""
  else bad "dry run omits \"$want\""; fi
done
# Matched on the tail: on macOS mktemp says /var/... where git resolves /private/var/...
if printf '%s\n' "$out" | grep -q '^worktree: .*/\.worktrees/issue-7$'; then
  ok "dry run names the worktree it would use"
else bad "dry run does not name the worktree"; fi
teardown

# --- which stage runs next ---------------------------------------------------------
# The resumption logic, asserted without launching anything. This is the decision that
# skipped propose entirely for a brand-new issue: the review gate has no live change to
# refuse, so it passes, and a guard that asked only the gate concluded the plan was done.
expect_stage() { # expect_stage <want> <label>
  local want="$1" label="$2" got
  got=$(cd "$repo" && "$RUN" "$issue_n" claude codex agy codex --dry-run 2>&1 | sed -n 's/^next stage: //p')
  if [ "$got" = "$want" ]; then ok "$label"; else bad "$label (wanted $want, got ${got:-nothing})"; fi
}

setup
issue_n=11
expect_stage worktree "with no worktree at all, the worktree comes first"
git worktree add -q .worktrees/issue-11 -b issue-11-kappa 2>/dev/null
expect_stage propose "a fresh worktree with no change proposes"
d="$repo/.worktrees/issue-11/openspec/changes/issue-11-kappa"
mkdir -p "$d/specs/thing"
printf '# Proposal\n' > "$d/proposal.md"
expect_stage propose "a proposal with no delta spec is still incomplete"
printf '## ADDED Requirements\n' > "$d/specs/thing/spec.md"
expect_stage plan-review "proposal plus a delta spec goes to review"
printf '# Plan review\n\nAUTHOR: claude\nREVIEWER: codex\nVERDICT: REVISE\n' > "$d/review.md"
expect_stage plan-review "a REVISE verdict stays in review"
printf '# Plan review\n\nAUTHOR: claude\nREVIEWER: codex\nVERDICT: APPROVE\n' > "$d/review.md"
expect_stage plan-review "and an approved review with no digest does not pass the gate"
"$here/specs-digest.sh" "$d" --write >/dev/null 2>&1
expect_stage tasks "an approved, digested plan writes its task list"
printf -- '- [ ] 1.1 do it\n' > "$d/tasks.md"
expect_stage apply "and then implements"
printf '# Diff review\n\nAUTHOR: agy\nREVIEWER: codex\nVERDICT: REVISE\n' > "$d/diff-review.md"
expect_stage apply "a REVISE diff review stays in apply"
printf '# Diff review\n\nAUTHOR: agy\nREVIEWER: codex\nVERDICT: APPROVE\n' > "$d/diff-review.md"
expect_stage apply "an approved diff review with no recorded contract is not trusted"
printf 'SPECS_SHA256: %s\n' "$("$here/specs-digest.sh" "$d")" >> "$d/diff-review.md"
expect_stage archive "an approved diff review naming the contract it read archives"
printf '\n## ADDED more\n' >> "$d/specs/thing/spec.md"
expect_stage plan-review "moving the specs voids the plan verdict first"
"$here/specs-digest.sh" "$d" --write >/dev/null 2>&1
expect_stage apply "and with the plan re-approved, the stale code approval sends it back to apply"
mkdir -p "$repo/.worktrees/issue-11/openspec/changes/archive/2026-01-01-issue-11-kappa"
find "$d" -mindepth 0 -delete 2>/dev/null
expect_stage gates "and an archived change goes to the gates"
teardown

# --- run-stage.sh: the roles ------------------------------------------------------
setup
add_change issue-1-alpha
out=$(cd "$repo" && "$STAGE" nosuchrole claude issue-1-alpha 2>&1); rc=$?
if [ "$rc" = "2" ] && printf '%s' "$out" | grep -q 'unknown role'; then
  ok "an unknown role is refused before anything is launched"
else bad "an unknown role is refused (exit $rc)"; fi
teardown

# --- the question protocol's own helpers -------------------------------------------
# Asserted on files rather than through a run, so they hold where no pty can be had.
source "$here/questions.sh"
setup
add_change issue-2-beta
wt="$repo/.worktrees/issue-2"
if ! questions_pending "$wt"; then ok "no QUESTIONS.md means nothing is pending"
else bad "an absent QUESTIONS.md read as pending"; fi
printf 'Q1: which is it?\n' > "$wt/QUESTIONS.md"
if questions_pending "$wt"; then ok "a written QUESTIONS.md is pending"
else bad "a written QUESTIONS.md is not pending"; fi
questions_park "$wt"
if ! questions_pending "$wt" && [ "$(find "$wt/.agent-runs" -name 'questions-*.md' | wc -l | tr -d ' ')" = "1" ]; then
  ok "parking moves it aside rather than losing it"
else bad "parking did not preserve the question"; fi
# An unanswered question is the only record of why a run stopped, so it is kept.
if grep -q 'which is it' "$wt"/.agent-runs/questions-*.md; then ok "and the parked copy is the question itself"
else bad "the parked copy is not the question"; fi
# Both files are ignored, so a git add -A at the landing commit cannot sweep them in.
printf 'A1: the first one\n' > "$wt/ANSWERS.md"
printf 'Q1: again?\n' > "$wt/QUESTIONS.md"
stray=$(cd "$wt" && git ls-files --others --exclude-standard | grep -c 'QUESTIONS\|ANSWERS')
if [ "$stray" = "0" ]; then ok "neither file would be staged by a git add -A"
else bad "$stray question file(s) would be staged"; fi
teardown

# --- each agent is sent its own spelling of each workflow step (#274) ---------------
source "$here/agents.sh"
for pair in "claude:propose:/opsx:propose issue-3-c" "claude:archive:/opsx:archive issue-3-c" \
            "agy:propose:/opsx-propose issue-3-c"   "agy:archive:/opsx-archive issue-3-c" \
            "opencode:propose:/opsx-propose issue-3-c" \
            "codex:propose:Create the OpenSpec change" "codex:archive:Archive the completed change"; do
  agent="${pair%%:*}"; rest="${pair#*:}"; step="${rest%%:*}"; want="${rest#*:}"
  got=$(agent_step_prompt "$agent" "$step" issue-3-c)
  if printf '%s' "$got" | grep -qF "$want"; then ok "$agent is told to $step with \"$want\""
  else bad "$agent was told \"$got\", wanted \"$want\""; fi
done
if ! agent_step_prompt claude nosuchstep issue-3-c >/dev/null 2>&1; then
  ok "an unknown step gets no prompt at all"
else bad "an unknown step got a prompt"; fi
# The old name still resolves: generalising it changed no caller.
if [ "$(agent_apply_prompt claude issue-3-c)" = "$(agent_step_prompt claude apply issue-3-c)" ]; then
  ok "agent_apply_prompt is still the apply step"
else bad "agent_apply_prompt drifted from agent_step_prompt"; fi

# Where the CLI can enforce read-only, both review roles use it: a reviewer that cannot
# write is a reviewer that cannot be talked into fixing what it found.
for role in review plan-review; do
  if agent_command codex "$role" "p" | grep -q -- '-s read-only'; then
    ok "codex runs $role read-only"
  else bad "codex runs $role writable"; fi
done
if agent_command codex implement "p" | grep -q -- '-s workspace-write'; then
  ok "and codex implements with write access"
else bad "codex cannot write while implementing"; fi

# agy's equivalent is plan mode (#290). It is an approval gate rather than a sandbox: told
# plainly to write a file it answers that it needs a Proceed first, and writes nothing.
# Weaker than codex's enforced read-only, stronger than asking in the prompt, and the
# digest guard above is what decides either way.
for role in review plan-review; do
  if agent_command agy "$role" "p" | grep -q -- '--mode plan'; then
    ok "agy runs $role in plan mode"
  else bad "agy runs $role able to edit"; fi
done
for role in propose tasks implement gate-fix archive commit-msg; do
  if agent_command agy "$role" "p" | grep -q -- '--mode accept-edits'; then
    ok "and agy $role with edits accepted"
  else bad "agy cannot write while running $role"; fi
done

# --- the stages that need a pty ----------------------------------------------------
# A shell whose stdin is not a terminal cannot allocate one, and script(1) then fails
# before the stand-in agent runs at all, so every case below would fail for the same
# unrelated reason. Say so once. CI has a terminal, so this is not a quiet pass.
pty_available=1
if ! pty_run true >/dev/null 2>&1; then
  pty_available=0
  printf 'SKIP  the run-stage cases: this shell cannot allocate a pty (script: tcgetattr)\n'
fi

if [ "$pty_available" = "1" ]; then
# propose is the one role that runs before the change folder exists.
setup
git worktree add -q .worktrees/issue-4 -b issue-4-gamma 2>/dev/null
bin=$(mktemp -d)
cat > "$bin/claude" <<'FAKE'
#!/usr/bin/env bash
mkdir -p openspec/changes/issue-4-gamma
printf '# Proposal\n' > openspec/changes/issue-4-gamma/proposal.md
echo '{"type":"result","subtype":"success","result":"proposed","session_id":"sess-1"}'
FAKE
chmod +x "$bin/claude"
out=$(cd "$repo" && PATH="$bin:$PATH" "$STAGE" propose claude issue-4-gamma 2>&1); rc=$?
if [ "$rc" = "0" ] && [ -f "$repo/.worktrees/issue-4/openspec/changes/issue-4-gamma/proposal.md" ]; then
  ok "propose runs with no change folder there yet"
else
  bad "propose refused to run before its own output existed (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | head -4
fi
# It shares the planning session slot with archive, so archive resumes what propose wrote.
if [ "$(cat "$repo/.worktrees/issue-4/.agent-runs/plan-claude.conversation" 2>/dev/null)" = "sess-1" ]; then
  ok "and records its session in the shared plan slot"
else bad "propose did not write plan-claude.conversation"; fi
find "$bin" -mindepth 0 -delete 2>/dev/null
teardown

# The commit message is written by the implementer, resuming the session that wrote the
# diff: it is the only participant that knows why the diff looks as it does.
setup
add_change issue-5-delta
add_passing_review issue-5-delta
bin=$(mktemp -d)
cat > "$bin/agy" <<'FAKE'
#!/usr/bin/env bash
mkdir -p .agent-runs
echo touched > touched.txt
printf 'Add a thing\n\nBecause of a reason.\n' > .agent-runs/commit-msg.txt
echo '{"conversation_id":"conv-9","status":"COMPLETED","response":"done"}'
FAKE
chmod +x "$bin/agy"
(cd "$repo" && PATH="$bin:$PATH" "$STAGE" implement agy issue-5-delta >/dev/null 2>&1)
out=$(cd "$repo" && PATH="$bin:$PATH" "$STAGE" commit-msg agy issue-5-delta --resume 2>&1); rc=$?
if [ "$rc" = "0" ]; then ok "commit-msg resumes the implementer's session"
else
  bad "commit-msg could not resume the implementer's session (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | head -4
fi
find "$bin" -mindepth 0 -delete 2>/dev/null
teardown

# A stage that asks stops the loop it is in, rather than being reported as a failure.
setup
add_change issue-6-epsilon
bin=$(mktemp -d)
cat > "$bin/agy" <<'FAKE'
#!/usr/bin/env bash
printf 'Q1: the proposal says both; which is it?\n' > QUESTIONS.md
echo '{"conversation_id":"conv-1","status":"COMPLETED","response":"asked"}'
FAKE
cat > "$bin/codex" <<'FAKE'
#!/usr/bin/env bash
echo '{"type":"thread.started","thread_id":"t-1"}'
echo '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"VERDICT: APPROVE"}}'
FAKE
chmod +x "$bin/agy" "$bin/codex"
# The plan review must pass before an implement stage will start, so give it one.
add_passing_review issue-6-epsilon
out=$(cd "$repo" && PATH="$bin:$PATH" "$APPLY" agy codex issue-6-epsilon 2>&1); rc=$?
if [ "$rc" = "8" ]; then ok "apply.sh stops with exit 8 when a stage asks"
else
  bad "apply.sh did not stop on a question (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | head -6
fi
if printf '%s\n' "$out" | grep -q 'which is it'; then ok "and prints the question itself"
else bad "the question was not surfaced"; fi
find "$bin" -mindepth 0 -delete 2>/dev/null
teardown
fi

# --- swapping the implementer mid-change (#292) -------------------------------------
# Legitimate, and it used to happen silently: the incoming agent got the "start
# implementing" prompt while inheriting the previous one's uncommitted diff and its
# checked boxes, which AGENTS.md makes a claim the next reader trusts.
if [ "$pty_available" = "1" ]; then
swap_prompt() { # swap_prompt <change> <previous-or-empty> -> the prompt the implementer got
  local change="$1" previous="$2" n bin
  n=$(printf '%s' "$change" | sed -n 's/^\(issue-[0-9]\{1,\}\).*/\1/p')
  bin=$(mktemp -d)
  cat > "$bin/opencode" <<'FAKE'
#!/usr/bin/env bash
printf '%s
' "$*" > "$PROMPT_SINK"
echo worked >> worked.txt
echo '{"type":"result","sessionID":"s-1","parts":[{"type":"text","text":"done"}]}'
FAKE
  cat > "$bin/codex" <<'FAKE'
#!/usr/bin/env bash
echo '{"type":"thread.started","thread_id":"t-swap"}'
echo '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"VERDICT: APPROVE"}}'
FAKE
  chmod +x "$bin/opencode" "$bin/codex"
  # What run-stage.sh records before each code-writing run: who is about to write here.
  [ -n "$previous" ] && printf '%s\n' "$previous" > "$repo/.worktrees/$n/.agent-runs/implement.last"
  # The predecessor's uncommitted work: a handover with nothing on the tree is a claim
  # about nothing, and handover_plan now says so.
  printf 'inherited work\n' >> "$repo/.worktrees/$n/inherited.txt"
  ( cd "$repo" && PROMPT_SINK="$bin/prompt" PATH="$bin:$PATH" "$APPLY" opencode codex "$change" --rounds 1 ) >/dev/null 2>&1
  cat "$bin/prompt" 2>/dev/null
  find "$bin" -mindepth 0 -delete 2>/dev/null
}

setup
add_change issue-20-swap
add_passing_review issue-20-swap
mkdir -p "$repo/.worktrees/issue-20/.agent-runs"
got=$(swap_prompt issue-20-swap agy)
if printf '%s' "$got" | grep -q 'A previous implementer, agy'; then
  ok "a swapped implementer is told whose work it inherits"
else bad "a swapped implementer got no handover"; fi
if printf '%s' "$got" | grep -q "claim rather than as fact"; then
  ok "and not to trust the checked boxes it did not tick"
else bad "the handover does not warn about the checked boxes"; fi
teardown

# A returns after B took over. A has a session, so keying on the incoming agent's own
# conversation missed this entirely and let A resume its stale pre-B context over B's
# newer diff. The recorded implementer is what decides.
setup
add_change issue-22-return
add_passing_review issue-22-return
mkdir -p "$repo/.worktrees/issue-22/.agent-runs"
# opencode has its own session here and could resume it. The RECORD says agy went last,
# so that session predates agy's diff and resuming it is worse than starting fresh with a
# handover. Conversation files do not decide this: they are written after the fact, and
# not at all when extraction fails.
printf 'a-session\n' > "$repo/.worktrees/issue-22/.agent-runs/implement-opencode.conversation"
got=$(swap_prompt issue-22-return agy)
if printf '%s' "$got" | grep -q 'A previous implementer, agy'; then
  ok "an agent returning after another names the agent that actually went last"
else bad "a returning agent was handed '$got'"; fi
if ! printf '%s' "$got" | grep -q -- '-s a-session'; then
  ok "and does not resume its own session, which predates that agent's diff"
else bad "a returning agent resumed its stale pre-swap session"; fi
teardown

# The whole path, not run-stage.sh in isolation: a swapped implementer that finds the
# inherited work already correct changes nothing, and the run must still succeed. apply.sh
# forwards only --resume; whether this continues anything is decided inside run-stage.sh,
# so what this proves is that the decision survives the trip through apply.sh.
setup
add_change issue-24-noopswap
add_passing_review issue-24-noopswap
mkdir -p "$repo/.worktrees/issue-24/.agent-runs"
printf 'agy\n' > "$repo/.worktrees/issue-24/.agent-runs/implement.last"
dirty_tree issue-24-noopswap
sbin2=$(mktemp -d)
cat > "$sbin2/opencode" <<'FAKE'
#!/usr/bin/env bash
echo '{"type":"result","sessionID":"s-1","parts":[{"type":"text","text":"the inherited work is already correct"}]}'
FAKE
cat > "$sbin2/codex" <<'FAKE'
#!/usr/bin/env bash
echo '{"type":"thread.started","thread_id":"t-n"}'
echo '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"VERDICT: APPROVE"}}'
FAKE
chmod +x "$sbin2/opencode" "$sbin2/codex"
out=$(cd "$repo" && PATH="$sbin2:$PATH" "$APPLY" opencode codex issue-24-noopswap --rounds 1 2>&1); rc=$?
if [ "$rc" = "0" ]; then ok "a handover through apply.sh may verify and change nothing"
else
  bad "a verifying handover failed through apply.sh (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | tail -4
fi
find "$sbin2" -mindepth 0 -delete 2>/dev/null
teardown

setup
add_change issue-21-noswap
add_passing_review issue-21-noswap
mkdir -p "$repo/.worktrees/issue-21/.agent-runs"
got=$(swap_prompt issue-21-noswap "")
if ! printf '%s' "$got" | grep -q 'A previous implementer'; then
  ok "and a first implementer is told nothing about a predecessor it does not have"
else bad "a handover was invented for a change nobody had implemented"; fi
teardown
fi

# A handover that finds the inherited work already correct changes nothing, and that is a
# real answer. The no-op guard exempted only RESUMED implements, so a swapped implementer
# doing exactly what it was told was reported as a stage that did not run.
if [ "$pty_available" = "1" ]; then
setup
add_change issue-23-noop
add_passing_review issue-23-noop
nbin=$(mktemp -d)
cat > "$nbin/agy" <<'FAKE'
#!/usr/bin/env bash
echo '{"conversation_id":"c","status":"COMPLETED","response":"the inherited work is already correct"}'
FAKE
chmod +x "$nbin/agy"
# A real handover, not an asserted one: the record says another agent holds this tree, so
# run-stage.sh decides for itself that this run continues nothing and may find nothing to
# do. The caller does not get to claim that.
mkdir -p "$repo/.worktrees/issue-23/.agent-runs"
printf 'opencode\n' > "$repo/.worktrees/issue-23/.agent-runs/implement.last"
dirty_tree issue-23-noop
out=$(cd "$repo" && PATH="$nbin:$PATH" "$STAGE" implement agy issue-23-noop 2>&1); rc=$?
if [ "$rc" = "0" ]; then ok "a handover that finds nothing left to do is not a failed run"
else bad "a verification-only handover exited $rc"; fi
# A genuinely first implement, on a change nothing has touched: no record, no session, so
# nothing is being continued and changing nothing means it did not run.
add_change issue-27-first
add_passing_review issue-27-first
out=$(cd "$repo" && PATH="$nbin:$PATH" "$STAGE" implement agy issue-27-first 2>&1); rc=$?
if [ "$rc" = "3" ]; then ok "and a first implement that changes nothing still is"
else bad "a no-op first implement exited $rc, wanted 3"; fi
find "$nbin" -mindepth 0 -delete 2>/dev/null
teardown
fi

# The decision itself, exercised directly, because it is shared by apply.sh and
# run-change.sh and a test through either one leaves the other's path unproven.
setup
add_change issue-25-plan
wt25="$repo/.worktrees/issue-25"
mkdir -p "$wt25/.agent-runs"
handover_plan "$wt25" opencode
if [ -z "$HANDOVER_RESUME" ] && [ -z "$HANDOVER_TEXT" ]; then
  ok "plan: a clean tree gets no resume and no handover"
else bad "plan: a clean tree produced resume='$HANDOVER_RESUME' text='${HANDOVER_TEXT:0:40}'"; fi

# A clean tree that DOES carry a record and a session, which is what a run that changed
# nothing leaves behind. There is still nothing to continue, so it must not resume: the
# retry would otherwise be told to continue work that was never done.
printf 'opencode\n' > "$wt25/.agent-runs/implement.last"
printf 's\n' > "$wt25/.agent-runs/implement-opencode.conversation"
handover_plan "$wt25" opencode
if [ -z "$HANDOVER_RESUME" ] && [ -z "$HANDOVER_TEXT" ]; then
  ok "plan: a clean tree does not resume even with a record and a session on it"
else bad "plan: a clean tree with records gave resume='$HANDOVER_RESUME'"; fi

dirty_tree issue-25-plan
printf 'opencode\n' > "$wt25/.agent-runs/implement.last"
printf 's\n' > "$wt25/.agent-runs/implement-opencode.conversation"
handover_plan "$wt25" opencode
if [ "$HANDOVER_RESUME" = "--resume" ] && [ -z "$HANDOVER_TEXT" ]; then
  ok "plan: its own recorded tree resumes, with nothing to hand over"
else bad "plan: own tree gave resume='$HANDOVER_RESUME'"; fi

printf 'agy\n' > "$wt25/.agent-runs/implement.last"
handover_plan "$wt25" opencode
if [ -z "$HANDOVER_RESUME" ] && printf '%s' "$HANDOVER_TEXT" | grep -q 'A previous implementer, agy'; then
  ok "plan: another agent's tree suppresses the resume and hands over"
else bad "plan: a swap gave resume='$HANDOVER_RESUME' text='${HANDOVER_TEXT:0:40}'"; fi

# Its own tree, but the last run's extraction failed so the id was never captured and the
# older one is invalid: the tree moved under it.
: > "$wt25/.agent-runs/implement-opencode.conversation"
printf 'opencode\n' > "$wt25/.agent-runs/implement.last"
handover_plan "$wt25" opencode
if [ -z "$HANDOVER_RESUME" ] && printf '%s' "$HANDOVER_TEXT" | grep -q 'could not be recovered'; then
  ok "plan: its own tree with no usable session hands over rather than resuming"
else bad "plan: a lost session gave resume='$HANDOVER_RESUME' text='${HANDOVER_TEXT:0:40}'"; fi
printf 's\n' > "$wt25/.agent-runs/implement-opencode.conversation"

find "$wt25/.agent-runs/implement.last" -mindepth 0 -delete 2>/dev/null
handover_plan "$wt25" opencode
if [ -z "$HANDOVER_RESUME" ] && printf '%s' "$HANDOVER_TEXT" | grep -q 'no record of who wrote it'; then
  ok "plan: a session with no record behind it is not resumed on a guess"
else bad "plan: an unrecorded tree gave resume='$HANDOVER_RESUME'"; fi
teardown

# The producer, not just the readers: run-stage.sh must record who is about to write, and
# must do it only once everything that could refuse the run has passed.
if [ "$pty_available" = "1" ]; then
setup
add_change issue-26-record
add_passing_review issue-26-record
rbin=$(mktemp -d)
cat > "$rbin/agy" <<'FAKE'
#!/usr/bin/env bash
echo touched >> touched.txt
echo '{"conversation_id":"c","status":"COMPLETED","response":"done"}'
FAKE
chmod +x "$rbin/agy"
(cd "$repo" && PATH="$rbin:$PATH" "$STAGE" implement agy issue-26-record >/dev/null 2>&1)
if [ "$(cat "$repo/.worktrees/issue-26/.agent-runs/implement.last" 2>/dev/null | tr -d '[:space:]')" = "agy" ]; then
  ok "run-stage records the implementer that is about to write"
else bad "run-stage did not record the implementer"; fi
# A run refused before launch must leave no record: recorded sooner, a launch that never
# happened still named its agent, and the next run resumed that agent's stale session.
# A change whose plan review does not pass is refused by the gate before anything is
# launched, which is exactly the window in which an early recording named an agent that
# never ran.
add_change issue-28-refused
(cd "$repo" && PATH="$rbin:$PATH" "$STAGE" implement agy issue-28-refused >/dev/null 2>&1)
if [ ! -f "$repo/.worktrees/issue-28/.agent-runs/implement.last" ]; then
  ok "and records nothing when the run is refused before it starts"
else bad "a refused run recorded '$(cat "$repo/.worktrees/issue-28/.agent-runs/implement.last")'"; fi
# A recording that cannot be made is fatal: the alternative is an agent editing a tree the
# marker still attributes to somebody else.
add_change issue-29-unwritable
add_passing_review issue-29-unwritable
# A directory where the marker belongs: the write fails while everything else about the
# run stays possible, which is the failure this must not shrug off. Making the whole
# .agent-runs unwritable breaks the log first and never reaches the check.
mkdir -p "$repo/.worktrees/issue-29/.agent-runs/implement.last"
out=$(cd "$repo" && PATH="$rbin:$PATH" "$STAGE" implement agy issue-29-unwritable 2>&1); rc=$?
find "$repo/.worktrees/issue-29/.agent-runs/implement.last" -mindepth 0 -delete 2>/dev/null
if [ "$rc" = "1" ] && printf '%s' "$out" | grep -q 'cannot record the implementer'; then
  ok "a run that cannot record who is writing does not run"
else bad "an unrecordable run exited $rc"; fi
find "$rbin" -mindepth 0 -delete 2>/dev/null
teardown
fi

# The caller's --resume is intent, not fact. apply.sh always passes it, so a FIRST run on
# a clean tree arrives with the flag set: it must still be told to start work rather than
# to continue it, and must still fail if it changes nothing.
if [ "$pty_available" = "1" ]; then
setup
add_change issue-31-intent
add_passing_review issue-31-intent
ibin=$(mktemp -d)
cat > "$ibin/agy" <<'FAKE'
#!/usr/bin/env bash
printf '%s
' "$*" > "$PROMPT_SINK"
echo '{"conversation_id":"c","status":"COMPLETED","response":"did nothing"}'
FAKE
chmod +x "$ibin/agy"
out=$(cd "$repo" && PROMPT_SINK="$ibin/p" PATH="$ibin:$PATH" "$STAGE" implement agy issue-31-intent --resume 2>&1); rc=$?
if [ "$rc" = "3" ]; then ok "a first run given --resume still fails when it changes nothing"
else bad "a first run with --resume that did nothing exited $rc, wanted 3"; fi
if ! grep -q 'Continue your work' "$ibin/p" 2>/dev/null; then
  ok "and is told to start work, not to continue it"
else bad "a first run was told to continue work it had not done"; fi
find "$ibin" -mindepth 0 -delete 2>/dev/null
teardown
fi

# The truncation itself, not a hand-emptied file: a writing run whose extraction fails and
# which captured no id must leave no resumable session behind, and one that DID capture an
# id must keep it. codex and opencode record the id before requiring a final message,
# precisely so an interrupted run can be resumed.
if [ "$pty_available" = "1" ]; then
setup
add_change issue-30-extract
add_passing_review issue-30-extract
ebin=$(mktemp -d)
printf 'stale-id\n' > /dev/null
cat > "$ebin/agy" <<'FAKE'
#!/usr/bin/env bash
echo edited >> edited.txt
echo "console noise and no envelope at all"
FAKE
chmod +x "$ebin/agy"
mkdir -p "$repo/.worktrees/issue-30/.agent-runs"
printf 'stale-id\n' > "$repo/.worktrees/issue-30/.agent-runs/implement-agy.conversation"
dirty_tree issue-30-extract
(cd "$repo" && PATH="$ebin:$PATH" "$STAGE" implement agy issue-30-extract >/dev/null 2>&1)
if [ ! -s "$repo/.worktrees/issue-30/.agent-runs/implement-agy.conversation" ]; then
  ok "a failed extraction discards a session id this run did not capture"
else bad "a stale id survived a failed extraction: $(cat "$repo/.worktrees/issue-30/.agent-runs/implement-agy.conversation")"; fi

# codex captures its thread id first and only then fails to produce an answer. That id is
# this run's and does match the tree, so it must survive.
cat > "$ebin/codex" <<'FAKE'
#!/usr/bin/env bash
echo edited >> edited2.txt
echo '{"type":"thread.started","thread_id":"fresh-id"}'
echo "and then nothing that parses as an answer"
FAKE
chmod +x "$ebin/codex"
(cd "$repo" && PATH="$ebin:$PATH" "$STAGE" implement codex issue-30-extract >/dev/null 2>&1)
if [ "$(tr -d '[:space:]' < "$repo/.worktrees/issue-30/.agent-runs/implement-codex.conversation" 2>/dev/null)" = "fresh-id" ]; then
  ok "and keeps one it did capture, so an interrupted run stays resumable"
else bad "a freshly captured id was discarded"; fi
# A RESUMED run re-emits the id it was given, so the file reads what it read before. That
# is this run's id describing this tree, not a leftover, and it must survive a failed
# extraction of the answer.
printf 'kept-id\n' > "$repo/.worktrees/issue-30/.agent-runs/implement-codex.conversation"
cat > "$ebin/codex" <<'FAKE'
#!/usr/bin/env bash
echo edited >> edited3.txt
echo '{"type":"thread.started","thread_id":"kept-id"}'
echo "and then nothing that parses as an answer"
FAKE
chmod +x "$ebin/codex"
(cd "$repo" && PATH="$ebin:$PATH" "$STAGE" implement codex issue-30-extract --resume >/dev/null 2>&1)
if [ "$(tr -d '[:space:]' < "$repo/.worktrees/issue-30/.agent-runs/implement-codex.conversation" 2>/dev/null)" = "kept-id" ]; then
  ok "and a resumed run's unchanged id is kept, not read as stale"
else bad "a resumed run's own id was discarded as stale"; fi
find "$ebin" -mindepth 0 -delete 2>/dev/null
teardown
fi

# A run that writes nothing records its agent anyway, so on the retry that agent looks
# like it is inheriting its own work. There is nothing to inherit on a clean tree, and a
# no-op must still fail rather than being waved through as an implementation.
if [ "$pty_available" = "1" ]; then
setup
add_change issue-34-twice
add_passing_review issue-34-twice
tbin=$(mktemp -d)
cat > "$tbin/agy" <<'FAKE'
#!/usr/bin/env bash
echo '{"conversation_id":"c","status":"COMPLETED","response":"did nothing at all"}'
FAKE
chmod +x "$tbin/agy"
(cd "$repo" && PATH="$tbin:$PATH" "$STAGE" implement agy issue-34-twice >/dev/null 2>&1); first=$?
(cd "$repo" && PATH="$tbin:$PATH" "$STAGE" implement agy issue-34-twice >/dev/null 2>&1); second=$?
if [ "$first" = "3" ] && [ "$second" = "3" ]; then
  ok "a no-op does not become acceptable by being attempted twice"
else bad "no-op runs exited $first then $second, wanted 3 then 3"; fi
find "$tbin" -mindepth 0 -delete 2>/dev/null
teardown
fi

# Clearing the session it declined to resume is what keeps a stale id from being paired
# with a fresh record, so a failure to clear must stop the run rather than proceed.
if [ "$pty_available" = "1" ]; then
setup
add_change issue-33-unclearable
add_passing_review issue-33-unclearable
ubin=$(mktemp -d)
cat > "$ubin/agy" <<'FAKE'
#!/usr/bin/env bash
echo ran >> ran.txt
echo '{"conversation_id":"c","status":"COMPLETED","response":"done"}'
FAKE
chmod +x "$ubin/agy"
mkdir -p "$repo/.worktrees/issue-33/.agent-runs"
dirty_tree issue-33-unclearable
printf 'opencode
' > "$repo/.worktrees/issue-33/.agent-runs/implement.last"
# A directory where the session file belongs: the clear fails and nothing else does.
mkdir -p "$repo/.worktrees/issue-33/.agent-runs/implement-agy.conversation"
out=$(cd "$repo" && PATH="$ubin:$PATH" "$STAGE" implement agy issue-33-unclearable 2>&1); rc=$?
if [ "$rc" = "1" ] && printf '%s' "$out" | grep -q 'cannot clear the stale session'; then
  ok "a handover that cannot clear the old session does not run"
else bad "an unclearable session gave exit $rc"; fi
if [ ! -f "$repo/.worktrees/issue-33/ran.txt" ]; then
  ok "and the agent never launched"
else bad "the agent ran despite an unclearable session"; fi
find "$ubin" -mindepth 0 -delete 2>/dev/null
teardown
fi

# The lock's atomicity, provoked rather than asserted. A barrier in the `date` call the
# lock line makes holds two stages until both have arrived, so both reach the redirection
# together: noclobber lets exactly one through, and the check-then-write form it replaced
# let both. Without the barrier this is a race nobody can schedule on purpose.
if [ "$pty_available" = "1" ]; then
setup
add_change issue-32-lock
add_passing_review issue-32-lock
lbin=$(mktemp -d)
barrier=$(mktemp -d)
cat > "$lbin/date" <<FAKE
#!/usr/bin/env bash
touch "$barrier/\$\$"
for _ in \$(seq 1 100); do
  [ "\$(find "$barrier" -type f -name '[0-9]*' | wc -l)" -ge 2 ] && break
  sleep 0.1
done
exec /bin/date "\$@"
FAKE
cat > "$lbin/agy" <<FAKE
#!/usr/bin/env bash
echo launched >> "$barrier/launches"
echo '{"conversation_id":"c","status":"COMPLETED","response":"done"}'
FAKE
chmod +x "$lbin/date" "$lbin/agy"
( cd "$repo" && PATH="$lbin:$PATH" "$STAGE" implement agy issue-32-lock >"$barrier/out1" 2>&1 ) &
( cd "$repo" && PATH="$lbin:$PATH" "$STAGE" implement agy issue-32-lock >"$barrier/out2" 2>&1 ) &
wait
# Both must actually have reached the barrier, or this proves nothing: one stage failing
# early leaves the other to time out, launch alone, and satisfy a bare count of one.
arrived=$(find "$barrier" -type f -name '[0-9]*' | wc -l | tr -d ' ')
launches=$(grep -c '' "$barrier/launches" 2>/dev/null || true)
refused=$(grep -l 'already in progress' "$barrier/out1" "$barrier/out2" 2>/dev/null | wc -l | tr -d ' ')
if [ "$arrived" = "2" ] && [ "${launches:-0}" = "1" ] && [ "$refused" = "1" ]; then
  ok "two stages racing for the lock launch exactly one agent"
else bad "race: $arrived arrived, ${launches:-0} launched, $refused refused; wanted 2/1/1"; fi
find "$lbin" "$barrier" -mindepth 0 -delete 2>/dev/null
teardown
fi

# --- the guards a role change could silently unkey ---------------------------------
if [ "$pty_available" = "1" ]; then
setup
add_change issue-7-zeta
add_passing_review issue-7-zeta
bin=$(mktemp -d)
# A reviewer that edits anything at all. The change folder included: an earlier version
# excluded it from the digest, which let a reviewer rewrite the very plan it judged.
cat > "$bin/codex" <<'FAKE'
#!/usr/bin/env bash
echo "edited by the reviewer" >> openspec/changes/issue-7-zeta/proposal.md
echo '{"type":"thread.started","thread_id":"t-2"}'
echo '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"VERDICT: APPROVE"}}'
FAKE
chmod +x "$bin/codex"
out=$(cd "$repo" && PATH="$bin:$PATH" "$STAGE" review codex issue-7-zeta 2>&1); rc=$?
if [ "$rc" = "5" ]; then ok "a reviewer that edits the change folder is caught"
else
  bad "a reviewer edited the plan it was judging and passed (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | head -4
fi
# The same case under macOS's system bash. These scripts say `#!/usr/bin/env bash`, which
# on a developer machine finds bash 5 from Homebrew and hides everything bash 3.2 refuses
# - expanding an empty array under `set -u` among them, which aborted every stage before
# it ran. Run explicitly under /bin/bash so that never passes unnoticed again.
if [ -x /bin/bash ]; then
  out=$(cd "$repo" && PATH="$bin:$PATH" /bin/bash "$STAGE" review codex issue-7-zeta 2>&1); rc=$?
  if [ "$rc" = "5" ]; then ok "the guards still fire under $(/bin/bash -c 'echo $BASH_VERSION')"
  else
    bad "run-stage.sh under /bin/bash exited $rc, not 5"
    printf '%s\n' "$out" | sed 's/^/        /' | head -4
  fi
fi

# A reviewer that commits its edit leaves a clean status and an empty `git diff HEAD`.
# Only HEAD itself gives it away.
cat > "$bin/codex" <<'FAKE'
#!/usr/bin/env bash
echo "smuggled" >> openspec/changes/issue-7-zeta/proposal.md
git add -A >/dev/null 2>&1
git -c user.email=t@t -c user.name=t commit -qm "reviewer commit" >/dev/null 2>&1
echo '{"type":"thread.started","thread_id":"t-9"}'
echo '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"VERDICT: APPROVE"}}'
FAKE
chmod +x "$bin/codex"
out=$(cd "$repo" && PATH="$bin:$PATH" "$STAGE" review codex issue-7-zeta 2>&1); rc=$?
if [ "$rc" = "5" ]; then ok "a reviewer that commits its edit is caught"
else
  bad "a reviewer committed its edit and passed (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | head -4
fi

# ANSWERS.md is the human's answer to a blocking question. Stages read it; none writes it.
add_change issue-12-lambda
add_passing_review issue-12-lambda
printf 'A1: the first one\n' > "$repo/.worktrees/issue-12/ANSWERS.md"
cat > "$bin/codex" <<'FAKE'
#!/usr/bin/env bash
echo "actually, the second one" >> ANSWERS.md
echo '{"type":"thread.started","thread_id":"t-10"}'
echo '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"VERDICT: APPROVE"}}'
FAKE
chmod +x "$bin/codex"
out=$(cd "$repo" && PATH="$bin:$PATH" "$STAGE" review codex issue-12-lambda 2>&1); rc=$?
if [ "$rc" = "5" ]; then ok "a stage that rewrites the human's answers is caught"
else bad "a stage rewrote ANSWERS.md and passed (exit $rc)"; fi

# An implement run that exits cleanly having written nothing did not run.
cat > "$bin/agy" <<'FAKE'
#!/usr/bin/env bash
echo '{"conversation_id":"c","status":"COMPLETED","response":"did nothing"}'
FAKE
chmod +x "$bin/agy"
out=$(cd "$repo" && PATH="$bin:$PATH" "$STAGE" implement agy issue-7-zeta 2>&1); rc=$?
if [ "$rc" = "3" ]; then ok "an implement run that changed nothing is refused"
else bad "a no-op implement was reported as a run (exit $rc)"; fi
# The lock is what keeps a commit, a merge or a push from landing under a writing stage.
printf 'someone else\n' > "$repo/.git/APPLY_IN_PROGRESS"
out=$(cd "$repo" && PATH="$bin:$PATH" "$STAGE" implement agy issue-7-zeta 2>&1); rc=$?
if [ "$rc" = "1" ] && printf '%s' "$out" | grep -q 'already in progress'; then
  ok "a writing stage refuses to start while another holds the lock"
else bad "the apply lock did not stop a second writing stage (exit $rc)"; fi
find "$repo/.git/APPLY_IN_PROGRESS" -delete 2>/dev/null
find "$bin" -mindepth 0 -delete 2>/dev/null
teardown

# A verdict is a whole line. Read as a prefix, "VERDICT: APPROVE WITH CHANGES" - the
# reviewer spelling it with spaces - becomes a plain APPROVE that skips the required
# changes and that the commit gate then accepts.
setup
add_change issue-9-theta
add_passing_review issue-9-theta
bin=$(mktemp -d)
cat > "$bin/agy" <<'FAKE'
#!/usr/bin/env bash
echo worked >> worked.txt
echo '{"conversation_id":"c","status":"COMPLETED","response":"done"}'
FAKE
cat > "$bin/codex" <<'FAKE'
#!/usr/bin/env bash
echo '{"type":"thread.started","thread_id":"t-4"}'
echo '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"VERDICT: APPROVE WITH CHANGES"}}'
FAKE
chmod +x "$bin/agy" "$bin/codex"
out=$(cd "$repo" && PATH="$bin:$PATH" "$APPLY" agy codex issue-9-theta 2>&1); rc=$?
if [ "$rc" = "4" ]; then ok "a verdict that is not exactly APPROVE or REVISE is refused"
else
  bad "a prefix of APPROVE was read as an approval (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | head -4
fi
if [ "$(cd "$repo/.worktrees/issue-9" && grep -c '^VERDICT' openspec/changes/issue-9-theta/diff-review.md 2>/dev/null || echo 0)" = "0" ]; then
  ok "and no diff-review.md was recorded from it"
else bad "an unreadable verdict was recorded as a diff review"; fi
find "$bin" -mindepth 0 -delete 2>/dev/null
teardown

# The commit-message stage runs after the review and after the gates, so anything it
# changes would be committed having passed neither.
setup
add_change issue-10-iota
add_passing_review issue-10-iota
bin=$(mktemp -d)
cat > "$bin/agy" <<'FAKE'
#!/usr/bin/env bash
mkdir -p .agent-runs src
echo 'fn main() {}' >> src/sneak.rs
printf 'Add a thing
' > .agent-runs/commit-msg.txt
echo '{"conversation_id":"c","status":"COMPLETED","response":"done"}'
FAKE
chmod +x "$bin/agy"
(cd "$repo" && PATH="$bin:$PATH" "$STAGE" implement agy issue-10-iota >/dev/null 2>&1)
out=$(cd "$repo" && PATH="$bin:$PATH" "$STAGE" commit-msg agy issue-10-iota --resume 2>&1); rc=$?
if [ "$rc" = "5" ]; then ok "a commit-message stage that edits code is caught"
else
  bad "the commit-message stage changed code and was accepted (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | head -4
fi
find "$bin" -mindepth 0 -delete 2>/dev/null
teardown

# The stages after archive still need the change, and by then it has moved.
setup
add_change issue-8-eta
mkdir -p "$repo/.worktrees/issue-8/openspec/changes/archive/2026-01-01-issue-8-eta"
find "$repo/.worktrees/issue-8/openspec/changes/issue-8-eta" -mindepth 0 -delete 2>/dev/null
bin=$(mktemp -d)
cat > "$bin/codex" <<'FAKE'
#!/usr/bin/env bash
echo '{"type":"thread.started","thread_id":"t-3"}'
echo '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"VERDICT: APPROVE"}}'
FAKE
chmod +x "$bin/codex"
out=$(cd "$repo" && PATH="$bin:$PATH" "$STAGE" review codex issue-8-eta 2>&1); rc=$?
if [ "$rc" = "0" ]; then ok "a stage finds its change once archive has moved it"
else
  bad "a stage after archive could not find its change (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | head -4
fi
out=$(cd "$repo" && PATH="$bin:$PATH" "$STAGE" review codex issue-8-nosuch 2>&1); rc=$?
if [ "$rc" = "2" ]; then ok "and a change that exists nowhere is still refused"
else bad "a nonexistent change was accepted (exit $rc)"; fi
find "$bin" -mindepth 0 -delete 2>/dev/null
teardown
fi

# --- detaching a long run (#284) ---------------------------------------------------
# The line this replaced named setsid and timeout, and macOS ships neither: nohup does
# report the missing binary and exits 127, but the `&` throws that away, because a shell
# reports 0 for having STARTED a background job whatever becomes of it.
DETACH="$here/detach.sh"

# Every launcher is exercised where it exists, through DETACH_LAUNCHER. Left to pick for
# itself, a Linux runner would only ever take the setsid branch and a macOS one only the
# python3 branch, so half the code would never run anywhere.
detach_case() { # detach_case <launcher>
  local L="$1" d h out st
  d=$(mktemp -d)
  h=$(DETACH_LAUNCHER="$L" "$DETACH" "$d/run" bash -c 'echo one; echo two; exit 4' 2>/dev/null)
  "$DETACH" --wait "$h" 60 >/dev/null 2>&1; st=$?
  if [ "$st" = "4" ]; then ok "$L: the run's exit status comes back"
  else bad "$L: --wait returned $st, wanted 4"; fi
  out=$(cat "$h" 2>/dev/null)
  if [ "$out" = "one
two" ]; then ok "$L: the log holds the run's output and nothing else"
  else bad "$L: the log reads '$out'"; fi
  find "$d" -mindepth 0 -delete 2>/dev/null
}
detach_case nohup
for L in python3 setsid; do
  if command -v "$L" >/dev/null 2>&1; then detach_case "$L"
  else printf 'SKIP  %s: not on this machine; another platform covers it\n' "$L"; fi
done

# Detachment is the point, and only a new session survives a harness reaping a process
# group. nohup gives SIGHUP immunity and nothing more, which is why it is the last
# resort rather than the macOS answer.
if command -v python3 >/dev/null 2>&1; then
  d=$(mktemp -d)
  mine=$(python3 -c 'import os; print(os.getsid(0))')
  h=$(DETACH_LAUNCHER='python3' "$DETACH" "$d/sid" python3 -c 'import os; print(os.getsid(0))' 2>/dev/null)
  "$DETACH" --wait "$h" 60 >/dev/null 2>&1
  theirs=$(tr -d '[:space:]' < "$h" 2>/dev/null)
  if [ -n "$theirs" ] && [ "$theirs" != "$mine" ]; then ok "python3: the run gets its own session"
  else bad "python3: the run shares session '$mine' with the launcher"; fi
  h=$(DETACH_LAUNCHER='nohup' "$DETACH" "$d/sid2" python3 -c 'import os; print(os.getsid(0))' 2>/dev/null)
  "$DETACH" --wait "$h" 60 >/dev/null 2>&1
  theirs=$(tr -d '[:space:]' < "$h" 2>/dev/null)
  if [ "$theirs" = "$mine" ]; then ok "nohup: and does not, which is why it warns"
  else bad "nohup unexpectedly created a session: '$theirs' vs '$mine'"; fi
  find "$d" -mindepth 0 -delete 2>/dev/null
fi

d=$(mktemp -d)
# Two launches on one prefix are two runs, not a race to be survived. Earlier versions
# shared a log and kept the launches apart with a pointer and then a lock; four review
# rounds found four races in that machinery. Nothing is shared here, so there is none.
h1=$("$DETACH" "$d/p" bash -c 'echo A; sleep 4; exit 11' 2>/dev/null)
h2=$("$DETACH" "$d/p" bash -c 'echo B; exit 22' 2>/dev/null)
# Structural rather than probabilistic: mktemp creates the name atomically, so this
# asserts the interface rather than trying to provoke a collision, which no sequential
# test could do reliably.
if [ "$h1" != "$h2" ]; then ok "two launches on one prefix get different handles"
else bad "two launches collided on the handle $h1"; fi
"$DETACH" --wait "$h2" 60 >/dev/null 2>&1; st=$?
if [ "$st" = "22" ] && [ "$(cat "$h2")" = "B" ]; then ok "the second answers for itself while the first still runs"
else bad "the second launch returned $st with log '$(cat "$h2" 2>/dev/null)'"; fi
"$DETACH" --wait "$h1" 60 >/dev/null 2>&1; st=$?
if [ "$st" = "11" ] && [ "$(cat "$h1")" = "A" ]; then ok "and the first keeps its own status and transcript"
else bad "the first launch returned $st with log '$(cat "$h1" 2>/dev/null)'"; fi

# An empty log counts as zero lines, not "0\n0". `grep -c` prints 0 and exits 1 when it
# matches nothing, so the obvious `|| echo 0` fallback fires on top of the 0 it printed.
h=$("$DETACH" "$d/quiet" bash -c 'exit 0' 2>/dev/null)
out=$("$DETACH" --wait "$h" 60 2>&1); st=$?
if [ "$st" = "0" ] && [ "$out" = "finished: exit 0 after 0 lines" ]; then
  ok "a silent run reports zero lines and exit 0"
else bad "a silent run reported '$out' (exit $st)"; fi
# Gated on a file rather than a timer, so the suite ends it deterministically instead of
# hunting it with a system-wide pkill that could match somebody else's work.
h=$("$DETACH" "$d/slow" bash -c 'while [ ! -f "'"$d"'/release" ]; do sleep 1; done' 2>/dev/null)
started=$(date +%s)
out=$("$DETACH" --wait "$h" 6 2>&1); st=$?
elapsed=$(( $(date +%s) - started ))
if [ "$st" = "1" ] && printf '%s' "$out" | grep -q 'still running after 6s'; then
  ok "--wait gives up at its deadline rather than blocking forever"
else bad "--wait past its deadline gave '$out' (exit $st)"; fi
# A fixed five-second step would sleep past a deadline of six and accept a run that
# finished at seven, which is a bound the caller did not give.
# Nine, not eight: the distinction being proved is 6 against the 10 a fixed five-second
# step would reach, so the tolerance only has to stay under 10, and a loaded runner
# should not fail correct code for a second of scheduling.
if [ "$elapsed" -le 9 ]; then ok "and returns at the deadline, not past the next whole step"
else bad "--wait with a 6s deadline took ${elapsed}s"; fi
: > "$d/release"
"$DETACH" --wait "$h" 30 >/dev/null 2>&1

# Nothing announced itself, so there is no run to wait for. This is the original bug's
# shape: a launcher that starts nothing, reported as a clean pass.
: > "$d/ghost"
out=$("$DETACH" --wait "$d/ghost" 5 2>&1); st=$?
if [ "$st" = "1" ] && printf '%s' "$out" | grep -q 'nothing has started'; then
  ok "a launch that started nothing is reported, not waited on forever"
else bad "a ghost launch gave '$out' (exit $st)"; fi

# A launcher that cannot detach must not run the command anyway, and the launch must say
# so in its status. The REAL handler is exercised: python imports
# sitecustomize at startup, so making os.setsid raise there drives detach.sh's own except
# branch. Reverting that branch to a bare `pass` fails this, which a stand-in python could
# never do.
if command -v python3 >/dev/null 2>&1; then
  pylib=$(mktemp -d)
  cat > "$pylib/sitecustomize.py" <<'FAKE'
import os
def _fail():
    raise OSError("forced by the test suite")
os.setsid = _fail
FAKE
  out=$(DETACH_LAUNCHER='python3' PYTHONPATH="$pylib" DETACH_PATIENCE=2 \
        "$DETACH" "$d/nosid" bash -c 'echo should-not-run' 2>&1 >/dev/null); st=$?
  if [ "$st" = "1" ] && printf '%s' "$out" | grep -q 'nothing started'; then
    ok "a launcher that cannot detach fails the launch"
  else bad "a failed detach gave '$out' (exit $st)"; fi
  log=$(find "$d" -maxdepth 1 -name 'nosid.*' ! -name '*.started' ! -name '*.exit' | head -1)
  if grep -q 'setsid failed' "$log" 2>/dev/null && ! grep -q 'should-not-run' "$log" 2>/dev/null; then
    ok "and the command did not run, which is the point"
  else bad "the failed-detach log reads '$(cat "$log" 2>/dev/null)'"; fi
  find "$pylib" -mindepth 0 -delete 2>/dev/null
fi

# A launch that cannot see its run start must say so in its STATUS. The handle is printed
# either way: an earlier version withheld it and gated the child on a go-ahead marker, and
# that machinery cost three ordering bugs in three review rounds to defend against a late
# child that then runs correctly. What matters is that the caller can tell.
qbin=$(mktemp -d)
printf '#!/usr/bin/env bash\nexit 0\n' > "$qbin/python3"
chmod +x "$qbin/python3"
out=$(DETACH_LAUNCHER='python3' PATH="$qbin:$PATH" DETACH_PATIENCE=1 "$DETACH" "$d/quietfail" bash -c 'true' 2>/dev/null); st=$?
if [ "$st" = "1" ]; then ok "a launch whose run never announces itself exits non-zero"
else bad "a silent launcher returned $st"; fi
if [ -n "$out" ]; then ok "and still hands back the handle, so the caller can look"
else bad "a failed launch printed no handle"; fi
find "$qbin" -mindepth 0 -delete 2>/dev/null

# An announcement landing in the loop's FINAL tick. The loop tests and then sleeps, so
# without a check after the last sleep this child is reported as never having started.
# Pinned to one whole-second tick with a one-second patience, so the announcement at half
# a second falls squarely inside that last sleep rather than depending on timing luck.
fbin=$(mktemp -d)
cat > "$fbin/python3" <<'FAKE'
#!/usr/bin/env bash
shift 2
( sleep 0.5; exec "$@" ) >/dev/null 2>&1 &
exit 0
FAKE
chmod +x "$fbin/python3"
fh=$(DETACH_LAUNCHER='python3' PATH="$fbin:$PATH" DETACH_TICK=1 DETACH_PATIENCE=1 \
     "$DETACH" "$d/finaltick" bash -c 'echo ok; exit 0' 2>/dev/null); fst=$?
if [ "$fst" = "0" ]; then ok "an announcement in the final tick is seen, not missed"
else bad "a child announcing in the final tick was reported as never started (exit $fst)"; fi
"$DETACH" --wait "$fh" 30 >/dev/null 2>&1
find "$fbin" -mindepth 0 -delete 2>/dev/null

# The other half: a child that announces late. The stand-in delays and then execs the REAL
# child body, so what is being observed is the code under test. The run happens, late, and
# --wait on the handle the launch returned still gets the truth about it.
sbin=$(mktemp -d)
cat > "$sbin/python3" <<'FAKE'
#!/usr/bin/env bash
# argv here is: -c <program> bash -c <the real child body> _ <command...>
shift 2
( sleep 3; exec "$@" ) >/dev/null 2>&1 &
exit 0
FAKE
chmod +x "$sbin/python3"
lh=$(DETACH_LAUNCHER='python3' PATH="$sbin:$PATH" DETACH_PATIENCE=1 \
     "$DETACH" "$d/late" bash -c 'echo LATE-RAN; exit 9' 2>/dev/null); lst=$?
if [ "$lst" = "1" ] && [ -n "$lh" ]; then ok "a late child's launch reports non-zero and returns its handle"
else bad "a late child's launch returned $lst with handle '$lh'"; fi
"$DETACH" --wait "$lh" 30 >/dev/null 2>&1; st=$?
if [ "$st" = "9" ] && grep -q 'LATE-RAN' "$lh" 2>/dev/null; then
  ok "and waiting on that handle still gets the truth about the run"
else bad "waiting on a late run gave $st with log '$(cat "$lh" 2>/dev/null)'"; fi
find "$sbin" -mindepth 0 -delete 2>/dev/null

# The log going away WHILE the wait is in progress, which is the case the post-exit
# check below cannot reach.
h=$("$DETACH" "$d/vanish" bash -c 'while [ ! -f "'"$d"'/vrelease" ]; do sleep 1; done' 2>/dev/null)
( sleep 2; find "$h" -mindepth 0 -delete 2>/dev/null ) &
"$DETACH" --wait "$h" 30 >/dev/null 2>&1
if [ "$?" = "2" ]; then ok "a log that vanishes mid-wait is an error, not a silent wait"
else bad "a log vanishing mid-wait was not reported"; fi
: > "$d/vrelease"

# The run's own files are what --wait reports on; without them it has nothing to say.
: > "$d/gone"
: > "$d/gone.started"
printf '0\n' > "$d/gone.exit"
find "$d/gone" -mindepth 0 -delete 2>/dev/null
"$DETACH" --wait "$d/gone" 5 >/dev/null 2>&1
if [ "$?" = "2" ]; then ok "a handle whose log has gone is an error, not a zero-line run"
else bad "a vanished log was reported as a run"; fi

# A status that is empty, not a number, or above 255 is a run that ended without saying
# how. `exit 999` would silently become 231.
for bad_status in '' 'garbage' '999'; do
  name="s$(printf '%s' "${bad_status:-empty}" | tr -cd 'a-z0-9')"
  : > "$d/$name"
  : > "$d/$name.started"
  printf '%s\n' "$bad_status" > "$d/$name.exit"
  out=$("$DETACH" --wait "$d/$name" 5 2>&1); st=$?
  if [ "$st" = "1" ]; then ok "an exit status of '${bad_status:-empty}' is a failure, not a pass"
  else bad "an exit status of '${bad_status:-empty}' gave exit $st: $out"; fi
done

# The refusals. A command that is not there must fail loudly, since the whole point is
# that the old form failed silently with a zero exit.
for args in "" "--wait" "$d/only"; do
  # shellcheck disable=SC2086
  "$DETACH" $args >/dev/null 2>&1
  rc=$?
  if [ "$rc" = "2" ]; then ok "detach refuses '${args:-no arguments}'"
  else bad "detach with '${args:-no arguments}' exited $rc, wanted 2"; fi
done
"$DETACH" --wait "$d/no-such-handle" 5 >/dev/null 2>&1
if [ "$?" = "2" ]; then ok "and refuses to wait on a handle that does not exist"
else bad "waiting on a nonexistent handle was accepted"; fi
out=$("$DETACH" "$d/x" nosuchcommandanywhere 2>&1); rc=$?
if [ "$rc" = "2" ] && printf '%s' "$out" | grep -q 'not on PATH'; then
  ok "and refuses a command that is not on PATH, loudly"
else bad "a missing command exited $rc: $out"; fi
if ! DETACH_LAUNCHER='wat' "$DETACH" "$d/y" true >/dev/null 2>&1; then
  ok "and refuses an unknown DETACH_LAUNCHER"
else bad "an unknown DETACH_LAUNCHER was accepted"; fi
find "$d" -mindepth 0 -delete 2>/dev/null

printf '\n%s passed, %s failed\n' "$pass" "$fail"
[ "$fail" = "0" ]
