# How changes get made

This document describes the path a behavior change takes from issue to `main`, and the single point
at which it stops for a human decision.

The rules an agent follows are in [`AGENTS.md`](../AGENTS.md). The loop's own mechanics live with the
loop, a git subtree at `tools/openspec-loop/` reached through `.workflow/loop <command>`.

## The input

A change starts as a GitHub issue. Issues and milestones are the only backlog, and one change
implements exactly one open issue: scope is agreed there, before anything else happens.

Not every commit is a change in this sense. The path below is for changes to labeler's behavior: its
API, its template schema, its layout model, its coordinates and its error contract, which are exactly
the ones that write a spec delta. The harness that runs this path is not labeler, however much
changing it changes what the path does, so a commit against `tools/openspec-loop/`, `.workflow/`,
`.claude/`, `AGENTS.md`, this file or `openspec/config.yaml` is not a behavior change here. Nor is a documentation
fix, a CI change or a dependency bump. Each still starts as an issue and still ends as one commit that
closes it, but none of the stages apply to it. Nothing declares which kind a piece of work is, and
size has no say: writing the delta is what makes it a change, and needing one is discovered rather
than decided.

The exclusion is not an exemption granted to the tooling. The three stages below exist to get a
contract under `openspec/specs/` reviewed before it is built, and a change to the harness writes no
such contract, so a plan review of one would be judging a proposal against nothing.

From there the work runs in three stages — plan, implement, archive — each started by one command and
each running to completion without supervision: isolating the work, writing the spec and design,
getting them reviewed, implementing, reviewing the implementation, and updating the specs of record.
Nothing is committed while they run. The commit comes after the last of them, and closes the issue
when the branch merges.

The stage boundaries are deliberate. Planning stops before implementation so the spec and design are
settled, and reviewed, while changing them is still cheap.

Work discovered mid-change that falls outside its issue becomes a new issue. It does not widen the
one in flight, and it is never parked as an unchecked task.

## Running it

One command runs the whole loop:

```
/change 283 claude codex agy codex
```

The issue number, then the four agents: who plans, who reviews the plan, who implements, who reviews
the code. It scopes the issue with you first, which is the only part that asks anything, and then runs
worktree, plan, plan review, implementation, diff review, archive, the gates and the commit unattended,
printing the merge sequence and stopping. The merge into `main` is left to you, and by that point it is
mechanical.

Which stage runs next is read off the artifacts on disk rather than off a record of what ran, so
re-running the same command after any stop resumes where it left off instead of starting over.

The stages can also be driven one at a time, which is what the rest of this section describes.

