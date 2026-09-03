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
# Not covered, and worth knowing: the plan-review gate preflight is asserted by
# apply-tests.sh, along with the unreadable-review refusal it shares with the section
# below on a stage that gives no account of itself; the driver's own top -
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
printf '# Diff review\n\nAUTHORS: agy\nREVIEWER: codex\nVERDICT: REVISE\n' > "$d/diff-review.md"
expect_stage apply "a REVISE diff review stays in apply"
printf '# Diff review\n\nAUTHORS: agy\nREVIEWER: codex\nVERDICT: APPROVE\n' > "$d/diff-review.md"
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

# --- the apply lock is per worktree, not per repository (#294) ------------------------
# Keyed on the shared git dir it blocked every tree while any one was written: a finished,
# reviewed change could not be pushed because an unrelated agent was writing elsewhere.
#
# Proven with the same barrier the atomicity test uses, and for the same reason: two
# stages that merely start near each other prove nothing, because a suite that runs them
# sequentially passes a test that only checks both exit 0. Held in `date` until both have
# arrived, they reach their lock lines together. Keyed per worktree both take one and both
# launch; keyed on the common dir this is the old race, and exactly one survives it.
if [ "$pty_available" = "1" ]; then
setup
add_change issue-40-locka
add_passing_review issue-40-locka
add_change issue-41-lockb
add_passing_review issue-41-lockb
kbin=$(mktemp -d)
kbar=$(mktemp -d)
cat > "$kbin/date" <<FAKE
#!/usr/bin/env bash
touch "$kbar/\$\$"
for _ in \$(seq 1 100); do
  [ "\$(find "$kbar" -type f -name '[0-9]*' | wc -l)" -ge 2 ] && break
  sleep 0.1
done
exec /bin/date "\$@"
FAKE
cat > "$kbin/agy" <<FAKE
#!/usr/bin/env bash
echo launched >> "$kbar/launches"
echo worked >> worked.txt   # in the worktree: a run that changes nothing exits 3
echo '{"conversation_id":"c","status":"COMPLETED","response":"done"}'
FAKE
chmod +x "$kbin/date" "$kbin/agy"
( cd "$repo" && PATH="$kbin:$PATH" "$STAGE" implement agy issue-40-locka >"$kbar/out1" 2>&1 ) &
first_pid=$!
( cd "$repo" && PATH="$kbin:$PATH" "$STAGE" implement agy issue-41-lockb >"$kbar/out2" 2>&1 ) &
second_pid=$!
# These jobs only. A bare `wait` waits on every background job the shell still has, which
# in a suite that launches detached runs is a wait on things that outlive it.
wait "$first_pid"; first=$?
wait "$second_pid"; second=$?
arrived=$(find "$kbar" -type f -name '[0-9]*' | wc -l | tr -d ' ')
launches=$(grep -c '' "$kbar/launches" 2>/dev/null || true)
if [ "$arrived" = "2" ] && [ "${launches:-0}" = "2" ] && [ "$first" = "0" ] && [ "$second" = "0" ]; then
  ok "two stages overlapping in different worktrees both run"
else bad "overlap: $arrived arrived, ${launches:-0} launched, exits $first/$second; wanted 2/2/0/0"; fi

# And the same worktree still refuses, which is the case the lock exists for. The path is
# spelled out rather than asked of git, because asking git the question the code asks
# would pass whatever the code decided the answer was.
printf 'agy issue-40-locka started now (pid 1)\n' > "$repo/.git/worktrees/issue-40/APPLY_IN_PROGRESS"
out=$(cd "$repo" && PATH="$kbin:$PATH" "$STAGE" implement agy issue-40-locka 2>&1); rc=$?
if [ "$rc" = "1" ] && printf '%s' "$out" | grep -q 'already in progress'; then
  ok "and the worktree holding the lock still refuses a second stage"
else bad "a second stage in the locked worktree exited $rc"; fi

# The hooks must find the same file the runner writes. A hook that stops seeing the lock
# looks exactly like a hook that passes, and silently removes the protection this whole
# change is about, so each one is run against a lock a stage really left behind.
( cd "$repo/.worktrees/issue-40" && git config core.hooksPath "$here/../.githooks" )
( cd "$repo/.worktrees/issue-41" && git config core.hooksPath "$here/../.githooks" )
hc=$(cd "$repo/.worktrees/issue-40" && "$here/../.githooks/pre-commit" 2>&1); hcrc=$?
if [ "$hcrc" = "1" ] && printf '%s' "$hc" | grep -q 'apply is in progress here'; then
  ok "pre-commit sees the lock the runner wrote"
else bad "pre-commit missed the runner's lock (exit $hcrc)"; fi
hp=$(cd "$repo/.worktrees/issue-40" && "$here/../.githooks/pre-push" 2>&1); hprc=$?
if [ "$hprc" = "1" ] && printf '%s' "$hp" | grep -q 'apply is in progress here'; then
  ok "pre-push sees it too"
else bad "pre-push missed the runner's lock (exit $hprc)"; fi
# The other tree is not locked, and must not be stopped by this one.
if (cd "$repo/.worktrees/issue-41" && "$here/../.githooks/pre-commit" >/dev/null 2>&1) &&
   (cd "$repo/.worktrees/issue-41" && "$here/../.githooks/pre-push" >/dev/null 2>&1); then
  ok "an unlocked worktree may still commit and push"
else bad "an unlocked worktree was refused by a lock held elsewhere"; fi
# A merge involves two trees, and asks about those two only (#316). Repository-wide it
# refused a finished change because an unrelated agent was writing elsewhere, which is the
# defect #294 was filed on surviving in the path #294 exempted. Staged with --no-commit,
# which leaves MERGE_HEAD exactly as the hook meets it and runs no hook of its own; both
# sides need a commit the other lacks, or there is no merge to stage.
git -C "$repo/.worktrees/issue-40" commit -q --no-verify --allow-empty -m locka
git -C "$repo" commit -q --no-verify --allow-empty -m meanwhile
git -C "$repo" merge --no-ff --no-commit -q issue-40-locka >/dev/null 2>&1 || true
printf 'agy issue-40-locka started now (pid 1)\n' > "$repo/.git/worktrees/issue-40/APPLY_IN_PROGRESS"
hm=$(cd "$repo" && "$here/../.githooks/pre-merge-commit" 2>&1); hmrc=$?
if [ "$hmrc" = "1" ] && printf '%s' "$hm" | grep -q 'holds what you are merging'; then
  ok "pre-merge-commit refuses while the tree holding the merged branch is locked"
else bad "pre-merge-commit missed the lock on the tree it was merging from (exit $hmrc)"; fi
find "$repo/.git/worktrees/issue-40/APPLY_IN_PROGRESS" -mindepth 0 -delete 2>/dev/null
# The regression #316 is about: a lock held by a tree this merge does not touch.
printf 'agy issue-41-lockb started now (pid 2)\n' > "$repo/.git/worktrees/issue-41/APPLY_IN_PROGRESS"
if (cd "$repo" && "$here/../.githooks/pre-merge-commit" >/dev/null 2>&1); then
  ok "and allows it while only an unrelated worktree is locked"
else bad "an unrelated worktree's lock refused a merge (#316)"; fi
find "$repo/.git/worktrees/issue-41/APPLY_IN_PROGRESS" -mindepth 0 -delete 2>/dev/null
# The tree being merged INTO, read the way pre-commit and pre-push read their own.
printf 'someone\n' > "$repo/.git/APPLY_IN_PROGRESS"
hm=$(cd "$repo" && "$here/../.githooks/pre-merge-commit" 2>&1); hmrc=$?
if [ "$hmrc" = "1" ] && printf '%s' "$hm" | grep -q 'in progress here'; then
  ok "and refuses when the tree being merged into is itself mid-apply"
else bad "pre-merge-commit missed a lock held by the tree it lands in (exit $hmrc)"; fi
find "$repo/.git/APPLY_IN_PROGRESS" -mindepth 0 -delete 2>/dev/null
if (cd "$repo" && "$here/../.githooks/pre-merge-commit" >/dev/null 2>&1); then
  ok "and allows the merge once nothing holds a lock"
else bad "pre-merge-commit refused a merge with no lock held"; fi
# A worktree whose path contains a space, holding the branch being merged. Git names its
# admin directory after the basename and sanitises it, so the entry is `has-space` while
# the path git hands back is not; the hook takes that path from git's registry and must
# carry it whole. Spelt out rather than asked of git, so that a future git which stopped
# sanitising would fail here rather than quietly hand the hook a path it splits.
git -C "$repo" merge --abort 2>/dev/null || true
git -C "$repo" worktree add -q ".worktrees/has space" -b spacey 2>/dev/null
git -C "$repo/.worktrees/has space" commit -q --no-verify --allow-empty -m spacey
git -C "$repo" merge --no-ff --no-commit -q spacey >/dev/null 2>&1 || true
printf 'someone\n' > "$repo/.git/worktrees/has-space/APPLY_IN_PROGRESS"
if ! (cd "$repo" && "$here/../.githooks/pre-merge-commit" >/dev/null 2>&1); then
  ok "pre-merge-commit reaches a worktree whose path has a space in it"
else bad "a worktree path with a space hid its lock from pre-merge-commit"; fi
find "$repo/.git/worktrees/has-space/APPLY_IN_PROGRESS" -mindepth 0 -delete 2>/dev/null
find "$kbin" "$kbar" -mindepth 0 -delete 2>/dev/null
teardown
fi

# A branch that no worktree holds, merged in a repository with no linked worktree at all.
# Nothing in the registry matches MERGE_HEAD, and nothing special-cases that, so nothing
# reintroduces a special case that was never needed.
setup
git -C "$repo" checkout -q -b solo
git -C "$repo" commit -q --no-verify --allow-empty -m solo
git -C "$repo" checkout -q -
git -C "$repo" commit -q --no-verify --allow-empty -m meanwhile
git -C "$repo" merge --no-ff --no-commit -q solo >/dev/null 2>&1 || true
if (cd "$repo" && "$here/../.githooks/pre-merge-commit" >/dev/null 2>&1); then
  ok "pre-merge-commit passes for a branch no worktree holds"
else bad "a branch held by no worktree refused a merge"; fi
teardown

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
# opencode's real shape: one event per line, the answer in the text parts (#286). The
# shape it used to print here was nobody's, so extraction failed and the stage passed
# only because a writing role's unreadable result was ignored; it is refused now (#315).
cat > "$sbin2/opencode" <<'FAKE'
#!/usr/bin/env bash
echo '{"type":"text","sessionID":"s-1","part":{"type":"text","text":"the inherited work is already correct"}}'
echo '{"type":"step_finish","sessionID":"s-1","part":{"type":"step-finish","reason":"stop"}}'
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
if [ "$rc" = "1" ] && printf '%s' "$out" | grep -q "cannot record 'agy' as the implementer"; then
  ok "a run that cannot record who is writing does not run"
else bad "an unrecordable run exited $rc"; fi
find "$rbin" -mindepth 0 -delete 2>/dev/null
teardown
fi

