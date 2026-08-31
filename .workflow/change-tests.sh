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

printf '\n%s passed, %s failed\n' "$pass" "$fail"
[ "$fail" = "0" ]