File the issue, then drive the stages. Use the slash commands explicitly — a plain-English request
like "implement #181" is unreliable, because it matches the *apply* skill ("Implement tasks from an
OpenSpec change") rather than propose, and would try to apply a change that does not exist yet.

```bash
gh issue create --title "Duplicate template id should not be fatal" --body "..."
```

```
/opsx:propose issue #181
```

Writes the proposal, the delta specs and the design, then **stops**. Not the task list: that is
written after the review, because a task list drawn up for a plan the reviewer then sends back
describes work nobody approved. Planning does not run on into
implementation in the same turn, by design: the artifacts are meant to be settled before code exists.
The adversarial review of that plan runs next, on a different model, and gates what follows.

```
/opsx:apply
```

Refused until the review passes. Runs the task list, then the implementation is reviewed in turn. It
commits nothing: an approved diff is not a landed one.

```
/opsx:archive
```

Folds the delta specs into `openspec/specs/`, archives the planning record, and stops short of the
commit.

That is where the commit belongs, and only there. Archive rewrote `openspec/specs/` after the last
review pass, so what it produced is checked rather than re-read: the published specs must be the
reviewed delta applied to the previous commit, which a gate can decide and a person re-reading their
own output cannot. The verification gates run, and then the whole change lands as a single commit and
the branch merges.

Also available: `/opsx:explore` for thinking something through before an issue exists, and
`/opsx:update` for revising a change's plan in place after a review asks for edits.

### Which agent runs which stage

Roles are largely interchangeable, under two constraints. The reviewer is never the author, which the
commit gate checks. And an author must be resumable, because every loop sends findings back to
whoever wrote the thing; naming an author that cannot be resumed is refused before anything is
launched. Every agent currently satisfies that, so the refusal has nothing to fire on today. It is
kept because it is what a new entry is checked against: `opencode` failed it until it grew a way to
continue a session, and it took every role from that point.

A stage that fails on a model says which model it was on and what to set to move off it, and then
stops. It does not move off it. The default model for `opencode` costs nothing, and the model that
gets past a spent free allowance is billed to this account, so the run offers that as a line to run
and never runs it: paying is a decision with a bill attached, and a runner does not get to make it
quietly. Where the tool said its free allowance is what ran out, the message says so; where it said
anything else, the message repeats what it said and claims nothing about allowances. Either way the
run stops and the same override is offered, so a wrong guess about the cause costs a less precise
sentence and never a surprise charge.

Nothing checks before a stage starts that the model it names still answers. That check is the one
thing that would have caught what this was filed over, and it is not here: the list a tool can be
asked for is a cache of names, an exhausted model is still a name on it, and a check that passes
whenever it cannot see the answer is worse than no check. So this fails late, loudly, and legibly
rather than early.

The stage commands differ per agent, because OpenSpec writes a separate command set for each and not
every tool reads the same one. Two forms exist: the **workflow** form `/opsx-*` and the **skill** form
`/openspec-*`.

| Agent | Stage commands | Notes |
| --- | --- | --- |
| Claude Code | `/opsx:propose`, `/opsx:apply`, `/opsx:archive` | Colon-separated. From `.claude/commands/opsx/`. |
| `agy` (Antigravity) | `/openspec-propose`, `/openspec-apply-change`, `/openspec-archive-change` | **Skill form.** The CLI reads `.agent/skills/` and ignores `.agent/workflows/`, so `/opsx-apply` is not a command despite what OpenSpec's tool table implies. |
| `codex` | none needed | Used for reviews via `codex exec`, with the instructions piped in. |
| `opencode` | `/opsx-apply` and friends, from `.opencode/commands/` | Verified: a slash command in the run message is expanded, not passed through as text. |

Implementation and its review run on two named agents, given when the stage is started rather than
decided later:

```bash
.workflow/loop apply agy codex issue-186-pin-rust-toolchain
```

The pair comes first because it is the guarantee. The change comes last and is optional: left out, it
is resolved from the worktree you are standing in, and refused rather than guessed when several are in
flight.

The first agent implements, the second reviews, and naming the same one twice is refused. The script
owns the loop: findings return to the implementer, which resumes its session, and the reviewer
re-checks, for up to three rounds. A change that has not converged by then stops for a person rather
than looping; the reviewer's verdict is a `VERDICT:` line, so the decision to loop is read rather than
inferred from prose. Findings go
back to the implementer, which keeps the session it built in, and the reviewer re-checks; the reviewer
never fixes what it found. Transcripts stay in logs rather than being read back, since a full agent
transcript is thousands of lines of no interest to anyone.

Its commits are gated the same as anyone's — `core.hooksPath` resolves inside a worktree, so the
vendored `pre-commit` hook runs. What it cannot see is the Claude Code edit-time hook, so its only gate is
at commit time.

Stages hand off through **files on disk, not conversation**. Every artifact's instructions re-read
their dependencies from disk, so a stage can run in a fresh session, a different agent, or the same
session as the last one. Fresh context is required in exactly one place: the review, which must not
run in the context that wrote the artifacts.

The gate applies to all of them equally. It is a committed git pre-commit hook plus the same check in
CI, judging files rather than which agent produced them. Enable it once per clone:

```bash
.workflow/loop setup
```

The hooks also refuse one shape outright: a change branch merging into itself. Every check that reads
history reads it through a single base ref, and a merge leaves two previous commits for that one ref
to explain, so the archive check reports whichever parent it was not pointed at as an unauthorised
hand-edit. A branch that has fallen behind is rebased onto `main` instead, and integration is a
fast-forward. That is why the branch you push may be replaced rather than added to: rewriting is
confined to change branches, which nothing is based on and which are deleted once merged.

### Checking on a change

```bash
openspec list                              # changes in flight
openspec status --change issue-181-...     # which artifacts are done, what is blocked
openspec show issue-181-...                # read a change
openspec view                              # interactive dashboard of specs and changes
```

`openspec status` is the one that answers "why has this not moved": it prints each artifact and what
it is blocked by.

### Prerequisites

`openspec` (1.9.0, pinned — the committed schema is generated by it), `gh`, `codex` for reviews, and
`agy` for implementation. The repository already carries the schema, config, and review hook.

```bash
openspec doctor                            # confirms the repo's OpenSpec root is sound
```

## The output

A change lands as **one commit** carrying the code, the updated specs, and the archived planning
record, which is where the decision and its reasoning live. It closes its issue on push.

The planning record is kept rather than discarded, so for any past change the proposal, the design,
the reviewer's objections, and their resolution remain readable.

## What review guarantees

Every change is reviewed twice, and the reviews are adversarial by construction rather than by
intent:

- **The plan is reviewed before any code exists.** Spec and design are judged together, so a design
  contradicting its own spec is catchable.
- **The implementation is reviewed after it is written.**

An artifact is never reviewed by whoever wrote it. The plan review records one `AUTHOR:`; the code
review records `AUTHORS:`, every agent that actually changed the worktree, because code can pass
through more than one pair of hands. The gate refuses a change whose reviewer appears among its
authors, and refuses a code review that claims no author at all, so the rule is checked rather than
trusted. Both are recorded as files, `review.md` and `diff-review.md`, because a verdict that lives
only in a transcript is a verdict nothing can check.

The code review also records `TREE_SHA256:`, the tree it judged. Without it the folder can hold a
stack of verdicts with no way to tell which of them, if any, describes the code that shipped, and it
once did: two rounds of one change returned opposite verdicts on an identical tree. A review is never
launched a second time on a tree an earlier round already judged; the run stops for a person instead,
because whether an implementer answered the findings in prose or ignored them is a judgement no
further round can make.

The body of `review.md` is the reviewer's own final message, not a summary of it. Nothing transcribes
a review, which keeps the interested party out of the record and avoids pulling a thousand-line
transcript through the author's context to copy something already on disk. The heading fields above it
are written by the driver rather than by the reviewer, because those fields are what the commit gate
reads and an agent asked to fill them in can fill them in wrong.

The reviewer works from the files alone, without access to the conversation that produced them. Where
its tool can enforce read-only it is launched that way; where it cannot, a check compares the worktree
before and after its turn and throws the verdict out if anything moved. Either way a reviewer that
edited what it reviewed does not get to approve it.

A review ends in one of three verdicts. `APPROVE` proceeds. `APPROVE WITH CHANGES` lists specific
required edits: the author applies them and the work continues, with no second review, which is why a
reviewer is told to file anything it cannot state completely as `REVISE` instead. `REVISE` marks a
fundamental defect: the plan is fixed and reviewed again from scratch, by a reviewer starting fresh.

Four things the process refuses to let slide:

- A blocking finding cannot coexist with approval.
- Altering the contract after approval voids that approval, and the gate detects it. The contract is
  the delta specs. Correcting a factual error in the proposal or the design costs nothing on purpose:
  a rule that charges a full re-review for a correction is a rule that rewards leaving the plan
  wrong.
- A serious finding cannot be dismissed by the author alone; the reviewer must accept the rebuttal.
- A reviewer that is unavailable or produces nothing usable stops the change. It never degrades to
  self-review, and never substitutes an assumed verdict.

## Where it stops

Four things stop the work, and each wants something different from you.

**A stage asks a question.** It hit something it could not decide: a contradiction in what it was
given, or a missing decision that changes the contract. It writes the question down and stops rather
than guessing, and you answer it. This is the cheapest of the four, and the point of allowing it is
that the alternative is a guess buried in an artifact a later reader trusts.

**A review will not converge.** A plan revised three times that still has not passed, or a diff
reviewed three times that still has not. The change halts with the findings surfaced. The decision
needed there is direction, not proofreading: correctness, scope and edge cases have already been
examined more thoroughly than a read-through would manage, and what remains unanswerable by a reviewer
is whether the change solves the right problem, whether its scope is right, and whether it is worth
doing at all.

**A gate fails twice.** The implementer gets one round to fix what `fmt`, `clippy` or the tests
reported. A second failure is a defect rather than a lint, and it stops. What that round wrote is code
like any other, so the code reviewer judges it before the commit, and a rejection stops the run for the
same reason a second failure does. A test that fails identically
at the commit the branch forked from is neither: the suite is run there too, the failures present in
both are named and set aside, and only a failure the base does not have stops anything. Formatting and
lints are never set aside that way. Where the comparison cannot be made at all, every failure counts
against the change and it stops, because a change waved through on an attribution nobody could make is
worse than a stop.

**The merge.** Nothing reaches `main` unattended. The work is committed on its branch when you are
asked, so what is left is a decision, not an inspection.

## What is not guaranteed

- The pre-commit hook is skippable with `git commit --no-verify`. CI runs the identical check on what
  lands, so a skipped hook delays the refusal rather than avoiding it.
- The refusal of a merge on a change branch is skippable the same way, with `git merge --no-verify`.
  CI catches it only where both sides changed published specs, which is the case that cannot be read
  at all; a merge that CI can still read passes, and leaves the graph it leaves.
- The gates check a change that exists. Whether a given diff *should* have been a change at all is a
  judgement no gate can make, so a commit carrying no change folder is checked by nobody.
- The one round the implementer gets to fix a failed gate is reviewed like any other edit, but only
  when the driver ran it. The driver records what that round left behind, and the landing check
  compares the standing approval against that record; a gate fix made by hand records nothing, so
  nothing is compared and the edit lands unreviewed.
- A failure set aside as pre-existing was measured once, on the machine running the change. A test
  that fails for the machine rather than for the code is set aside with it, which is the point; a test
  that fails intermittently can be set aside by happening to fail at both ends of one comparison.
- Whether a rendered label looks right is checked by nobody either. It is a visual judgement made
  against a running server, no artifact of it reaches the repository, and the process says so rather
  than carrying a checkbox that cannot fail.
- There are no pull requests and no branch runs. What is given up is a check on a clean machine
  before the commit lands; a broken commit surfaces on `main`'s own CI run instead, where it is a
  post-mortem rather than a gate. Publishing is unaffected, because `build` needs `[rust, ui]` and
  runs only on `main` or a tag, so a broken commit ships nothing until it is fixed forward.
- Specs live in two places during migration. `docs/SPEC.md` is frozen and remains authoritative for
  behavior that has not moved; `openspec/specs/` holds everything since. A spec in the new location
  names the frozen section it replaces, so precedence is recorded rather than inferred.

## Where things live

| | |
| --- | --- |
| Current behavior, frozen baseline | `docs/SPEC.md` |
| Current behavior, everything since | `openspec/specs/` |
| Why a change was made | its `proposal.md` and `design.md` under `openspec/changes/archive/` |
| Decisions taken before OpenSpec | `docs/adr/`, frozen and never extended |
| Planning records of past changes | `openspec/changes/archive/` |
| Template authoring guide | `docs/AUTHORING.md` |
| Rules the agent follows | `AGENTS.md` |