# --- recording the implementer survives a vanished .agent-runs (#296) ---------------
# note_implementer's mkdir and its write are two statements, and during the #235 run the
# directory went away between them: the write failed with "No such file or directory" on
# the line after its own mkdir -p had succeeded, in a worktree whose .agent-runs was there
# before the stage and after it. What stepped into that window is not established, so what
# is asserted here is the window itself and not a cause.
setup
add_change issue-35-vanish
wt35="$repo/.worktrees/issue-35"
mkdir -p "$wt35/.agent-runs"
# The injection point sourcing agents.sh gives: a shell function named mkdir shadows the
# command for the call inside note_implementer, so the directory can be created and then
# taken away again before the write - exactly the window, which no amount of timing
# reproduces. It steps aside after the first call, so the retry meets a normal mkdir.
vanish=1
mkdir() {
  command mkdir "$@" || return 1
  [ "$vanish" = "1" ] || return 0
  vanish=0
  find "$wt35/.agent-runs" -mindepth 0 -delete 2>/dev/null
  return 0
}
note_implementer "$wt35" agy; rc=$?
unset -f mkdir
if [ "$rc" = "0" ] && [ "$(last_implementer "$wt35")" = "agy" ]; then
  ok "a .agent-runs that vanishes under the write is retried rather than fatal"
else bad "a vanished .agent-runs gave exit $rc and the record '$(last_implementer "$wt35")'"; fi

# A write that returns 0 and records nothing, which the same window also produces: unlink
# the directory once the file is open and printf writes into an orphaned inode, exits 0
# and leaves no marker. /dev/null stands in for it, being the one path that accepts every
# byte and keeps none. No retry can help here, so success has to mean the record reads
# back rather than that the write returned 0.
find "$wt35/.agent-runs/implement.last" -mindepth 0 -delete 2>/dev/null
ln -s /dev/null "$wt35/.agent-runs/implement.last"
note_implementer "$wt35" opencode; rc=$?
find "$wt35/.agent-runs/implement.last" -mindepth 0 -delete 2>/dev/null
if [ "$rc" != "0" ]; then ok "a write that lands nowhere is a failure, not a record"
else bad "a write that recorded nothing returned 0"; fi
teardown

# And the stage stops on it, before launching anything. A run whose record did not land is
# the one #292 forbids: an agent editing a tree the marker still attributes to somebody
# else. The unwritable case above proves the refusal; this proves it also fires when the
# write itself reports success.
if [ "$pty_available" = "1" ]; then
setup
add_change issue-36-unlanded
add_passing_review issue-36-unlanded
vbin=$(mktemp -d)
cat > "$vbin/agy" <<'FAKE'
#!/usr/bin/env bash
echo ran >> ran.txt
echo '{"conversation_id":"c","status":"COMPLETED","response":"done"}'
FAKE
chmod +x "$vbin/agy"
mkdir -p "$repo/.worktrees/issue-36/.agent-runs"
ln -s /dev/null "$repo/.worktrees/issue-36/.agent-runs/implement.last"
out=$(cd "$repo" && PATH="$vbin:$PATH" "$STAGE" implement agy issue-36-unlanded 2>&1); rc=$?
find "$repo/.worktrees/issue-36/.agent-runs/implement.last" -mindepth 0 -delete 2>/dev/null
if [ "$rc" = "1" ] && printf '%s' "$out" | grep -q "cannot record 'agy' as the implementer"; then
  ok "a stage whose record does not land refuses to run"
else bad "a stage with an unlanded record exited $rc"; fi
if [ ! -f "$repo/.worktrees/issue-36/ran.txt" ]; then
  ok "and the agent never launched"
else bad "the agent ran without a record of who was writing"; fi
find "$vbin" -mindepth 0 -delete 2>/dev/null
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
printf 'someone else\n' > "$repo/.git/worktrees/issue-7/APPLY_IN_PROGRESS"
out=$(cd "$repo" && PATH="$bin:$PATH" "$STAGE" implement agy issue-7-zeta 2>&1); rc=$?
if [ "$rc" = "1" ] && printf '%s' "$out" | grep -q 'already in progress'; then
  ok "a writing stage refuses to start while another holds the lock"
else bad "the apply lock did not stop a second writing stage (exit $rc)"; fi
find "$repo/.git/worktrees/issue-7/APPLY_IN_PROGRESS" -delete 2>/dev/null
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

# --- what was judged, and who wrote it (#299) --------------------------------------
# Both halves are plumbing facts the driver already had and wrote nowhere: run-stage.sh
# knows the digest of the tree it left and whether the stage changed anything, and apply.sh
# knew neither. What is asserted here is that both now reach the artifact, and that the
# refusal built on the first one fires before a reviewer is launched.
if [ "$pty_available" = "1" ]; then

# A worktree digest is printed by every stage, and it is the only place it can be measured.
setup
add_change issue-30-tree
add_passing_review issue-30-tree
tbin=$(mktemp -d)
cat > "$tbin/agy" <<'FAKE'
#!/usr/bin/env bash
echo worked >> worked.txt
echo '{"conversation_id":"c","status":"COMPLETED","response":"done"}'
FAKE
chmod +x "$tbin/agy"
out=$(cd "$repo" && PATH="$tbin:$PATH" "$STAGE" implement agy issue-30-tree 2>&1); rc=$?
first=$(printf '%s\n' "$out" | grep -E '^tree: [0-9a-f]{64}$' | tail -1 | sed 's/^tree: //')
if [ "$rc" = "0" ] && [ -n "$first" ]; then ok "a stage prints the digest of the tree it left"
else bad "no 'tree: <64 hex>' line from a stage that ran (exit $rc)"; fi
# Same tree, second stage: the measurement is of the tree, not of the run.
cat > "$tbin/codex" <<'FAKE'
#!/usr/bin/env bash
echo '{"type":"thread.started","thread_id":"t-30"}'
echo '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"VERDICT: APPROVE"}}'
FAKE
chmod +x "$tbin/codex"
out=$(cd "$repo" && PATH="$tbin:$PATH" "$STAGE" review codex issue-30-tree 2>&1)
second=$(printf '%s\n' "$out" | grep -E '^tree: [0-9a-f]{64}$' | tail -1 | sed 's/^tree: //')
if [ -n "$second" ] && [ "$first" = "$second" ]; then ok "and two stages over one unchanged tree print the same digest"
else bad "the digest moved across a stage that changed nothing ('$first' then '$second')"; fi
# The change folder is excluded, or apply.sh writing a round artifact between rounds would
# move the digest every round and the refusal below could never fire.
printf 'a round artifact\n' > "$repo/.worktrees/issue-30/openspec/changes/issue-30-tree/diff-review-1.md"
out=$(cd "$repo" && PATH="$tbin:$PATH" "$STAGE" review codex issue-30-tree 2>&1)
third=$(printf '%s\n' "$out" | grep -E '^tree: [0-9a-f]{64}$' | tail -1 | sed 's/^tree: //')
if [ "$first" = "$third" ]; then ok "and writing into openspec/changes does not move it"
else bad "the change folder counts toward the tree digest, so no round can ever match another"; fi
find "$tbin" -mindepth 0 -delete 2>/dev/null
teardown

# apply.sh reads the digest off the stage's stdout, so the stage now runs through a pipe, and
# a pipeline reports the LAST command's status. `set -o pipefail` is what currently saves
# this, and it is one line away in a different file: drop it, or move the wrapper into a
# script that never had it, and every failed implement reads as a clean one while the
# reviewer is handed a tree nobody finished writing. Asserted on the status rather than on
# the spelling, so it holds however the wrapper is written; the digest is asserted above.
setup
add_change issue-34-status
add_passing_review issue-34-status
xbin=$(mktemp -d)
cat > "$xbin/agy" <<'FAKE'
#!/usr/bin/env bash
echo worked >> worked.txt
echo '{"conversation_id":"c","status":"FAILED","response":"ran out of quota"}'
exit 4
FAKE
cat > "$xbin/codex" <<'FAKE'
#!/usr/bin/env bash
echo x >> "$REVIEW_COUNT"
echo '{"type":"thread.started","thread_id":"t-34"}'
echo '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"VERDICT: APPROVE"}}'
FAKE
chmod +x "$xbin/agy" "$xbin/codex"
xcount="$xbin/reviews"; : > "$xcount"
out=$(cd "$repo" && REVIEW_COUNT="$xcount" PATH="$xbin:$PATH" "$APPLY" agy codex issue-34-status --rounds 1 2>&1); rc=$?
if [ "$rc" = "1" ]; then ok "an implement stage that failed still fails apply.sh through the pipe"
else
  bad "a failed implement was read as a clean one (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | tail -4
fi
if [ "$(grep -c . "$xcount")" = "0" ]; then ok "and no reviewer was handed the unfinished tree"
else bad "the reviewer ran after a failed implement"; fi
find "$xbin" -mindepth 0 -delete 2>/dev/null
teardown

# The author ledger. run-stage.sh is the only place that knows both the agent's name and
# whether its stage changed anything; every caller knows one or the other.
setup
add_change issue-31-ledger
add_passing_review issue-31-ledger
led="$repo/.worktrees/issue-31/openspec/changes/issue-31-ledger/authors"
lbin=$(mktemp -d)
cat > "$lbin/agy" <<'FAKE'
#!/usr/bin/env bash
echo worked >> worked.txt
echo '{"conversation_id":"c","status":"COMPLETED","response":"done"}'
FAKE
cat > "$lbin/opencode" <<'FAKE'
#!/usr/bin/env bash
[ -n "${OPENCODE_WRITES:-}" ] && echo more >> worked.txt
echo '{"type":"result","sessionID":"s-31","parts":[{"type":"text","text":"done"}]}'
FAKE
chmod +x "$lbin/agy" "$lbin/opencode"
(cd "$repo" && PATH="$lbin:$PATH" "$STAGE" implement agy issue-31-ledger) >/dev/null 2>&1
if [ "$(cat "$led" 2>/dev/null)" = "agy" ]; then ok "an implement stage that changed the tree records its author"
else bad "the ledger reads '$(cat "$led" 2>/dev/null)', not 'agy'"; fi
# In the change folder, which is committed. .agent-runs/ is gitignored working state these
# scripts create and delete, and a record a passing broom can carry off is not a record.
if [ ! -e "$repo/.worktrees/issue-31/.agent-runs/authors" ]; then ok "and it is not in .agent-runs/"
else bad "the ledger was written to gitignored working state"; fi
(cd "$repo" && PATH="$lbin:$PATH" "$STAGE" implement agy issue-31-ledger --resume) >/dev/null 2>&1
if [ "$(grep -c '^agy$' "$led" 2>/dev/null)" = "1" ]; then ok "and a second round by the same agent names it once"
else bad "the ledger repeated an author"; fi
# A swapped implementer that reads the inherited tree and correctly changes nothing wrote
# none of it. That is #291's shape, and naming it was the misattribution.
(cd "$repo" && PATH="$lbin:$PATH" "$STAGE" implement opencode issue-31-ledger --resume) >/dev/null 2>&1
if ! grep -qx 'opencode' "$led" 2>/dev/null; then ok "an implementer that changed nothing claims no authorship"
else bad "an agent that wrote nothing was recorded as an author"; fi
(cd "$repo" && OPENCODE_WRITES=1 PATH="$lbin:$PATH" "$STAGE" implement opencode issue-31-ledger --resume) >/dev/null 2>&1
if [ "$(tr '\n' ',' < "$led" 2>/dev/null)" = "agy,opencode," ]; then ok "and a swap that does write names both, in the order they wrote"
else bad "the ledger reads '$(tr '\n' ',' < "$led" 2>/dev/null)', not 'agy,opencode,'"; fi
# gate-fix edits src/ after archive has moved the folder, so it must find the ledger there.
mv "$repo/.worktrees/issue-31/openspec/changes/issue-31-ledger" \
   "$repo/.worktrees/issue-31/openspec/changes/archive/2026-01-01-issue-31-ledger"
led2="$repo/.worktrees/issue-31/openspec/changes/archive/2026-01-01-issue-31-ledger/authors"
cat > "$lbin/claude" <<'FAKE'
#!/usr/bin/env bash
echo lintfix >> worked.txt
echo '{"type":"result","subtype":"success","session_id":"cs-31","result":"done"}'
FAKE
chmod +x "$lbin/claude"
(cd "$repo" && PATH="$lbin:$PATH" "$STAGE" gate-fix claude issue-31-ledger --resume) >/dev/null 2>&1
if [ "$(tr '\n' ',' < "$led2" 2>/dev/null)" = "agy,opencode,claude," ]; then ok "a gate fix appends to the ledger the archive moved"
else bad "the archived ledger reads '$(tr '\n' ',' < "$led2" 2>/dev/null)'"; fi
find "$lbin" -mindepth 0 -delete 2>/dev/null
teardown

# The refusal. During #291 two rounds returned opposite verdicts on a byte-identical tree,
# and the second one shipped. run-stage.sh already warned that the fix round changed
# nothing; that warning is what the run flapped past, so this stops instead, and stops
# BEFORE the launch, because the waste is the review.
setup
add_change issue-32-same
add_passing_review issue-32-same
sbin=$(mktemp -d)
cat > "$sbin/agy" <<'FAKE'
#!/usr/bin/env bash
if [ -e .agent-runs/agy-ran ]; then
  echo '{"conversation_id":"c","status":"COMPLETED","response":"every finding is answered in prose"}'
else
  mkdir -p .agent-runs; : > .agent-runs/agy-ran
  echo worked >> worked.txt
  echo '{"conversation_id":"c","status":"COMPLETED","response":"done"}'
fi
FAKE
cat > "$sbin/codex" <<'FAKE'
#!/usr/bin/env bash
echo x >> "$REVIEW_COUNT"
echo '{"type":"thread.started","thread_id":"t-32"}'
echo '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"VERDICT: REVISE"}}'
FAKE
chmod +x "$sbin/agy" "$sbin/codex"
count="$sbin/reviews"; : > "$count"
out=$(cd "$repo" && REVIEW_COUNT="$count" PATH="$sbin:$PATH" "$APPLY" agy codex issue-32-same --rounds 3 2>&1); rc=$?
if [ "$rc" = "10" ]; then ok "a fix round that changed nothing stops the loop"
else
  bad "the same tree went to a second review (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | tail -4
fi
if [ "$(grep -c . "$count")" = "1" ]; then ok "and the second reviewer was never launched"
else bad "the reviewer ran $(grep -c . "$count") times, so the stop came after the launch"; fi
if printf '%s' "$out" | grep -q 'diff-review-1.md' && printf '%s' "$out" | grep -q 'diff-review-2.md'; then
  ok "and it names the round already judged and the one it did not write"
else bad "the refusal named neither round file"; fi
if [ ! -e "$repo/.worktrees/issue-32/openspec/changes/issue-32-same/diff-review-2.md" ]; then
  ok "and wrote no second round artifact"
else bad "a round artifact was written for a review that never ran"; fi
# The comparison reads the digest back from the round file, so a restarted apply.sh - whose
# own round counter begins at 1 again - still sees what the previous invocation judged.
: > "$count"
out=$(cd "$repo" && REVIEW_COUNT="$count" PATH="$sbin:$PATH" "$APPLY" agy codex issue-32-same --rounds 3 2>&1); rc=$?
if [ "$rc" = "10" ]; then ok "and the refusal survives a restart of apply.sh"
else
  bad "a restart reviewed the same tree again (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | tail -4
fi
if [ "$(grep -c . "$count")" = "0" ]; then ok "launching no reviewer at all on the restart"
else bad "the restart launched a reviewer before comparing"; fi
find "$sbin" -mindepth 0 -delete 2>/dev/null
teardown

# What apply.sh writes down, on the path that approves.
setup
add_change issue-33-record
add_passing_review issue-33-record
rbin=$(mktemp -d)
cat > "$rbin/opencode" <<'FAKE'
#!/usr/bin/env bash
echo worked >> worked.txt
echo '{"type":"text","sessionID":"s-33","part":{"type":"text","text":"done"}}'
echo '{"type":"step_finish","sessionID":"s-33","part":{"type":"step-finish","reason":"stop"}}'
FAKE
cat > "$rbin/codex" <<'FAKE'
#!/usr/bin/env bash
echo '{"type":"thread.started","thread_id":"t-33"}'
echo '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"VERDICT: APPROVE"}}'
FAKE
chmod +x "$rbin/opencode" "$rbin/codex"
# The predecessor whose work this run inherits, recorded the way run-stage.sh records it.
printf 'agy\n' > "$repo/.worktrees/issue-33/openspec/changes/issue-33-record/authors"
d="$repo/.worktrees/issue-33/openspec/changes/issue-33-record"
out=$(cd "$repo" && PATH="$rbin:$PATH" "$APPLY" opencode codex issue-33-record --rounds 1 2>&1); rc=$?
if [ "$rc" = "0" ]; then ok "an approving run records its outcome"
else
  bad "the approving run failed (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | tail -4
fi
if grep -qx 'AUTHORS: agy, opencode' "$d/diff-review.md" 2>/dev/null; then
  ok "naming every agent that wrote, not the one this invocation was handed"
else bad "AUTHORS reads '$(grep '^AUTHORS:' "$d/diff-review.md" 2>/dev/null)'"; fi
if ! grep -q '^AUTHOR:' "$d/diff-review.md" 2>/dev/null; then ok "and the singular field is gone"
else bad "diff-review.md still carries an AUTHOR: line"; fi
dtree=$(grep -E '^TREE_SHA256: [0-9a-f]{64}$' "$d/diff-review.md" 2>/dev/null | sed 's/^TREE_SHA256: //')
rtree=$(grep -E '^TREE_SHA256: [0-9a-f]{64}$' "$d/diff-review-1.md" 2>/dev/null | sed 's/^TREE_SHA256: //')
if [ -n "$dtree" ] && [ "$dtree" = "$rtree" ]; then ok "and the tree the approving round judged, on both the round and the verdict"
else bad "diff-review.md says '$dtree' and diff-review-1.md says '$rtree'"; fi
# What apply.sh writes must be what the gate accepts. Asserted end to end rather than by
# reading both scripts: they are edited separately, and a field renamed in one and not the
# other stops the run at the commit, after every agent has already been paid for.
arch="$repo/.worktrees/issue-33/openspec/changes/archive/2026-01-01-issue-33-record"
mkdir -p "$repo/.worktrees/issue-33/openspec/changes/archive"
cp -r "$d" "$arch"
( cd "$repo/.worktrees/issue-33" && "$here/review-gate-check.sh" . \
    openspec/changes/archive/2026-01-01-issue-33-record/diff-review.md src/main.rs ) >/dev/null 2>&1
grc=$?
if [ "$grc" = "0" ]; then ok "and the landing gate accepts what apply.sh wrote"
else
  bad "the landing gate refuses the diff-review.md apply.sh just wrote (exit $grc)"
  ( cd "$repo/.worktrees/issue-33" && "$here/review-gate-check.sh" . \
      openspec/changes/archive/2026-01-01-issue-33-record/diff-review.md src/main.rs ) 2>&1 \
    | sed 's/^/        /' | head -3
fi
find "$rbin" -mindepth 0 -delete 2>/dev/null
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

# --- a stage that could not run says what to do about it, and stops (#297) -----------
# The run died a minute at a time for an hour and reported NO_STRUCTURED_RESULT, which
# names neither the model nor a way forward. What is asserted is the naming and the
# stopping. Nothing asserts a model switch, because nothing switches: the only model that
# gets past a spent allowance bills, and that is not a runner's decision.

# One spelling of the pin. A test repeating the default would pass while agent_command
# used a different one, so the assertion is the relationship and never the string.
default_model=$( unset OPENCODE_MODEL; . "$here/agents.sh"; agent_model opencode )
if [ -n "$default_model" ]; then ok "agent_model names a default opencode model"
else bad "agent_model named no default opencode model"; fi
got=$( unset OPENCODE_MODEL; . "$here/agents.sh"; agent_command opencode implement p )
if [ -n "$default_model" ] && printf '%s' "$got" | grep -qF -- "-m $default_model"; then
  ok "and agent_command launches opencode with exactly that model"
else bad "agent_command used something other than agent_model's '$default_model': $got"; fi
got=$( OPENCODE_MODEL=over/ride; export OPENCODE_MODEL
       . "$here/agents.sh"; agent_command opencode implement p )
if printf '%s' "$got" | grep -qF -- "-m over/ride"; then
  ok "and OPENCODE_MODEL overrides it all the way into the invocation"
else bad "the override did not reach the invocation: $got"; fi

# The tool's own error, which run-stage.sh used to report as its own parser's failure.
ebin=$(mktemp -d)
printf '%s\n' '{"type":"error","timestamp":1,"sessionID":"s-e","error":{"name":"UnknownError","data":{"message":"Unexpected server error. Check server logs for details.","ref":"err_abc123"}}}' > "$ebin/err.json"
printf '%s\n' '{"type":"text","sessionID":"s-e","part":{"type":"text","text":"hello"}}' > "$ebin/ok.json"
emsg=$( . "$here/agents.sh"; agent_error opencode "$ebin/err.json" ); erc=$?
if [ "$erc" = "0" ] && printf '%s' "$emsg" | grep -qF 'UnknownError' \
   && printf '%s' "$emsg" | grep -qF 'err_abc123'; then
  ok "agent_error reads opencode's own error envelope"
else bad "agent_error exited $erc with '$emsg'"; fi
# Paired with the positive, so a function that reports nothing at all cannot pass for one
# that correctly reports nothing.
if [ "$erc" = "0" ] && ! ( . "$here/agents.sh"; agent_error opencode "$ebin/ok.json" ) >/dev/null 2>&1; then
  ok "and reports none where the tool reported none"
else bad "agent_error does not distinguish a capture carrying an error from one that is not"; fi
if [ "$erc" = "0" ] && ! ( . "$here/agents.sh"; agent_error codex "$ebin/err.json" ) >/dev/null 2>&1; then
  ok "and answers for opencode only, which is the one tool with that shape"
else bad "agent_error does not distinguish opencode's envelope from another tool's capture"; fi

# The quota matcher. Only ever wording: if it is wrong the message is less precise, and
# no money moves either way. Captured by running opencode against a provider returning a
# 429 with that body, because whether responseBody survives into --format json is not
# readable off the source.
printf '%s\n' '{"type":"error","timestamp":1,"sessionID":"s-q","error":{"name":"APIError","data":{"message":"Free usage limit exceeded","statusCode":429,"isRetryable":true,"responseBody":"{\"error\": {\"name\": \"FreeUsageLimitError\"}}"}}}' > "$ebin/quota.json"
printf '%s\n' '{"type":"text","part":{"type":"text","text":"opencode throws FreeUsageLimitError when the free tier is gone"}}' > "$ebin/prose.json"
printf '%s\n' '{"type":"error","error":{"name":"APIError","data":{"message":"limit","statusCode":429,"responseBody":"{\"error\":{\"name\":\"GoUsageLimitError\"}}"}}}' > "$ebin/go.json"
qpos=0
( . "$here/agents.sh"; agent_quota_exhausted opencode "$ebin/quota.json" ) 2>/dev/null && qpos=1
if [ "$qpos" = "1" ]; then
  ok "agent_quota_exhausted reads the envelope a spent free allowance produces"
else bad "the real quota envelope was not recognised"; fi
qfired=""
for c in err ok prose go; do
  ( . "$here/agents.sh"; agent_quota_exhausted opencode "$ebin/$c.json" ) 2>/dev/null && qfired="$qfired $c"
done
if [ "$qpos" = "1" ] && [ -z "$qfired" ]; then
  ok "and not on a server error, a clean run, prose naming it, or the paid account's own limit"
else bad "agent_quota_exhausted fired on:$qfired (positive case: $qpos)"; fi

# The two message shapes. Both name the model and offer the same override; only one of
# them may say quota, because claiming it of an unrelated failure sends someone to pay
# for a problem paying does not fix.
qadv=$( unset OPENCODE_MODEL; . "$here/agents.sh"; agent_stall_advice opencode "$ebin/quota.json" )
eadv=$( unset OPENCODE_MODEL; . "$here/agents.sh"; agent_error opencode "$ebin/err.json" >/dev/null
        agent_stall_advice opencode "$ebin/err.json" )
if printf '%s' "$qadv" | grep -qiF 'allowance' && printf '%s' "$qadv" | grep -qF "$default_model"; then
  ok "agent_stall_advice says the free allowance is gone and names the model it was on"
else bad "the quota message said: $qadv"; fi
if printf '%s' "$eadv" | grep -qF 'UnknownError' && ! printf '%s' "$eadv" | grep -qiE 'allowance|quota'; then
  ok "and reports any other failure as what the tool said, claiming no quota"
else bad "the non-quota message said: $eadv"; fi
for want in 'OPENCODE_MODEL=meta/muse-spark-1.2-contributor' 'BILLS'; do
  if printf '%s' "$qadv" | grep -qF "$want" && printf '%s' "$eadv" | grep -qF "$want"; then
    ok "and both messages carry '$want'"
  else bad "'$want' is missing from one of the two messages"; fi
done
# Paired against the same call, because a function that does not exist declines codex too,
# and declining everything is not answering the question.
if ( . "$here/agents.sh"; agent_stall_advice opencode "$ebin/err.json" ) >/dev/null 2>&1 \
   && ! ( . "$here/agents.sh"; agent_stall_advice codex "$ebin/err.json" ) >/dev/null 2>&1; then
  ok "and there is nothing to say for a tool with no model to name"
else bad "agent_stall_advice does not distinguish opencode from a tool it cannot advise on"; fi
find "$ebin" -mindepth 0 -delete 2>/dev/null

# Through run-stage.sh, where a person actually reads it.
if [ "$pty_available" = "1" ]; then
setup
add_change issue-97-stall
add_passing_review issue-97-stall
sbin=$(mktemp -d)
cat > "$sbin/opencode" <<'FAKE'
#!/usr/bin/env bash
echo launched >> "$LAUNCH_SINK"
echo edited >> "$STAGE_MARK"
printf '%s\n' "$STUB_EVENT"
exit 1
FAKE
chmod +x "$sbin/opencode"

quota_event='{"type":"error","timestamp":1,"sessionID":"s-q","error":{"name":"APIError","data":{"message":"Free usage limit exceeded","statusCode":429,"isRetryable":true,"responseBody":"{\"error\": {\"name\": \"FreeUsageLimitError\"}}"}}}'
other_event='{"type":"error","timestamp":1,"sessionID":"s-o","error":{"name":"UnknownError","data":{"message":"Unexpected server error. Check server logs for details.","ref":"err_zz9"}}}'

: > "$sbin/sink"
out=$(cd "$repo" && env -u OPENCODE_MODEL LAUNCH_SINK="$sbin/sink" STAGE_MARK=q.txt \
      PATH="$sbin:$PATH" STUB_EVENT="$quota_event" \
      "$STAGE" implement opencode issue-97-stall 2>&1); rc=$?
if [ "$rc" != "0" ] && [ "$(wc -l < "$sbin/sink")" = "1" ]; then
  ok "a spent allowance stops the stage after one launch, having switched to nothing"
else bad "the quota stage exited $rc after $(wc -l < "$sbin/sink") launches"; fi
if printf '%s' "$out" | grep -qiF 'allowance' \
   && printf '%s' "$out" | grep -qF 'OPENCODE_MODEL=meta/muse-spark-1.2-contributor'; then
  ok "and the stage output says so and gives the line that would move off it"
else bad "the stage said nothing usable: $(printf '%s' "$out" | tail -12)"; fi

: > "$sbin/sink"
out=$(cd "$repo" && env -u OPENCODE_MODEL LAUNCH_SINK="$sbin/sink" STAGE_MARK=o.txt \
      PATH="$sbin:$PATH" STUB_EVENT="$other_event" \
      "$STAGE" implement opencode issue-97-stall 2>&1); rc=$?
# The guard that makes the assertions above mean anything: an unrelated failure must
# reach the same stop with a different sentence, never a claim about quota.
if [ "$rc" != "0" ] && [ "$(wc -l < "$sbin/sink")" = "1" ] \
   && ! printf '%s' "$out" | grep -qiE 'allowance|out of quota'; then
  ok "an unrelated failure stops the same way without claiming an allowance ran out"
else bad "the non-quota stage exited $rc after $(wc -l < "$sbin/sink") launches: $(printf '%s' "$out" | tail -12)"; fi
if printf '%s' "$out" | grep -q 'status: AGENT_ERROR' && printf '%s' "$out" | grep -qF 'err_zz9'; then
  ok "and is still reported as the error the tool itself gave"
else bad "the error envelope was not named: $(printf '%s' "$out" | grep '^role:')"; fi
find "$sbin" -mindepth 0 -delete 2>/dev/null
teardown
fi

# --- a failure the base already has is not this change's (#298) ---------------------
# The driver read every non-zero from the gates as the change's fault, so on a machine
# whose suite does not pass at HEAD it could never finish. During #235 that threw away a
# reviewed, approved, archived change over 16 failures that fail identically at the base.
#
# No real cargo suite runs here: the parse is asserted against a canned log, and every
# decision that would launch one is driven through a stand-in cargo that answers from files
# the test writes. What that stand-in also proves is where the baseline ran - it records its
# own working directory - because "in a worktree that is not the one being written" is the
# part a passing exit status would never show.
source "$here/gates.sh"

# The parse, keyed by target as well as by name. A test name is unique only within one
# binary; merged on the name alone, the new failure in tests/http_tests.rs below would be
# cancelled by the passing run of the same name under unittests.
plog=$(mktemp)
cat > "$plog" <<'LOG'
   Compiling labeler v0.1.0 (/repo)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 41.02s
     Running unittests src/lib.rs (target/debug/deps/labeler-1111111111111111)

running 3 tests
test render::tests::flips_y ... ok
test fs_safe::tests::opens_a_path ... FAILED
test templates::tests::load_from_dir_handles_non_utf8_paths ... FAILED

failures:
    fs_safe::tests::opens_a_path

test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests/http_tests.rs (target/debug/deps/http_tests-2222222222222222)

running 2 tests
test fs_safe::tests::opens_a_path ... ok
test template_group_renders ... FAILED

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
   Doc-tests labeler

running 1 test
test src/lib.rs - render (line 12) ... FAILED
LOG
want='Doc-tests labeler :: src/lib.rs - render (line 12)
tests/http_tests.rs :: template_group_renders
unittests src/lib.rs :: fs_safe::tests::opens_a_path
unittests src/lib.rs :: templates::tests::load_from_dir_handles_non_utf8_paths'
got=$(gate_failed_tests "$plog")
if [ "$got" = "$want" ]; then ok "every failing test is read off a cargo log under its own target"
else bad "gate_failed_tests read '$(printf '%s' "$got" | tr '\n' '|')'"; fi
printf 'running 1 test\ntest a::b ... ok\n\ntest result: ok. 1 passed; 0 failed\n' > "$plog"
if [ -z "$(gate_failed_tests "$plog")" ]; then ok "and a suite that passed names nothing"
else bad "a passing log produced failures"; fi
find "$plog" -mindepth 0 -delete 2>/dev/null

# A cargo that answers from files the test controls, and says where it was run.
mk_cargo() { # mk_cargo <bindir>
  cat > "$1/cargo" <<'FAKE'
#!/usr/bin/env bash
case "$1" in fmt|clippy) exit 0 ;; esac
printf 'cwd=%s target=%s\n' "$PWD" "${CARGO_TARGET_DIR:-unset}" >> "$CARGO_CALLS"
cat "$CARGO_OUT"
exit "${CARGO_RC:-101}"
FAKE
  chmod +x "$1/cargo"
}
canned() { # canned <file> <test-name>...
  local f="$1"; shift
  printf '     Running unittests src/lib.rs (target/debug/deps/labeler-aaaa)\n\n' > "$f"
  printf 'running %s tests\n' "$#" >> "$f"
  for t in "$@"; do printf 'test %s ... FAILED\n' "$t" >> "$f"; done
  printf '\ntest result: FAILED. 0 passed; %s failed\n' "$#" >> "$f"
}

setup
add_change issue-50-attr
wt50="$repo/.worktrees/issue-50"
mkdir -p "$wt50/.agent-runs"
glog="$wt50/.agent-runs/gates.log"
blog="$wt50/.agent-runs/gates-base.log"
abin=$(mktemp -d); mk_cargo "$abin"
export CARGO_CALLS="$abin/calls" CARGO_OUT="$abin/base.log" CARGO_RC=101
# A target directory in the environment, which the baseline must not take: that is how it
# would end up sharing one with the tree it is measuring.
export CARGO_TARGET_DIR="$abin/shared-target"
: > "$CARGO_CALLS"
canned "$CARGO_OUT" fs_safe::tests::opens_a_path templates::tests::non_utf8

# The base commit is the fork point, never HEAD: on a re-run after the commit, HEAD carries
# the change and comparing against it would subtract the change from itself. Nor is it the
# tip of the default branch, which moves on while a change is in flight. Nor is it reachable
# by counting parents: the fixture puts TWO commits on the change branch and ONE on the
# default branch, so the fork point is HEAD~2 here and HEAD~1 nowhere, and the three wrong
# answers - echo HEAD, the default tip, HEAD~1 - are all distinct commits from it and from
# each other. Depth matters as much as divergence: against a single-commit repo all four
# coincide, and against one commit a side the parent-arithmetic answer is right by accident.
base50=$(git -C "$repo" rev-parse HEAD)
for c in 1 2; do
  printf 'the change, part %s\n' "$c" > "$wt50/change-$c.txt"
  git -C "$wt50" add "change-$c.txt" >/dev/null 2>&1
  git -C "$wt50" commit -q -m "the change's own commit $c" >/dev/null 2>&1
done
printf 'moved on\n' > "$repo/main-moved.txt"
git -C "$repo" add main-moved.txt >/dev/null 2>&1
git -C "$repo" commit -q -m "the default branch moves on" >/dev/null 2>&1
got50=$(gate_base_commit "$wt50")
if [ "$got50" = "$base50" ] \
   && [ "$got50" != "$(git -C "$wt50" rev-parse HEAD)" ] \
   && [ "$got50" != "$(git -C "$wt50" rev-parse HEAD~1)" ] \
   && [ "$got50" != "$(git -C "$repo" rev-parse HEAD)" ]; then
  ok "the baseline commit is the fork point, not the branch tip, its parent, or the default one"
else bad "gate_base_commit gave '$got50', not the fork point $base50 (branch tip $(git -C "$wt50" rev-parse HEAD), its parent $(git -C "$wt50" rev-parse HEAD~1), default tip $(git -C "$repo" rev-parse HEAD))"; fi

# fmt and clippy are deterministic, and a pre-existing lint is not a thing this repo
# tolerates: they fail outright, and no baseline is run for them at all.
canned "$glog" fs_safe::tests::opens_a_path
for pair in "$GATE_FMT_FAILED:fmt" "$GATE_CLIPPY_FAILED:clippy"; do
  out=$(cd "$repo" && PATH="$abin:$PATH" gate_attribute "$wt50" "${pair%%:*}" "$glog" "$blog" 2>&1); rc=$?
  if [ "$rc" = "1" ] && printf '%s' "$out" | grep -qi "${pair#*:}"; then
    ok "a failing ${pair#*:} is this change's, whatever the base does"
  else bad "a failing ${pair#*:} gave exit $rc: $out"; fi
done
if [ ! -s "$CARGO_CALLS" ]; then ok "and no baseline suite was run for either"
else bad "a lint failure ran the baseline suite $(grep -c . "$CARGO_CALLS") time(s)"; fi

# A build error names no test, so there is nothing to match against the base. That is the
# refusal, not a pass: a driver that waved a change through on an attribution it could not
# make would be worse than the false stop this replaces.
printf 'error[E0432]: unresolved import `crate::nope`\nerror: could not compile `labeler`\n' > "$glog"
out=$(cd "$repo" && PATH="$abin:$PATH" gate_attribute "$wt50" "$GATE_TEST_FAILED" "$glog" "$blog" 2>&1); rc=$?
if [ "$rc" = "1" ] && printf '%s' "$out" | grep -q 'without naming a single failing test'; then
  ok "a test gate that named no test is this change's, and says why"
else bad "an unattributable test failure gave exit $rc: $out"; fi
if [ ! -s "$CARGO_CALLS" ]; then ok "and it did not pay for a baseline it could not use"
else bad "the baseline ran with nothing to compare against"; fi

# A result line the parse does not recognise is the other way to end up with an incomplete
# set, and the dangerous one: a failure dropped here is subtracted away as pre-existing and
# the change ships broken. So cargo's own count is checked against what was read, and a
# disagreement stops the run rather than being compared anyway.
{ printf '     Running unittests src/lib.rs (target/debug/deps/labeler-aaaa)\n\n'
  printf 'running 2 tests\n'
  printf 'test render::tests::flips_y ... FAILED\n'
  printf 'test fs_safe::tests::opens_a_path ... FAILED (time limit exceeded)\n'
  printf '\ntest result: FAILED. 0 passed; 2 failed\n'; } > "$glog"
out=$(cd "$repo" && PATH="$abin:$PATH" gate_attribute "$wt50" "$GATE_TEST_FAILED" "$glog" "$blog" 2>&1); rc=$?
if [ "$rc" = "1" ] && printf '%s' "$out" | grep -q 'counted 2 failing test(s) here and this read 1'; then
  ok "a failure cargo counted but this could not read stops the run, both numbers named"
else bad "an unreadable result line gave exit $rc: $out"; fi
if [ ! -s "$CARGO_CALLS" ]; then ok "and no baseline is run against a set that cannot be trusted"
else bad "the baseline ran on an incomplete failure set"; fi

# The other way to hold an incomplete set, and the one the counts cannot see: a test binary
# that dies mid-run prints its banner and never prints a result, so what it would have
# failed on is missing while every failure cargo did count is read. Measured on this repo,
# not imagined - an abort() in one test file under --no-fail-fast gave 8 banners against 7
# summaries, 2 failures counted and 2 read - and without this the subtraction would have
# excused a change that crashed the harness.
{ printf '     Running tests/zz_broken.rs (target/debug/deps/zz_broken-aaaa)\n\n'
  printf 'running 1 test\n'
  printf '     Running tests/zz_failing.rs (target/debug/deps/zz_failing-bbbb)\n\n'
  printf 'running 2 tests\n'
  printf 'test zz_first ... FAILED\ntest zz_second ... FAILED\n'
  printf '\ntest result: FAILED. 0 passed; 2 failed\n'; } > "$glog"
out=$(cd "$repo" && PATH="$abin:$PATH" gate_attribute "$wt50" "$GATE_TEST_FAILED" "$glog" "$blog" 2>&1); rc=$?
if [ "$rc" = "1" ] && printf '%s' "$out" | grep -q 'started 2 test target(s) here and 1 reported'; then
  ok "a test binary that died mid-run stops the run, however well the counts add up"
else bad "a target that never reported gave exit $rc: $out"; fi
if [ ! -s "$CARGO_CALLS" ]; then ok "and no baseline is run against a set missing a whole binary"
else bad "the baseline ran against a set missing a target"; fi

# The first direction: everything here fails there too.
before_trees=$(git -C "$repo" worktree list | grep -c .)
before_status=$(git -C "$wt50" status --porcelain)
canned "$glog" fs_safe::tests::opens_a_path templates::tests::non_utf8
out=$(cd "$repo" && PATH="$abin:$PATH" gate_attribute "$wt50" "$GATE_TEST_FAILED" "$glog" "$blog" 2>&1); rc=$?
if [ "$rc" = "0" ] && printf '%s' "$out" | grep -q '2 failure(s) fail identically'; then
  ok "failures the base has too predate the change, and are counted and named"
else bad "a wholly pre-existing failure set gave exit $rc: $out"; fi
if [ "$(grep -c . "$CARGO_CALLS")" = "1" ]; then ok "at the cost of one extra suite"
else bad "the baseline ran $(grep -c . "$CARGO_CALLS") time(s)"; fi
# Where it ran matters as much as that it ran: the change is uncommitted working state, so
# checking the base out over it would destroy the work being judged.
if ! grep -q 'cwd=[^ ]*\.worktrees/issue-50' "$CARGO_CALLS"; then
  ok "in a worktree that is not the one being written"
else bad "the baseline suite ran inside the change's own worktree"; fi
# And in a target directory of its own. Cargo does not key this package's artifacts on the
# tree they were built in, so a shared one lets each tree run the other's binaries: the
# baseline would measure the change's compiled code, and the change's own re-run would be
# stopped by a binary built in a scratch tree that has since been deleted. Both were
# observed while this was written; the second failed as `read SPEC.md: NotFound` in a test
# that reads through env!("CARGO_MANIFEST_DIR") (src/errors.rs:653).
if grep -q "target=$wt50/target/baseline" "$CARGO_CALLS"; then
  ok "and in a target directory that is not the change's, nor the environment's"
else bad "the baseline built into $(sed -n 's/.*target=//p' "$CARGO_CALLS" | tail -1)"; fi
if [ "$(git -C "$wt50" status --porcelain)" = "$before_status" ] \
   && [ "$(git -C "$repo" worktree list | grep -c .)" = "$before_trees" ]; then
  ok "leaving the change's tree exactly as it was and no scratch worktree behind"
else bad "the baseline moved the change's tree or left $(git -C "$repo" worktree list | grep -c .) worktree(s)"; fi

# The same base commit, so the fix round's re-run reuses the measurement rather than paying
# for it again. Announced, because a cache nobody can see is a cache nobody can distrust.
out=$(cd "$repo" && PATH="$abin:$PATH" gate_attribute "$wt50" "$GATE_TEST_FAILED" "$glog" "$blog" 2>&1); rc=$?
if [ "$rc" = "0" ] && [ "$(grep -c . "$CARGO_CALLS")" = "1" ] && printf '%s' "$out" | grep -q 'reusing'; then
  ok "a second attempt at the same base reuses the baseline and says so"
else bad "the second attempt ran $(grep -c . "$CARGO_CALLS") suite(s), exit $rc"; fi

# The other direction, against that same cached baseline: one failure the base does not
# have, and the whole set is not excused by the two that it does.
canned "$glog" fs_safe::tests::opens_a_path templates::tests::non_utf8 render::tests::flips_y
out=$(cd "$repo" && PATH="$abin:$PATH" gate_attribute "$wt50" "$GATE_TEST_FAILED" "$glog" "$blog" 2>&1); rc=$?
if [ "$rc" = "1" ] && printf '%s' "$out" | grep -q 'render::tests::flips_y'; then
  ok "a failure the base does not have is this change's, and is named"
else bad "a new failure among pre-existing ones gave exit $rc: $out"; fi
if ! printf '%s' "$out" | grep -q 'fs_safe::tests::opens_a_path'; then
  ok "and the ones that predate it are not put on the implementer"
else bad "the report blamed the change for a pre-existing failure"; fi

# A baseline that will not build reports nothing to subtract, and cannot be read as "the
# base is clean, so every failure here is new" either.
find "$blog.commit" -mindepth 0 -delete 2>/dev/null
printf 'error: could not compile `labeler` (lib test)\n' > "$CARGO_OUT"
out=$(cd "$repo" && PATH="$abin:$PATH" gate_attribute "$wt50" "$GATE_TEST_FAILED" "$glog" "$blog" 2>&1); rc=$?
if [ "$rc" = "1" ] && printf '%s' "$out" | grep -q 'without naming a failing test'; then
  ok "a baseline that would not build attributes nothing, and stops"
else bad "an unreadable baseline gave exit $rc: $out"; fi

# The same count on the base's side, where a dropped failure blames this change for one it
# did not cause. Not compared, and not cached either: a measurement that cannot be read
# whole must not be reused by the fix round's re-run.
canned "$glog" fs_safe::tests::opens_a_path
{ printf '     Running unittests src/lib.rs (target/debug/deps/labeler-aaaa)\n\n'
  printf 'running 2 tests\n'
  printf 'test fs_safe::tests::opens_a_path ... FAILED\n'
  printf 'test templates::tests::non_utf8 ... FAILED (time limit exceeded)\n'
  printf '\ntest result: FAILED. 0 passed; 2 failed\n'; } > "$CARGO_OUT"
out=$(cd "$repo" && PATH="$abin:$PATH" gate_attribute "$wt50" "$GATE_TEST_FAILED" "$glog" "$blog" 2>&1); rc=$?
if [ "$rc" = "1" ] && printf '%s' "$out" | grep -q 'counted 2 failing test(s) at '; then
  ok "a baseline whose failures cannot all be read is not subtracted from"
else bad "an incompletely read baseline gave exit $rc: $out"; fi
if [ ! -f "$blog.commit" ]; then ok "and is not cached as if it had measured something"
else bad "an incomplete baseline was cached for the next attempt"; fi

# And a baseline whose own harness died. Its missing failures would be read as passing
# there, which is what turns a failure this change did not cause into one it gets blamed
# for - or, when the change's set matches what is left, lets a real one through.
canned "$glog" fs_safe::tests::opens_a_path
{ printf '     Running tests/zz_broken.rs (target/debug/deps/zz_broken-aaaa)\n\n'
  printf 'running 1 test\n'
  printf '     Running unittests src/lib.rs (target/debug/deps/labeler-aaaa)\n\n'
  printf 'running 1 test\ntest fs_safe::tests::opens_a_path ... FAILED\n'
  printf '\ntest result: FAILED. 0 passed; 1 failed\n'; } > "$CARGO_OUT"
out=$(cd "$repo" && PATH="$abin:$PATH" gate_attribute "$wt50" "$GATE_TEST_FAILED" "$glog" "$blog" 2>&1); rc=$?
if [ "$rc" = "1" ] && printf '%s' "$out" | grep -q 'started 2 test target(s) at '; then
  ok "a baseline whose harness died measures nothing, even where the failures line up"
else bad "a baseline missing a target gave exit $rc: $out"; fi
if [ ! -f "$blog.commit" ]; then ok "and that baseline is not cached either"
else bad "a baseline missing a target was cached for the next attempt"; fi

# A commit that is not there: the scratch worktree cannot be made, which is not the same as
# a suite that ran and failed, and must not be read as one.
gate_tests_at "$wt50" 0000000000000000000000000000000000000000 "$blog"; rc=$?
if [ "$rc" = "125" ] && [ "$(git -C "$repo" worktree list | grep -c .)" = "$before_trees" ]; then
  ok "a baseline that could not be checked out is a run that never happened"
else bad "a bad commit gave $rc and left $(git -C "$repo" worktree list | grep -c .) worktree(s)"; fi
find "$abin" -mindepth 0 -delete 2>/dev/null
unset CARGO_CALLS CARGO_OUT CARGO_RC CARGO_TARGET_DIR
teardown

# No default branch anywhere, so there is no fork point and no baseline. Nothing is
# guessed from it: without a base every failure is the change's.
setup
def=$(git -C "$repo" symbolic-ref --short HEAD)
add_change issue-52-nobase
git -C "$repo" branch -m "$def" sideline
wt52="$repo/.worktrees/issue-52"
mkdir -p "$wt52/.agent-runs"
if ! gate_base_commit "$wt52" >/dev/null 2>&1; then ok "no default branch means no baseline commit"
else bad "gate_base_commit invented a base of $(gate_base_commit "$wt52")"; fi
canned "$wt52/.agent-runs/gates.log" fs_safe::tests::opens_a_path
out=$(gate_attribute "$wt52" "$GATE_TEST_FAILED" "$wt52/.agent-runs/gates.log" "$wt52/.agent-runs/gates-base.log" 2>&1); rc=$?
if [ "$rc" = "1" ] && printf '%s' "$out" | grep -q 'no baseline'; then
  ok "and every failure counts as this change's, said out loud"
else bad "a missing baseline gave exit $rc: $out"; fi
teardown

# --- and the driver acts on it (#298) -----------------------------------------------
# The gates stage end to end, which is reachable without stubbing an agent: an archived
# change sends next_stage straight to the gates. The stand-in cargo answers differently in
# the two trees, keyed on the path it was run in.
drive_gates() { # drive_gates <issue> <change-failures-file> <base-failures-file> -> the run's output
  local n="$1" cf="$2" bf="$3" bin out
  bin=$(mktemp -d)
  cat > "$bin/cargo" <<FAKE
#!/usr/bin/env bash
# fmt is recorded but not counted with the suites: what matters about it is the flag it
# carried, not how many trees it ran in.
case "\$1" in
  fmt) printf '%s\n' "\$*" >> "$FMT_SINK"; exit 0 ;;
  clippy) exit 0 ;;
esac
printf '%s\n' "\$*" >> "$ARGS_SINK"
case "\$PWD" in
  *.worktrees/issue-$n*) cat "$cf" ;;
  *) cat "$bf" ;;
esac
exit 101
FAKE
  # The issue body is read from the cached scope file below; a gh that answers would reach
  # for a network this suite must not need. The implementer fails its fix round rather than
  # being stubbed into fixing anything: what is read here is the attribution before it.
  printf '#!/usr/bin/env bash\nexit 1\n' > "$bin/gh"
  printf '#!/usr/bin/env bash\nexit 1\n' > "$bin/agy"
  chmod +x "$bin/cargo" "$bin/gh" "$bin/agy"
  out=$(cd "$repo" && PATH="$bin:$PATH" "$RUN" "$n" claude codex agy codex 2>&1)
  printf '%s\n' "$out"
  find "$bin" -mindepth 0 -delete 2>/dev/null
}
stage_gates_repo() { # stage_gates_repo <issue> <slug>
  local n="$1" name="issue-$1-$2" w="$repo/.worktrees/issue-$1"
  git -C "$repo" worktree add -q "$w" -b "$name" 2>/dev/null
  mkdir -p "$w/openspec/changes/archive/2026-01-01-$name" "$w/.agent-runs"
  printf '# Proposal\n' > "$w/openspec/changes/archive/2026-01-01-$name/proposal.md"
  printf '# Issue %s\n\nthe scope\n' "$n" > "$w/.agent-runs/issue-$n.md"
  # Committed, so the gates stage is followed by the push rather than by a commit stage
  # that would need an agent. What the run does at the gates is what is being read here.
  git -C "$w" add -A >/dev/null 2>&1
  git -C "$w" commit -q --no-verify -m "the change" >/dev/null 2>&1
}

setup
stage_gates_repo 53 preexisting
cfail=$(mktemp); bfail=$(mktemp)
export ARGS_SINK=$(mktemp) FMT_SINK=$(mktemp)
canned "$cfail" fs_safe::tests::opens_a_path templates::tests::non_utf8
cp "$cfail" "$bfail"
out=$(drive_gates 53 "$cfail" "$bfail")
# Both suites, or the two failure sets are not comparable: cargo stops after the first
# failing test binary, so a regression in a later one would sit behind a pre-existing
# failure in an earlier one and be subtracted away unseen.
if [ "$(grep -c . "$ARGS_SINK")" = "2" ] && [ "$(grep -c -- '--no-fail-fast' "$ARGS_SINK")" = "2" ]; then
  ok "both the change's suite and the base's run every test binary"
else bad "the suites ran as: $(tr '\n' '|' < "$ARGS_SINK")"; fi
# Check mode, every time it runs. The gates fire after the diff review has approved the
# tree, so a fmt that rewrites lands bytes nobody reviewed (#326); a revert to plain
# `cargo fmt` is invisible to every other assertion here.
fmtn=$(grep -c . "$FMT_SINK")
if [ "$fmtn" -ge 1 ] && [ "$(grep -c -- '--check' "$FMT_SINK")" = "$fmtn" ]; then
  ok "the fmt gate reports rather than rewriting the tree it was handed"
else bad "fmt ran as: $(tr '\n' '|' < "$FMT_SINK")"; fi
if printf '%s' "$out" | grep -q 'predate this change'; then
  ok "the driver names the failures it is not stopping for"
else bad "the driver said nothing about a wholly pre-existing failure set"; fi
if printf '%s' "$out" | grep -q '^== push' && ! printf '%s' "$out" | grep -q 'gets one round'; then
  ok "and goes on to the push without spending the fix round"
else bad "a change whose failures all predate it did not get past the gates"; fi
teardown

setup
stage_gates_repo 54 regression
canned "$cfail" fs_safe::tests::opens_a_path render::tests::flips_y
canned "$bfail" fs_safe::tests::opens_a_path
out=$(drive_gates 54 "$cfail" "$bfail")
if printf '%s' "$out" | grep -q 'render::tests::flips_y' && printf '%s' "$out" | grep -q 'do not fail at'; then
  ok "a test this change broke still stops the driver, by name"
else bad "a regression alongside a pre-existing failure was not named"; fi
if ! printf '%s' "$out" | grep -q '^== push'; then ok "and nothing is pushed"
else bad "a change that broke a test reached the push"; fi
find "$cfail" "$bfail" "$ARGS_SINK" "$FMT_SINK" -mindepth 0 -delete 2>/dev/null
unset ARGS_SINK FMT_SINK
teardown

# --- a change whose deliverable is the delta spec (#313) ---------------------------
# run-change.sh could not drive one. implement carries produces=1, so a stage that wrote
# no code because its plan asked for none exited 3 and the run stopped reporting that it
# never ran. Two facts tell those apart, and both are artifacts: the plan's own
# `DELIVERABLE: spec-only` line, read before the agent launches, and the stage still
# having had to leave something in the change folder.

# This section gates in two places on purpose. What follows is refused before any agent is
# launched, so it needs no pty and runs on a shell that cannot allocate one; everything
# under the pty_available guard below launches a stand-in agent through script(1) and
# cannot. Moving these inside that guard would skip the refusals on exactly the shells
# where nothing else here runs either.
#
# The declaration has one legal value. Anything else is a plan saying something this
# tooling cannot act on, and reading it as absent is the guess.
setup
add_change issue-40-deliverable
add_passing_review issue-40-deliverable
dbin=$(mktemp -d)
cat > "$dbin/agy" <<'FAKE'
#!/usr/bin/env bash
echo launched >> launched.txt
echo '{"conversation_id":"c","status":"COMPLETED","response":"done"}'
FAKE
chmod +x "$dbin/agy"
dd40="$repo/.worktrees/issue-40/openspec/changes/issue-40-deliverable"
printf '# Proposal\n\nDELIVERABLE: whatever\n' > "$dd40/proposal.md"
out=$(cd "$repo" && PATH="$dbin:$PATH" "$STAGE" implement agy issue-40-deliverable 2>&1); rc=$?
if [ "$rc" = "2" ] && printf '%s' "$out" | grep -q "not a deliverable this loop knows"; then
  ok "a deliverable the loop does not know refuses the stage"
else
  bad "an unknown DELIVERABLE value gave exit $rc"
  printf '%s\n' "$out" | sed 's/^/        /' | head -4
fi
printf '# Proposal\n\nDELIVERABLE: spec-only\nDELIVERABLE: spec-only\n' > "$dd40/proposal.md"
out=$(cd "$repo" && PATH="$dbin:$PATH" "$STAGE" implement agy issue-40-deliverable 2>&1); rc=$?
if [ "$rc" = "2" ] && printf '%s' "$out" | grep -q "which one is the plan is a guess"; then
  ok "and two of them are a guess, not a declaration"
else bad "a doubled DELIVERABLE line gave exit $rc"; fi
# Trimmed at the ends, never through the middle. Deleting every space would read this as
# the legal value and accept it, which is the reader repairing a malformed declaration
# instead of refusing it, in the one place a planner types the field by hand.
printf '# Proposal\n\nDELIVERABLE: spec - only\n' > "$dd40/proposal.md"
out=$(cd "$repo" && PATH="$dbin:$PATH" "$STAGE" implement agy issue-40-deliverable 2>&1); rc=$?
if [ "$rc" = "2" ] && printf '%s' "$out" | grep -q "spec - only"; then
  ok "and a value spelled with spaces inside it is not repaired into the legal one"
else bad "'DELIVERABLE: spec - only' gave exit $rc"; fi
if [ ! -f "$repo/.worktrees/issue-40/launched.txt" ]; then
  ok "with no agent launched on any of them"
else bad "an agent ran on a plan the loop had already refused"; fi
find "$dbin" -mindepth 0 -delete 2>/dev/null
teardown

if [ "$pty_available" = "1" ]; then
setup
add_change issue-41-specdelta
add_passing_review issue-41-specdelta
sbin=$(mktemp -d)
d41="$repo/.worktrees/issue-41/openspec/changes/issue-41-specdelta"
# Padded on purpose. Whitespace around the value is not part of it, and this is the fixture
# that carries a legal declaration through a stage that actually runs.
printf '# Proposal\n\nDELIVERABLE:   spec-only  \n' > "$d41/proposal.md"
# What such an implement stage legitimately does: it verifies, ticks its boxes, and writes
# no code. openspec/changes is excluded from implement's work digest, so this is exactly
# the shape the guard read as a stage that had not run.
cat > "$sbin/agy" <<'FAKE'
#!/usr/bin/env bash
printf -- '- [x] 1.1 confirmed against src/convert.rs\n' \
  > openspec/changes/issue-41-specdelta/tasks.md
echo '{"conversation_id":"c","status":"COMPLETED","response":"nothing to write"}'
FAKE
chmod +x "$sbin/agy"
out=$(cd "$repo" && PATH="$sbin:$PATH" "$STAGE" implement agy issue-41-specdelta 2>&1); rc=$?
if [ "$rc" = "0" ]; then ok "an implement stage that writes no code passes on a spec-only plan"
else
  bad "a spec-only change still could not get past implement (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | tail -4
fi
if printf '%s' "$out" | grep -q 'DELIVERABLE: spec-only'; then
  ok "saying why, rather than passing quietly"
else bad "nothing in the output says why an empty implement was accepted"; fi
# The exemption is not from being measured, only from being measured by the code written.
# A stage that touched nothing anywhere did not run, spec-only plan or not.
cat > "$sbin/agy" <<'FAKE'
#!/usr/bin/env bash
echo '{"conversation_id":"c","status":"COMPLETED","response":"did nothing"}'
FAKE
chmod +x "$sbin/agy"
out=$(cd "$repo" && PATH="$sbin:$PATH" "$STAGE" implement agy issue-41-specdelta 2>&1); rc=$?
if [ "$rc" = "3" ]; then ok "and one that touched nothing at all is still refused"
else bad "a silent implementer passed on a spec-only change (exit $rc)"; fi
# The declaration is read before the launch, so the stage it would exempt cannot write it.
# openspec/changes is outside implement's work digest, so writing it costs nothing.
add_change issue-42-selfdeclared
add_passing_review issue-42-selfdeclared
cat > "$sbin/agy" <<'FAKE'
#!/usr/bin/env bash
printf 'DELIVERABLE: spec-only\n' >> openspec/changes/issue-42-selfdeclared/proposal.md
echo '{"conversation_id":"c","status":"COMPLETED","response":"declared myself done"}'
FAKE
chmod +x "$sbin/agy"
out=$(cd "$repo" && PATH="$sbin:$PATH" "$STAGE" implement agy issue-42-selfdeclared 2>&1); rc=$?
if [ "$rc" = "3" ]; then ok "an implement stage cannot declare its own change spec-only"
else bad "a stage exempted itself by writing the declaration (exit $rc)"; fi
find "$sbin" -mindepth 0 -delete 2>/dev/null
teardown

# The author ledger. Nothing else can claim a spec-only change: implement writes no code,
# and an empty AUTHORS: is what the landing gate refuses, so the field was written by hand.
setup
git worktree add -q .worktrees/issue-43 -b issue-43-ledger 2>/dev/null
pbin=$(mktemp -d)
cat > "$pbin/claude" <<'FAKE'
#!/usr/bin/env bash
mkdir -p "openspec/changes/$CHANGE/specs/thing"
printf '# Proposal\n\n%s\n' "${DECLARE:-}" > "openspec/changes/$CHANGE/proposal.md"
printf '## MODIFIED Requirements\n' > "openspec/changes/$CHANGE/specs/thing/spec.md"
echo '{"type":"result","subtype":"success","result":"proposed","session_id":"sess-43"}'
FAKE
chmod +x "$pbin/claude"
led43="$repo/.worktrees/issue-43/openspec/changes/issue-43-ledger/authors"
(cd "$repo" && CHANGE=issue-43-ledger DECLARE='DELIVERABLE: spec-only' \
   PATH="$pbin:$PATH" "$STAGE" propose claude issue-43-ledger) >/dev/null 2>&1
if [ "$(cat "$led43" 2>/dev/null)" = "claude" ]; then
  ok "the propose stage of a spec-only change is its author"
else bad "the ledger reads '$(cat "$led43" 2>/dev/null)', not 'claude'"; fi
# And only there. On every other change the code is what lands, propose wrote none of it,
# and naming the planner would refuse a code reviewer that had written nothing.
git worktree add -q .worktrees/issue-44 -b issue-44-normal 2>/dev/null
led44="$repo/.worktrees/issue-44/openspec/changes/issue-44-normal/authors"
(cd "$repo" && CHANGE=issue-44-normal DECLARE='' \
   PATH="$pbin:$PATH" "$STAGE" propose claude issue-44-normal) >/dev/null 2>&1
if [ ! -e "$led44" ]; then ok "and a plan that delivers code claims no authorship of it"
else bad "propose claimed authorship on a change that delivers code"; fi
find "$pbin" -mindepth 0 -delete 2>/dev/null
teardown

# The pairing that becomes unusable. With the planner as the only author, a code reviewer
# named at launch can turn out to be that author; the landing gate would say so at the
# commit, after every agent has run.
setup
add_change issue-45-pairing
add_passing_review issue-45-pairing
abin=$(mktemp -d)
cat > "$abin/agy" <<'FAKE'
#!/usr/bin/env bash
echo launched >> launched.txt
echo '{"conversation_id":"c","status":"COMPLETED","response":"done"}'
FAKE
cat > "$abin/claude" <<'FAKE'
#!/usr/bin/env bash
echo launched >> launched.txt
echo '{"type":"result","subtype":"success","result":"done","session_id":"s-45"}'
FAKE
chmod +x "$abin/agy" "$abin/claude"
printf 'claude\n' > "$repo/.worktrees/issue-45/openspec/changes/issue-45-pairing/authors"
out=$(cd "$repo" && PATH="$abin:$PATH" "$APPLY" agy claude issue-45-pairing 2>&1); rc=$?
if [ "$rc" = "2" ]; then ok "apply.sh refuses a reviewer the author ledger names"
else
  bad "an author was accepted as the reviewer of its own work (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | head -4
fi
if [ ! -f "$repo/.worktrees/issue-45/launched.txt" ]; then
  ok "before launching either of them"
else bad "the refusal came after an agent had already run"; fi
# Compared the way review-gate-check.sh:88 compares, or a name spelled differently in the
# two files passes here, launches both agents and is refused at the commit.
printf 'Claude\n' > "$repo/.worktrees/issue-45/openspec/changes/issue-45-pairing/authors"
out=$(cd "$repo" && PATH="$abin:$PATH" "$APPLY" agy claude issue-45-pairing 2>&1); rc=$?
if [ "$rc" = "2" ] && [ ! -f "$repo/.worktrees/issue-45/launched.txt" ]; then
  ok "however the ledger spells the author's name"
else bad "a differently-cased author was accepted as its own reviewer (exit $rc)"; fi
find "$abin" -mindepth 0 -delete 2>/dev/null
teardown

# End to end on the shape that could not be driven: propose writes the delta and the
# declaration, implement ticks its boxes and writes no code, the review approves, and what
# lands names the planner as the author. That last field is what #266 wrote by hand.
setup
git worktree add -q .worktrees/issue-46 -b issue-46-endtoend 2>/dev/null
ebin=$(mktemp -d)
cat > "$ebin/claude" <<'FAKE'
#!/usr/bin/env bash
mkdir -p openspec/changes/issue-46-endtoend/specs/thing
printf '# Proposal\n\nDELIVERABLE: spec-only\n' > openspec/changes/issue-46-endtoend/proposal.md
printf '## MODIFIED Requirements\n' > openspec/changes/issue-46-endtoend/specs/thing/spec.md
echo '{"type":"result","subtype":"success","result":"proposed","session_id":"sess-46"}'
FAKE
cat > "$ebin/agy" <<'FAKE'
#!/usr/bin/env bash
printf -- '- [x] 1.1 confirmed; the delta is the whole deliverable\n' \
  > openspec/changes/issue-46-endtoend/tasks.md
echo '{"conversation_id":"c","status":"COMPLETED","response":"nothing to write"}'
FAKE
cat > "$ebin/codex" <<'FAKE'
#!/usr/bin/env bash
echo '{"type":"thread.started","thread_id":"t-46"}'
echo '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"VERDICT: APPROVE"}}'
FAKE
chmod +x "$ebin/claude" "$ebin/agy" "$ebin/codex"
(cd "$repo" && PATH="$ebin:$PATH" "$STAGE" propose claude issue-46-endtoend) >/dev/null 2>&1
add_passing_review issue-46-endtoend
d46="$repo/.worktrees/issue-46/openspec/changes/issue-46-endtoend"
out=$(cd "$repo" && PATH="$ebin:$PATH" "$APPLY" agy codex issue-46-endtoend --rounds 1 2>&1); rc=$?
if [ "$rc" = "0" ]; then ok "a change whose deliverable is the delta runs through apply.sh"
else
  bad "apply.sh could not drive a spec-only change (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | tail -4
fi
if grep -qx 'AUTHORS: claude' "$d46/diff-review.md" 2>/dev/null; then
  ok "naming the stage that wrote the delta, which is what landed"
else bad "AUTHORS reads '$(grep '^AUTHORS:' "$d46/diff-review.md" 2>/dev/null)'"; fi
arch46="$repo/.worktrees/issue-46/openspec/changes/archive/2026-01-01-issue-46-endtoend"
mkdir -p "$repo/.worktrees/issue-46/openspec/changes/archive"
cp -r "$d46" "$arch46"
( cd "$repo/.worktrees/issue-46" && "$here/review-gate-check.sh" . \
    openspec/changes/archive/2026-01-01-issue-46-endtoend/diff-review.md ) >/dev/null 2>&1
grc=$?
if [ "$grc" = "0" ]; then ok "and the landing gate accepts it with nobody filling a field in by hand"
else
  bad "the landing gate refuses a spec-only change (exit $grc)"
  ( cd "$repo/.worktrees/issue-46" && "$here/review-gate-check.sh" . \
      openspec/changes/archive/2026-01-01-issue-46-endtoend/diff-review.md ) 2>&1 \
    | sed 's/^/        /' | head -3
fi
find "$ebin" -mindepth 0 -delete 2>/dev/null
teardown
fi

# --- a stage that gives no account of itself (#315) ---------------------------------
# One run of #287 hit both shapes of this in an afternoon. agy printed nothing at all
# across 21 minutes while writing 1193 lines, and the line that copied its empty capture
# over the log left a 0-byte record of a finished run. opencode printed a transcript with
# no answer in it and exited 0, which no caller can tell from a stage that ran, so the
# driver went on to review a diff opencode had not written. Both are refused now, by one
# rule that does not consult the role or the status the agent chose to exit with.
if [ "$pty_available" = "1" ]; then
setup
add_change issue-40-silent
add_passing_review issue-40-silent
bin=$(mktemp -d)

# An agent that does the work and says nothing, exiting 0: opencode's status, agy's
# silence. The work is on the tree and nothing accounts for it.
cat > "$bin/agy" <<'FAKE'
#!/usr/bin/env bash
echo "1193 lines of it" >> implemented.txt
exit 0
FAKE
chmod +x "$bin/agy"
out=$(cd "$repo" && PATH="$bin:$PATH" "$STAGE" implement agy issue-40-silent 2>&1); rc=$?
log="$repo/.worktrees/issue-40/.agent-runs/implement-agy.log"
if [ "$rc" = "7" ]; then ok "an implement stage that printed nothing is refused"
else
  bad "an implement stage that printed nothing exited $rc, not 7"
  printf '%s\n' "$out" | sed 's/^/        /' | head -6
fi
if printf '%s\n' "$out" | grep -q 'status: NO_OUTPUT'; then
  ok "and is reported as NO_OUTPUT, not as an answer nobody could read"
else bad "the status word does not distinguish an empty capture from an unreadable one"; fi
if [ -s "$log" ] && grep -q 'wrote nothing' "$log"; then
  ok "and the log says the agent wrote nothing, rather than being 0 bytes"
else bad "implement-agy.log is $(wc -c < "$log" 2>/dev/null | tr -d ' ') bytes: $(head -1 "$log" 2>/dev/null)"; fi
if printf '%s\n' "$out" | grep -q 'the work is here, the account of it is not'; then
  ok "and says the tree changed while the agent said nothing"
else bad "a silent stage that changed the tree does not say so"; fi
# Not "implemented.txt still exists": the refusal reverts nothing, so that holds whatever
# this script does and would advertise coverage it has none of. What is new is that the
# log sends a person to the work the agent never described.
if grep -q '\.worktrees/issue-40' "$log" 2>/dev/null && grep -q 'git diff' "$log" 2>/dev/null; then
  ok "and points at the worktree holding the work nothing accounts for"
else bad "the log does not say where the unaccounted work is: $(head -c 120 "$log" 2>/dev/null)"; fi

# The same silence, exiting 2 as agy did. The refusal must not be the agent's choice:
# before this, agy's 2 stopped the run and opencode's 0 did not, on identical failures.
cat > "$bin/agy" <<'FAKE'
#!/usr/bin/env bash
echo "more of it" >> implemented.txt
exit 2
FAKE
chmod +x "$bin/agy"
out=$(cd "$repo" && PATH="$bin:$PATH" "$STAGE" implement agy issue-40-silent 2>&1); rc=$?
if [ "$rc" = "7" ]; then ok "the same silence exiting 2 is the same refusal, not the agent's status"
else bad "a silent implement exiting 2 came back as $rc, not 7"; fi

# The other shape: console noise, no answer in it, exit 0. This is what opencode did on
# the implement stage of #287, and the transcript is the only lead a person has, so it
# must survive as the log.
cat > "$bin/opencode" <<'FAKE'
#!/usr/bin/env bash
echo '{"type":"tool_use","name":"edit","input":{"path":"src/lib.rs"}}'
echo "console noise with no answer anywhere in it"
echo "written by an agent that never said so" >> implemented.txt
exit 0
FAKE
chmod +x "$bin/opencode"
out=$(cd "$repo" && PATH="$bin:$PATH" "$STAGE" implement opencode issue-40-silent 2>&1); rc=$?
olog="$repo/.worktrees/issue-40/.agent-runs/implement-opencode.log"
if [ "$rc" = "7" ]; then ok "an implement stage whose output carries no answer is refused"
else
  bad "an implement stage with no answer in its output exited $rc, not 7"
  printf '%s\n' "$out" | sed 's/^/        /' | head -6
fi
# One assertion over both facts, because keeping a non-empty capture as the log is not new:
# the old line copied every capture over the log regardless. Only the status word tells the
# two failures apart, so the transcript check rides with it rather than passing on its own.
got=$(printf '%s\n' "$out" | sed -n 's/^role:.*status: \([A-Z_]*\).*/\1/p' | head -1)
if [ "$got" = "NO_ANSWER_IN_OUTPUT" ] && grep -q 'console noise with no answer' "$olog" 2>/dev/null; then
  ok "and is reported as NO_ANSWER_IN_OUTPUT, with the transcript kept as its log"
else bad "an unreadable transcript reported '$got' over $(wc -c < "$olog" 2>/dev/null | tr -d ' ') bytes of log"; fi
find "$bin" -mindepth 0 -delete 2>/dev/null
teardown

# A silent run must not leave the PREVIOUS run's answer standing as its own record, and
# must not leave an empty file where that answer was. Both are the same fallback: a
# destination overwritten with something known to be worse, or knowingly left stale.
setup
add_change issue-42-record
add_passing_review issue-42-record
bin=$(mktemp -d)
cat > "$bin/agy" <<'FAKE'
#!/usr/bin/env bash
echo "first round" >> implemented.txt
echo '{"conversation_id":"conv-42","status":"COMPLETED","response":"PREVIOUS-ANSWER"}'
FAKE
chmod +x "$bin/agy"
(cd "$repo" && PATH="$bin:$PATH" "$STAGE" implement agy issue-42-record >/dev/null 2>&1)
log="$repo/.worktrees/issue-42/.agent-runs/implement-agy.log"
if grep -q 'PREVIOUS-ANSWER' "$log" 2>/dev/null; then
  ok "precondition: the first run leaves an answer for the silent one to overwrite"
else bad "the first run left no answer to overwrite; the case below proves nothing"; fi
cat > "$bin/agy" <<'FAKE'
#!/usr/bin/env bash
echo "second round" >> implemented.txt
exit 0
FAKE
chmod +x "$bin/agy"
(cd "$repo" && PATH="$bin:$PATH" "$STAGE" implement agy issue-42-record >/dev/null 2>&1)
# All three conditions in one, because two of them pass on a 0-byte log: an empty file
# holds no previous answer either. Only "non-empty AND this run's own account AND not the
# previous one" separates the fix from the destruction it replaced.
if [ -s "$log" ] && grep -q 'wrote nothing' "$log" && ! grep -q 'PREVIOUS-ANSWER' "$log"; then
  ok "a silent run records its own silence: neither empty nor the previous run's answer"
else bad "the silent run's log is $(wc -c < "$log" 2>/dev/null | tr -d ' ') bytes: $(head -1 "$log" 2>/dev/null)"; fi
find "$bin" -mindepth 0 -delete 2>/dev/null
teardown

# A review was always refused with 7, so exit 7 alone says nothing about this change and
# is asserted together with what is new: which of the two failures the run reports, and
# what it leaves behind to read. apply-tests.sh covers the refusal itself for a review.
setup
add_change issue-43-quiet
bin=$(mktemp -d)
cat > "$bin/codex" <<'FAKE'
#!/usr/bin/env bash
exit 0
FAKE
chmod +x "$bin/codex"
out=$(cd "$repo" && PATH="$bin:$PATH" "$STAGE" review codex issue-43-quiet 2>&1); rc=$?
rlog="$repo/.worktrees/issue-43/.agent-runs/review-codex.log"
got=$(printf '%s\n' "$out" | sed -n 's/^role:.*status: \([A-Z_]*\).*/\1/p' | head -1)
if [ "$rc" = "7" ] && [ "$got" = "NO_OUTPUT" ]; then
  ok "a silent review is refused as NO_OUTPUT, not as an answer nobody could read"
else bad "a silent review exited $rc reporting '$got'"; fi
if [ -s "$rlog" ] && grep -q 'wrote nothing' "$rlog"; then
  ok "and its log records the silence rather than being 0 bytes"
else bad "review-codex.log is $(wc -c < "$rlog" 2>/dev/null | tr -d ' ') bytes after a silent review"; fi

# The other half of the same role: a transcript with no answer in it. The review-specific
# refusal must still be the one that fires, and it is asserted with the status word, since
# that sentence is what the old code said too.
cat > "$bin/codex" <<'FAKE'
#!/usr/bin/env bash
echo "reading the diff"
echo "no envelope anywhere in this"
FAKE
chmod +x "$bin/codex"
out=$(cd "$repo" && PATH="$bin:$PATH" "$STAGE" review codex issue-43-quiet 2>&1); rc=$?
got=$(printf '%s\n' "$out" | sed -n 's/^role:.*status: \([A-Z_]*\).*/\1/p' | head -1)
if [ "$rc" = "7" ] && [ "$got" = "NO_ANSWER_IN_OUTPUT" ] \
   && printf '%s\n' "$out" | grep -q 'Refusing to treat a transcript as a review'; then
  ok "and an unreadable one is NO_ANSWER_IN_OUTPUT, still refused as a review"
else bad "an unreadable review exited $rc reporting '$got'"; fi
find "$bin" -mindepth 0 -delete 2>/dev/null
teardown

# What #287 actually cost: the driver read a refused implement stage as a finished one and
# launched the diff review on a tree that implementer never wrote. apply.sh stops instead,
# and stops BEFORE the reviewer, because there is nothing yet to review.
setup
add_change issue-44-driver
add_passing_review issue-44-driver
bin=$(mktemp -d)
cat > "$bin/agy" <<'FAKE'
#!/usr/bin/env bash
echo "unaccounted for" >> implemented.txt
exit 0
FAKE
cat > "$bin/codex" <<FAKE
#!/usr/bin/env bash
echo ran >> "$bin/.reviewer-ran"
echo '{"type":"thread.started","thread_id":"t-44"}'
echo '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"VERDICT: APPROVE"}}'
FAKE
chmod +x "$bin/agy" "$bin/codex"
out=$(cd "$repo" && PATH="$bin:$PATH" "$APPLY" agy codex issue-44-driver 2>&1); rc=$?
if [ "$rc" = "7" ]; then ok "apply.sh stops when the implementer gives no account of itself"
else
  bad "apply.sh carried on past an unaccounted implement stage (exit $rc)"
  printf '%s\n' "$out" | sed 's/^/        /' | head -6
fi
if [ ! -f "$bin/.reviewer-ran" ]; then
  ok "and never launches the reviewer on a diff nobody claims"
else bad "the reviewer was launched on work the implement stage did not report"; fi
if [ ! -f "$repo/.worktrees/issue-44/openspec/changes/issue-44-driver/diff-review.md" ]; then
  ok "and records no diff review from it"
else bad "a diff review was recorded for an unreported implement stage"; fi
find "$bin" -mindepth 0 -delete 2>/dev/null
teardown
fi

printf '\n%s passed, %s failed\n' "$pass" "$fail"
[ "$fail" = "0" ]
