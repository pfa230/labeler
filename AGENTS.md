# Labeler

A stateless label-rendering REST service (Rust/axum). It loads YAML label templates from
`{LABELER_CONFIG_DIR}/templates/` and renders a single label to PNG or a batch to PDF/ZIP, by
generating [Typst](https://typst.app/) source on the fly and compiling it in-process via
`typst-as-lib`.

## Where behavior is specified

Two places, and the split is deliberate (ADR-0057).

`docs/SPEC.md` is **frozen** at commit `bc7b1ce` (2026-08-19): the baseline for the API, template
schema, layout model, coordinates, and error contract as of that date. Do not edit it, and do not add
changelog entries to it.

`openspec/specs/<capability>/spec.md` holds everything added or changed since. It starts empty and
accrues one capability at a time.

**Precedence.** A frozen `docs/SPEC.md` section stays authoritative until an OpenSpec requirement
explicitly names and supersedes it, and then only for that section. To look a rule up: read
`docs/SPEC.md`, then check `openspec/specs/` for a requirement superseding it.

**First-touch.** The first change to behavior documented only in the frozen spec writes an `ADDED`
requirement holding the *complete* post-change contract, not the difference, naming the `docs/SPEC.md`
section it supersedes. A `MODIFIED` delta is only valid against a requirement that already exists in
`openspec/specs/`: the tooling resolves `MODIFIED` by locating that requirement, so a `MODIFIED`
against an unmigrated section has nothing to resolve against.

`docs/adr/` is **frozen** at ADR-0091 (2026-08-31), for the reason `docs/SPEC.md` is: it was the right
record for its era and a better one superseded it. Do not write ADRs and do not add rows. Its 87
entries stay readable and stay cited, because for behavior predating OpenSpec they are the only
account of *why*. Rationale for a change now lives in its `proposal.md` and `design.md`, kept
permanently under `openspec/changes/archive/`, and the contract lives in `openspec/specs/` (#285).

Also in `docs/`: `WORKFLOW.md` (how changes get made, for humans), `AUTHORING.md` (template model by worked example), `VISION.md`, `DEPLOY.md`.

## Breaking changes, until 1.0

Until `1.0`, a change that alters behavior breaks what came before, and that is the finished job. No
migration, no desugaring, no deprecation window, no second spelling, and no paragraph explaining the
one being removed. A dropped spelling becomes a parse error naming the file and the key, which
`deny_unknown_fields` gives once the field is gone; a field read and ignored is what this forbids.

Stored user data is the only exception: `store.rs:154-168` migrates the SQLite schema across
releases, because a user's printers and tokens have no author to fix them.

## Tracking work

GitHub issues and milestones are the sole live tracker. No markdown TODOs, no roadmap docs. File with
`gh issue create` and reference from commits (`Fixes #12`). Work you won't do now becomes an issue,
never a TODO in code or docs, and never an unchecked task in `tasks.md`.

The superpowers `writing-plans` step is retired here (ADR-0057); the OpenSpec change folder is the
plan, and it is committed. `superpowers:brainstorming` still helps for fuzzy ideas before a change
exists; its scratch stays under `docs/superpowers/` (gitignored).

## OpenSpec workflow

### Which changes go through it

Behavior changes, and nothing else. **Behavior means labeler's**: the API, the template schema, the
layout model, the coordinates and the error contract, which is what `docs/SPEC.md` froze and what
every capability under `openspec/specs/` names. The harness is not that, however much its own behavior
changes. `.workflow/`, `.claude/`, `.agent/`, `.agents/`, `.opencode/`, `.githooks/`, this file,
`docs/WORKFLOW.md` and `openspec/config.yaml` say how a change gets made, not what the service does,
and no capability under `openspec/specs/` is theirs to name. Every harness issue this repo has closed
already went that way, #283 and #313 included, and not one produced a change folder.

The discriminator is the **spec delta**, and it needs no declaring: a change to labeler's behavior
always produces one, because the first-touch rule makes the first change to any documented behavior
write the complete post-change contract as an `ADDED` requirement. A change with no delta has no
contract to review, and the loop below has nothing to gate.

So a documentation fix, anything under the paths above, a CI change, a dependency bump or a refactor
that keeps behavior identical goes: issue, worktree, the three gates, one commit with `Fixes #N`, push,
merge. No change folder, no plan review, no `diff-review.md`. Nothing else relaxes: it still starts as
an issue and still ends as one commit that closes it. It is also the lane a harness change wants on
its merits: the loop's four agents review a contract under `openspec/specs/`, and a change to the
loop itself writes none, so the plan review would be judging a proposal against nothing.

A correction to a published spec under `openspec/specs/` is not that lane, however much it reads like
a documentation fix. `archive-merge-check.sh:141` refuses a commit that changes a published spec with
no delta behind it, because those files are written by archive and never by hand, so the correction
arrives as a delta and a delta is what sends a change through the loop. What it does not have is code:
the deliverable is the delta itself, and the plan says so in one line, `DELIVERABLE: spec-only`
(#313).

Size decides nothing, and neither does effort. A nine-line handler check that alters behavior is a
full change; a five-hundred line documentation rewrite is not, and neither is rewriting the driver
end to end. There is no lane to declare, no criteria to qualify under and no
step that promotes one to the other. Writing a delta is what makes a change one, and
`review-gate-check.sh` demands a passing `review.md` from the moment a change folder exists, so
discovering mid-work that you need a delta costs the review and nothing else.

What no gate can decide is whether a diff *should* have carried a delta. A commit with no change
folder is checked by nobody, which `docs/WORKFLOW.md` records under what is not guaranteed.

### The loop

OpenSpec (CLI 1.9.0) on this project's own schema, `openspec/schemas/labeler/`:
`proposal → specs → design → review → tasks → apply`. Order matters, because the review gates
implementation and archive rewrites the main specs *after* it.

**Run it with `/change <issue#> [<planner> <plan-reviewer> <implementer> <code-reviewer>]`** (#283). That
scopes the issue with the user, then hands every stage below to `.workflow/run-change.sh`, which runs
them unattended through to a green branch run and stops there. **The issue body is the scope**: the
driver writes it to `.agent-runs/issue-<N>.md` in the worktree and the planner works from that file,
so a vague issue produces a vague plan and refining it is the point of the scoping stage. The four agents are named up front
because the pairing is the guarantee: nobody reviews their own plan, and nobody reviews their own code.
Which stage runs next is read off the artifacts, never off a ledger, so re-running after any stop
resumes rather than redoes. One function decides it, and `--dry-run` prints its answer, so what the
driver would do next is both testable and inspectable. `.workflow/run-stage.sh` still runs a single
stage when you want one.

**The four names are optional, and their default is machine-local (#330).** Given the issue number
alone, `run-change.sh` reads them from `.workflow/roles.local`, and `apply.sh` given neither agent
reads its two from the same file, keyed `planner`, `plan-reviewer`, `implementer` and `code-reviewer`.
The file is gitignored, for the reason `CLAUDE.local.md` is: which CLIs are installed and
authenticated is a property of this machine, and a lineup arriving by checkout would be repo policy
claiming to be a preference.

```
planner: claude
plan-reviewer: codex
implementer: agy
code-reviewer: opencode
```

All four roles or none, on both sides. Naming some on the command line and leaving the rest to the
file is refused rather than merged, because a lineup assembled from two places is one nobody can read
off either. Naming all of them replaces the file entirely. All four keys are required even by
`apply.sh`, which fills two: one complete lineup read the same way by both callers beats a file whose
meaning depends on who opened it. An unknown key is a parse error naming the file and the line, the
way `deny_unknown_fields` refuses a template's, because a key read and dropped is a preference that
silently did nothing.

Every validation applies identically to a value that came from the file, so it can never reach a
pairing the command line refuses, and a failure names the file and the key rather than printing a
usage line: a value read from a file is not fixed by reading a synopsis of arguments nobody typed.
With neither file nor arguments present, both scripts stop and name the path they wanted. Never a
default: a driver that picks its own four agents has made the one decision these commands exist to
record. `LABELER_ROLES_FILE` overrides the path, which is what keeps `change-tests.sh` and
`apply-tests.sh` from asserting against whichever lineup the developer running them happens to have.

One consequence worth knowing at the call site: `apply.sh` takes its change last, so once the pair may
be absent, `apply.sh issue-12-thing` names a **change** and not an implementer. A lone argument that
is a known agent is refused as half a pair rather than resolved as a change nobody could find.

The steps below are what those stages mean. Read them to understand the loop or to drive it by hand;
`/change` is how it is normally driven.

1. **Issue**, then a worktree: `git worktree add .worktrees/issue-<N> -b issue-<N>-<slug>`.
2. **`/opsx:propose`** writes `openspec/changes/issue-<N>-<slug>/`. Planning only; it must not touch
   code. Link the issue in `proposal.md`.
3. **Adversarial review of the plan**, before any task is written: the `review` artifact, judging
   `proposal.md` + `specs/` + `design.md`. A second model in read-only mode, else a fresh-context
   subagent. **Never** self-review inside the authoring context. It writes `review.md` ending in a
   `VERDICT:` line. `REVISE` → fix the artifacts and re-run the *full* review in a fresh context;
   `APPROVE_WITH_CHANGES` → the author applies the listed edits and sets `CHANGES_APPLIED: yes`, and
   the loop then proceeds with **no second review**, which is why a reviewer is told to file anything it
   cannot state completely as `REVISE` instead. The digest is written after those edits, so it covers
   the contract that will actually be built. Editing `specs/` afterwards voids the verdict, and the
   gate detects it; editing `proposal.md` or `design.md` does not, because they are context and not
   the contract.

   **This is the only place a human enters the loop, and only on failure.** Three consecutive `REVISE`
   rounds is a hard stop: do not implement, do not keep retrying. Surface `review.md` and the
   artifacts, and wait. On the converging path the loop runs unattended through to the merge. The cap
   is weighed *after* the author has been given the last findings, never before: stopping first leaves
   that `REVISE` unacted on, so a restart re-reviews the same artifacts, reaches the same verdict and
   stops again, which is a loop no number of restarts can move.

   **`tasks.md` is written after this review, never before it.** The schema has `tasks` requiring
   `review` (`openspec/schemas/labeler/schema.yaml`), because a task list written for a plan the
   reviewer then sends back describes work nobody approved. `run-change.sh` runs a `tasks` stage on
   the planner once the verdict passes, and rewrites it whenever the plan moved in that run.

   **Launching it (#275).** `run-stage.sh plan-review <agent> <change>` owns the invocation, and
   `run-change.sh` owns the loop around it: the reviewer is launched **fresh every round**, because a
   resumed reviewer judges the delta since its own last message rather than the artifact in front of
   it, so a regression the fix round introduced outside its findings is never examined. The author
   resumes, because it must keep what it built. `review.md` is written by the driver from the
   reviewer's own final message, not by the reviewer, because the canonical fields are the gate's
   contract and an agent asked to fill them in can fill them in wrong.

   Launch any long run **detached**, never as a harness background task: one was killed 4.3 seconds
   after its turn ended, taking 15,127 lines of review with it, with no reason recorded and no way to
   tell that from a `TaskStop`. `.workflow/detach.sh` is the one spelling, because the previous one was
   prose that named `setsid` and `timeout`, and macOS ships neither (#284):

   ```bash
   run=$(.workflow/detach.sh /tmp/change-283 .workflow/run-change.sh 283 claude codex agy codex)
   .workflow/detach.sh --wait "$run"
   ```

   The launch prints a **handle** on stdout, which is the log file, and that handle is what `--wait`
   takes: every launch gets its own, so two runs never share a transcript and no launch has to be
   cleaned up before the next. It puts the run in its own **session**, which is what survives a
   harness reaping a process group: `setsid` where there is one, else `python3`'s `os.setsid()`,
   else plain `nohup`, which is only SIGHUP-proof and says so when it is used. `--wait` is how you learn the outcome, never the process
   existing: where `setsid` forks rather than execs, the pid belongs to a parent that exits at once
   while the real run is orphaned into another session. It reports the run's exit status or that its deadline
   passed. The launch prints the handle either way and uses its **exit status** to say whether the run
   was seen to start, so `run=$(...)` always gives you something to wait on and a non-zero launch is
   the signal to look before you do. The
   status file exists because backgrounding throws the status away: a shell reports 0 for having
   *started* a background job whatever becomes of it, which is how the old line's missing `timeout`
   (a real 127) came back as a clean pass.

   A zero exit with no verdict is the failure to watch for: `codex exec` given an empty stdin prints
   nothing and exits 0, which is indistinguishable from a clean pass unless you look. `run-stage.sh`
   refuses any stage whose answer it could not extract (exit 7) and `run-change.sh` refuses a log with
   no readable `VERDICT:` line (exit 4), so that failure now has a name rather than a silent pass.

   That refusal covers **every role and every agent exit status** (#315). It once covered the two
   review roles alone, and on the implement stage of #287 opencode returned no result and exit 0,
   which reads exactly like a stage that ran: the driver went on to review code it had not written.
   agy hit the same extraction failure the same day and only stopped the run because it happened to
   exit 2, so which of the two failures stopped anything was the agent's choice and not the driver's.
   `run-stage.sh` reports the two shapes apart: `NO_ANSWER_IN_OUTPUT` when the capture is a console
   transcript with no answer in it, which becomes the stage log, and `NO_OUTPUT` when the agent
   printed nothing, where the log records that absence and says whether the tree changed anyway. It
   never copies an empty capture over the log, which is how a 21-minute agy run that wrote 1193 lines
   came back as a 0-byte record of itself.

4. **Apply and review the diff**, as a named pair:
   `.workflow/apply.sh [<implementer> <reviewer>] [change]`, or `/apply` with the same arguments. The
   pair is named first because it is the guarantee, and may be left to `.workflow/roles.local`; the
   change is last and optional, resolved from the worktree you are standing in, or from the single one
   in flight across `.worktrees/`, and refused rather than guessed when several are. With no pair
   named, a lone argument is that change. `--rounds N` moves the three-round cap and `--dry-run` shows
   what it would do without launching anything. It runs both roles and the fix loop between them;
   `.workflow/run-stage.sh` runs a single stage, and still takes the change explicitly. Prefer it over
   `/opsx:apply`: implementing here means this session would have to review its own diff, and the
   pairing exists precisely so that separation does not depend on remembering. Findings return to the implementer, which resumes its
   session; the reviewer re-checks and never edits. Two different reviews: step 3 judged the plan,
   this one judges the code. Do not skip it because tasks are checked.

   **A transcript belongs in a log, not in this context and not in the repository.**
   Every run artifact goes to `.agent-runs/` at the worktree root: `run-stage.sh` writes
   `<role>-<agent>.{log,json,conversation}` there, `apply-with-agy.sh` writes `agy-apply.*`, and a
   new script writes its own there too. `.gitignore` matches the directory, so a `git add -A` stages
   the change's output and nothing else; untracked was not enough, because it left every commit
   depending on whoever ran it noticing the dotfiles (#255). `review.md` and `diff-review.md` are the
   record, and each holds the reviewer's own final message rather than a summary of it, so there is
   nothing left to preserve alongside. The raw capture a plan review extracts from is working state:
   keep it outside the repository, never in the change folder, where `git add -A` would commit it. An
   earlier convention committed the raw `codex exec` capture next to the review, banner and session
   id included; 19 such files reached 47,190 lines, against 893 lines of actual planning record in
   the worst change, and they are gone (#244).

   `apply.sh` records the outcome as `diff-review.md` in the change folder, carrying `AUTHORS:`,
   `REVIEWER:`, `VERDICT:`, `ROUNDS:`, `TREE_SHA256:` and `SPECS_SHA256:`, with each round kept
   alongside as `diff-review-<n>.md` under its own `TREE_SHA256:`. That file is what the gate reads,
   so a verdict living only in a transcript is a verdict nothing can check.

   Two of those fields say *what* was judged and *who* wrote it, because a verdict answers neither on
   its own (#299). `TREE_SHA256:` is the digest of the worktree the approving review was handed, minus
   `openspec/changes`; `run-stage.sh` prints it as a `tree:` line and `apply.sh` records it. `AUTHORS:`
   is every agent whose `implement` or `gate-fix` stage actually changed that worktree, comma-separated
   and first-written-first, read from the `authors` ledger `run-stage.sh` keeps in the change folder.
   It is not the implementer this invocation was given: during #291 that named the last stage to run,
   attributing six rounds of another agent's work to an agent that wrote none of it. On a change
   declaring `DELIVERABLE: spec-only` it is instead the `propose` stage that wrote the delta, because
   the delta is what lands and no implement stage wrote a line of it.

   **A change whose deliverable is the delta.** A plan may ask for no code at all: correcting a
   published spec is that shape, and #266 was it, twelve verification tasks over a `MODIFIED`
   requirement the planner had already written. Such a proposal carries one line reading
   `DELIVERABLE: spec-only`, and that line is the whole mechanism. `run-stage.sh` reads it **before**
   the agent launches and never after, because `openspec/changes` is outside implement's work digest,
   so a stage that wrote the line during its own run would be exempting itself for free. The exemption
   is not from being measured, only from being measured by the code written: the stage must still have
   changed the change folder, having ticked the boxes it verified, so an implementer that silently
   failed is refused exactly as before (exit 3, `run-stage.sh:507`). The line has one legal value and
   any other stops the stage, because a plan naming a deliverable this loop cannot act on is not a plan
   to guess at. Every change that delivers code omits the line. What it does not bind is the plan
   review: `SPECS_SHA256:` covers `specs/` alone, so a planner may add the line after its verdict, and
   what that buys is an empty implement stage that the diff reviewer still has to approve as an empty
   diff, over a line sitting in the committed `proposal.md` for anyone to read.

   That makes the planner the only author of such a change, and therefore the one agent that cannot
   review its diff. `apply.sh` refuses a reviewer the `authors` ledger already names, before launching
   either role, rather than leaving it to the landing gate, which sees it at the commit with every
   agent already paid for.

   **`apply.sh` exits 10 rather than review the same bytes twice.** Before each review after the first
   it compares the tree it would hand the reviewer against the `TREE_SHA256:` of the previous round,
   read back from the round file so a restart still sees it, and stops without launching anything when
   they match. During #291 two rounds returned opposite verdicts on a byte-identical tree and the
   second one shipped. An implementer that answered every finding in prose lands here too, which is
   right: whether prose answered the findings is a person's call, and no round of review can make it.

   **Apply ends at implementation.** It does not commit, archive, sync deltas into
   `openspec/specs/`, or move the change folder. A checked box is a claim the next reader trusts
   instead of redoing the work, so check one only after performing it: a task saying to add an HTTP
   test is not satisfied by a unit test one layer below the status code. `openspec/config.yaml`
   (`operations.apply.guidance`) says the same to every agent.
5. **`/opsx:archive`**, always syncing every delta into `openspec/specs/`. Archive is advisory and
   will offer to skip the sync or accept unchecked tasks; both are forbidden here. Out-of-scope tasks
   get cut and filed as issues.
6. **Verify** with the three cargo gates, then one commit covering code, specs, and the archived
   change, with `Fixes #N`. Push the branch and wait for its run. `run-change.sh` does all of this and
   stops there: the merge into `main` is the one step a person approves, and by then it is mechanical.
   A gate failure gets the implementer one resumed round with the output, and a second failure stops
   the run, because a lint is what an unattended round should absorb and a second failure is a defect.

   **Every gate command is read-only** (#326). `cargo fmt` runs as `cargo fmt --check`, the spelling CI
   uses, so a gate reports a mis-formatted tree rather than repairing it. It has to: the gates run after
   the diff review has approved the tree and before the commit, so anything a gate writes lands having
   been reviewed by nobody, and the landing check on `TREE_SHA256:` is shape-only and would wave it
   through. Repairing formatting is the fix round's job, and that round has an author.

   **And that round is reviewed** (#328). It edits code after the diff review approved the tree and
   after archive, and it used to fall straight through to the commit: the approving `diff-review.md`
   described a tree that no longer existed. So `run-change.sh` records the digest the round left behind
   in `gate-fix.tree` in the change folder, and the code reviewer judges it before the commit; a
   `REVISE` stops the run (exit 11), because a gate fix is one unattended round on a lint and findings
   against it are the defect the second gate failure already stops for. The record is a file rather
   than this run's control flow on purpose: the fix can have happened in an earlier invocation whose
   review then stopped, and a check nested inside the branch that launched it would never fire again.
   It costs a reviewer launch on the failing path only, and only when the round actually changed
   something.

   **A failure that was already there is neither** (#298). A failing `cargo test` is measured against
   the commit the branch forked from: `.workflow/gates.sh` checks that commit out in a scratch
   worktree outside the repository, runs the suite there, and subtracts. Failures present in both
   predate the change, are named as such and do not stop the run; a failure absent at the base is this
   change's, is named, and does. `cargo fmt` and `clippy` are never measured this way, because they
   are deterministic and a pre-existing lint is not a thing this repo tolerates. The baseline runs
   only after the gates have already failed, so the passing path costs nothing, and it is cached
   against the commit it measured, so the fix round's re-run does not pay for it twice. It builds
   into `target/baseline`, never the target directory of the tree it is measuring: cargo does not key
   this package's artifacts on the tree they were built in, so a shared one lets each tree run the
   other's binaries, and the first thing that breaks is `env!("CARGO_MANIFEST_DIR")` (`src/errors.rs`
   reads `docs/SPEC.md` through it). That buys correctness for a cold build of every dependency, once
   per worktree. Anything the comparison cannot establish - no base commit, a suite that failed
   without naming a test, a failure set missing a target that died mid-run or a failure cargo
   counted, a baseline that would not build - counts as this change's, because a run waved through
   on an attribution nobody could make is worse than the false stop it replaces. During #235 that
   false stop threw away a reviewed, approved, archived change over 16 failures that fail
   identically at the base (#288).

### When a stage cannot decide something

Any stage may write `QUESTIONS.md` at its worktree root and stop rather than guess; the driver stops
with exit 8, and `/change` relays the questions and writes the answers to `ANSWERS.md` beside it. Every
stage prompt says to read that file first, so the answer reaches the stage that asked without anything
being threaded through. Both files are gitignored: a question is working state, and what it settled
belongs in the plan.

The bar is in the prompt: this is for a contradiction in what the stage was given, or a missing
decision that changes the contract. Anything a stage can decide, it decides and records. A stage that
asks instead of deciding has traded an hour of yours for a minute of its own; a stage that guesses at
the contract has buried something a later reader will trust.

`ANSWERS.md` is also the one place a person can steer a run that is already going, and it is worth
knowing what that costs. **Every stage reads it, the reviewer included**, and `worktree_digest` hashes
it into the tree (`run-stage.sh:374-382`), so editing it makes the next round a new round even when no
source moved: `apply.sh`'s exit-10 identical-bytes guard will not fire, and a fix round that wrote
nothing gets reviewed anyway.

So an instruction written there sets the reviewer's work as much as the author's, and the trap is
asking the author for **evidence the reviewer can only check by reproducing it**. During #213 five
rounds had each found one uncovered guard, so `ANSWERS.md` asked the implementer to sweep every guard
the diff added and report a table: `file:line`, the mutation, the test that fails under it. The author
did that for 232,937 tokens on a free model. The reviewer, whose fixed prompt says to verify each
finding against the actual code and not to rubber-stamp, then copied the UI tree to `/tmp` and re-ran
the whole matrix, one full `vitest` run per row, because a coverage claim is the one kind of claim
reading cannot check.

Ask for the work, not for a claim about the work: "write the missing tests and prove them by mutation"
lands the same code and leaves the reviewer a diff to read. If a claim table really is wanted, bound
the check in the same file, which the reviewer also reads, or get the evidence from a tool
(`cargo-mutants`, Stryker) whose output a reviewer can read instead of reproduce.

## What the gates check

Two scripts, run by `.githooks/pre-commit` and by CI, so no agent is judged differently from another.
They inspect files, never which tool produced them. Enable them once per clone with
`.workflow/setup-hooks.sh`.

A third, `.workflow/merge-shape-check.sh`, judges the commit's shape rather than its files and runs
only locally, from `pre-commit` and `pre-merge-commit` (#341). It refuses a merge anywhere but `main`,
because the two below read history through one base ref and a merge gives them two previous commits.
CI has no counterpart to it: `archive-merge-check.sh` exits 2 on the merge it cannot read, which
covers what lands, and a merge it *can* read is not something CI refuses.

They read file contents from the working tree rather than the index, so the hook first refuses a
commit whose `openspec/`, `src/` or `ui/src/` files differ between disk and what is staged. Otherwise
an unstaged fix would be judged in place of what is being committed, and CI, which sees only what
landed, would refuse what the hook allowed.

`.workflow/review-gate-check.sh` judges a change at two different points.

**Landing**, meaning the commit that carries the change's folder into
`openspec/changes/archive/`. Checked whatever the commit touches, because there is no later moment:
the plan verdict must pass with `AUTHOR:` and `REVIEWER:` differing, `specs/` must still match the
digest that verdict recorded, and `diff-review.md` must pass with a non-empty `AUTHORS:` list that its
`REVIEWER:` appears nowhere in, and a wellformed `TREE_SHA256:`.

That last field is checked for **shape only**, never against the committed tree, and the difference is
deliberate. Two stages write after the approving review: archive moves the folder and syncs
`openspec/specs/`, and the commit message runs after that. The committed tree is therefore never the
reviewed tree, so a match check would refuse every change. The value is compared where the failure it
guards against actually happens: round to round, live, in `apply.sh`.

The third writer used to be the gate fix, and it was the one that mattered, because it edits `src/`
(#328). It is now measured: a gate-fix round that changed the worktree records what it left behind in
`gate-fix.tree`, and the landing gate refuses a change whose approving `TREE_SHA256:` is not that
digest. So the one post-review stage that writes code cannot land unread, while the two that cannot
write code stay free to run. What this cannot see is a fix made outside the driver, which records
nothing; the shape-only rule is what covers the rest, and `docs/WORKFLOW.md` records it under what is
not guaranteed.

**In flight**, meaning a live folder under `openspec/changes/`. The plan checks apply, but only when
the commit touches `src/` or `ui/src/`, so the planning and review loop itself stays writable.

The digest is `SPECS_SHA256:`, written by `.workflow/specs-digest.sh <change-dir> --write` once the
review has a verdict, and recomputed by the gate. Only `specs/` is hashed: `proposal.md` and
`design.md` are context, and correcting a wrong sentence in them is free on purpose, because a rule
that charges a full re-review for a factual fix teaches you to leave the plan wrong. Re-running the
tool to launder a stale verdict is possible, and leaves a visible edit to `review.md` that a silent
edit to `specs/` never did.

`.workflow/archive-merge-check.sh` checks that `openspec/specs/` is the delta applied to the previous
commit: every requirement the delta names landed verbatim or is gone, and every requirement it does
not name is untouched. That second half is the point. Archive resolves `MODIFIED` by locating a
requirement *by name*, so a drifted name rewrites the wrong requirement silently, and the plan review
never saw `openspec/specs/` at all. It also refuses a commit archiving a delta for a capability it
never synced, which is the same rule read from the other side. This replaced a step asking whoever
archived to review the diff it had just produced (#218), which is a self-review, and could not fail.

`--plan-only` drops the diff-review check for callers that fire mid-implementation, when no diff
review can exist yet: `run-stage.sh`'s pre-flight probe and `.claude/hooks/review-gate.sh`, the
edit-time signal for Claude Code.

`.workflow/change-tests.sh` does the same for `run-change.sh` and the roles it drives: the self-review
and non-resumable refusals, every stage the resumption logic can resolve to, the guards in
`run-stage.sh` that a role change could silently unkey, the `DELIVERABLE: spec-only` exemption
from one of them, the question protocol, and which side of the base commit a failing gate belongs
to.

`.workflow/gate-tests.sh` asserts both scripts against a throwaway repo, mostly on the refusals: a
gate that stops firing looks exactly like a gate that passes, and both of these did that once during
development. It asserts the merge refusal through the real hooks, on both paths git splits a merge
commit across, because a hook asserted through a copy of its logic asserts the copy. `.workflow/apply-tests.sh` does the same for `apply.sh`'s change resolution, through
`--dry-run`, so no agent is launched. CI runs both. Change any of those scripts and run them.

**Exit 2 means the commit could not be judged**, and both gate scripts now have it (#333). A base ref
that does not resolve, a `git show` that fails on a path that is present, a commit naming a change
while `openspec/changes/` is absent: each used to reach the permissive branch, because a failure
nobody could see produced an empty requirement index, and an empty index reads exactly like a
capability that had nothing to displace. Every requirement then compared against nothing and the check
passed saying nothing at all. Existence is now asked of `git cat-file -e`, never inferred from `show`
failing, and every working file the check writes is checked.

The suites carry the other half, in `.workflow/suite-lib.sh`: around 250 fixture writes across the
three of them, not one of which was checked. A write that fails leaves a case asserting against a file
that was never written, and for a refusal case that reads as a gate which stopped firing, which is the
one signature `gate-tests.sh` exists to detect. So each `setup()` proves the fixture it just built is
on disk, and every assertion first proves the filesystem still takes a fixture-sized write. Either
failing ends the run with **exit 3** and no verdict, because a suite that cannot build what it asserts
against has nothing to report. `/tmp` here carries a per-user quota that `df` does not show, and under
it `gate-tests.sh` once returned 40 passed, 13 failed with two refusals that never fired.

The gates bound what they can see. A commit that skips OpenSpec entirely has no change folder, so
nothing is checked; `--no-verify` skips the hook. Both are recorded in `docs/WORKFLOW.md` under what
is not guaranteed.

[`docs/WORKFLOW.md`](docs/WORKFLOW.md) describes this loop for a human reader: what it guarantees
and when it stops for them. It carries no commands. Mechanics belong here, not there.

`tasks.md` is execution state for one accepted issue, never a backlog. That is what keeps the
"issues are the sole tracker" rule intact.

`openspec/config.yaml` (`context`, `rules.*`, `operations.*.guidance`) is what the `opsx` workflows
inject into each artifact. It restates these rules on purpose, so the workflow stands alone. Change a
process rule here and change it there too.

`openspec/schemas/labeler/` is a **fork** of the built-in `spec-driven` schema, so it does not inherit
upstream improvements. On a CLI upgrade, diff it against the new built-in and port what matters; the
command is in the schema's header comment. Its `review` artifact is adapted from the `anvil` community
schema by @jikkujoyce, minus the TDD stages.

The `openspec-*` skills and `opsx` commands under `.claude/`, `.agent/`, `.agents/`, `.opencode/` are
**generated** (43 files; the 24 `SKILL.md` manifests record `generatedBy: 1.9.0`). Never hand-edit
them. To upgrade: upgrade the CLI, `openspec update --force`, review all four trees, commit the
regeneration alone.

## Reviewing before you call it done

After implementation, spin up a **separate adversarial code reviewer** briefed to find real problems,
not to rubber-stamp. It audits the diff against the issue's acceptance criteria, correctness, edge
cases, tests, and this file.

**The reviewer never edits.** Its only output is findings, exactly as in the plan review. They go back
to whoever implemented, which fixes them; the reviewer then re-checks. That is what terminates the
loop: every edit has an author and a different reviewer, and a re-check is not an edit. A reviewer
that fixes what it found has produced a delta nobody reviewed, and the loop then ends only when
someone silently accepts unreviewed work.

The implementer addresses every meaningful finding, or justifies with file:line evidence why it is not
one. Re-review. Repeat until a pass surfaces no meaningful fixes.

Fluent code is not correct code: verify each finding against the actual code before accepting *or*
dismissing it. When the reviewer is **codex**, cap at **5** passes absent an unresolved blocking
issue. Converging on "no MAJOR issues" is the goal, not an empty findings list.

## Isolation: one change, one worktree, one issue

Every piece of work gets its own git **worktree**, not just a branch, and this one does not care
whether the work is an OpenSpec change: a branch shares one working directory, so two sessions
collide, and sessions here do run concurrently. An OpenSpec change adds a second reason, which is why
the rule started with them: its change folder is untracked until the final commit, so it follows you
across `git checkout` and makes "is a change in progress here?" unanswerable.

```bash
git worktree add .worktrees/issue-<N> -b issue-<N>-<slug>   # start
cd .worktrees/issue-<N>                                     # work here, only here
```

Need an unrelated hotfix while a change is in flight? Another worktree. Never switch branches inside
a change's worktree, and never carry one change's worktree into another's work.

`/.worktrees/` is gitignored. See `superpowers:using-git-worktrees`.

## Committing

Commit and push without prompting; do not wait to be asked. There are no pull requests, so the change
branch is the only place a change can be checked before it reaches `main`:

```bash
git rebase origin/main                  # only if main moved; never `git merge main`
git push -u origin issue-<N>-<slug>     # runs the checks; publishing stays bound to main
# once that run is green, from the repo root:
git merge --ff-only issue-<N>-<slug> && git push
git push origin --delete issue-<N>-<slug>
git worktree remove .worktrees/issue-<N> && git branch -d issue-<N>-<slug>
```

Do not merge on a red or absent branch run. CI on `main` is not a gate, it is a post-mortem: by the
time it fails, the commit is already integrated.

**A change branch rebases onto `main`; it never merges `main` into itself** (#341). A back-merge
records that a branch outlived `main` and nothing else, and 38 of the 163 merges on `main` are exactly
that, their whole message being `Merge remote-tracking branch 'origin/main'`. It also breaks the one
check that reads history: `archive-merge-check.sh` asks whether `openspec/specs/` is the delta applied
to *the* previous commit, and a merge has two, so it reports one parent's correctly archived work as a
hand-edit. The hooks refuse the shape rather than leaving this to memory, and it takes two of them:
git runs `pre-merge-commit` for a merge it resolved itself and never `pre-commit`, and `pre-commit` for
one that conflicted and is committed by hand. `.workflow/merge-shape-check.sh` is the one spelling
both call.

Integration is `--ff-only`, which after a rebase always succeeds and leaves no bubble. Of the last 30
issue branches merged, 24 held one commit and 6 held two, so the bubble was wrapping a single commit.
`--no-ff` stays for a branch whose boundary says something, which is what the milestone merges did.

**Never rewrite `main`, or any ref another session consumes.** That is the whole scope of the rule, and
a change branch is outside it: it is pushed to earn one run, nothing is based on it, and it is deleted
at integration. Rebase it and push with `--force-with-lease --force-if-includes`, never a bare
`--force`, so a push that would discard something you have not seen fails instead.

Rebase *before* the diff review wherever `main` has already moved, so the tree the reviewer approved is
the tree that lands. A rebase after that review is a post-review write to `src/` with an author and no
reviewer, and is measured as one (#342).

## Commands

```bash
LABELER_CONFIG_DIR=./config-dev cargo run   # needs a writable config dir; /config is not one
cargo test                                  # unit + HTTP integration
cargo fmt
cargo clippy --all-targets --all-features
```

`config-dev/` is gitignored and created on first run.

**Before reporting any change**, run `cargo fmt`, `cargo clippy --all-targets --all-features`, and
`cargo test`. Never silence a lint with `#[allow(clippy::...)]`; fix the root cause.

`rust-toolchain.toml` pins the compiler those gates run on, so a local pass and a CI pass mean the
same thing (#186). rustup installs the pinned toolchain on first `cargo` call; you need do nothing.
It is **not** an MSRV: it says "build with this", not "this is the oldest compiler we support", and
`Cargo.toml` declares no `rust-version`. One thing silently beats it: a per-directory
`rustup override set` from an earlier session. If your gate results stop matching CI, run
`rustup override unset` in the repo root and `rustc --version` to confirm.

Nothing bumps the pin for you. Dependabot has no updater for the file, which is the point: a new
stable can add a lint, and here that arrives as a deliberate commit with its fallout attached rather
than as a red build on someone else's PR. To bump: edit `channel`, run the three gates, fix what the
new toolchain flags, and commit the bump on its own.

For non-trivial changes, web-search first to confirm current API behavior, especially for Typst, axum,
and utoipa, whose APIs shift between versions.

`scripts/render_avery_sheet.sh` exercises the batch endpoint end-to-end against a running server.

## Templates are visual artifacts

A YAML edit that parses and renders without error is not proof the label looks right. Use a
render → inspect → fix loop: render to PNG (`POST /api/render/label?format=png`, with
`LABELER_NO_AUTH=true` locally), **open the image** and check it against intent (QR squareness, text
inside the printable area, alignment, auto-shrink, no clipping), fix the YAML, and re-render
(`POST /api/templates/reload` picks up edits without a restart). Stop when it is correct, not when it
merely renders. See #67.

**Nothing checks this, and no task should claim it.** The loop runs against a running server and a
config dir outside the repository, so its only evidence is an image no later reader can retrieve. A
checked box over it would be a claim nobody can verify and no gate can refuse, which is worse than an
honest gap, so the box is gone (#220). Template correctness rests on whoever edits the template. The
nine YAML files under `tests/fixtures/templates/` are a different thing: they are test inputs, and
what makes them right is the test that reads them.

## Architecture

Request path `api.rs → render/`; template path `templates.rs → parse.rs → raw.rs → convert.rs`.

- **Two-stage parsing.** YAML deserializes into `raw.rs` structs (all `deny_unknown_fields`), then
  converts to the domain model via `TryFrom` in `convert.rs`, with `serde_path_to_error` attaching a
  JSON path to every error. This lets the wire format differ from the validated model. *Adding a
  layout field means editing three files together: `raw.rs`, `models.rs`, `convert.rs`.*
- **Template registry.** Loaded and `validate()`d at startup (`main.rs`). A template that fails to
  parse or validate is **quarantined** and the server still starts (#175); so is a file whose id is
  already taken, with the lexicographically first filename keeping the id (#181, ADR-0058). No
  template content is fatal. Nothing is seeded into a fresh config dir. Templates are immutable,
  shared via `Arc`.
- **Layout model** (`models.rs`). `layout` is a tree of `LayoutItem`s: `Text`, `Qr`, `Image`, `Line`,
  `Container`. `Container` nests `items` recursively and may carry `shape`, `stroke`, `background`,
  `rounded`, `padding` and `flow`. Any item may carry `when:`, the universal conditional-visibility
  predicate over `params` (ADR-0056, #162).
- **Coordinates.** Bottom-left origin, y-up, in the template `unit` (`mm` or `in`). Typst is top-left,
  so the renderer flips with `frame_height_units - top`. A `Container` re-bases children into its
  padded inner box via a fresh `RenderContext`. *Watch this when touching placement math.*
- **Sizing** (`resolver.rs`). An extent is a number, `content` or `fill`, and comes from one of three
  sources: the author, the content, or the frame. `source_of` is the only place a spelling is
  classified; everything downstream branches on that classification, never on the spelling itself.
  `resolve`, `available` and `requirement` are shared by load-time validation and render-time
  resolution and cannot tell which stage they are in, so the two cannot drift the way they did in
  #150 and #155. Only the walk supplying intrinsic sizes differs, because load cannot measure text,
  encode a QR or decode an image: it passes the available extent instead, which makes a `content`
  extent resolve exactly as a `fill` one does. *Adding a source or a bound means editing
  `resolver.rs` alone.* (ADR-0080, ADR-0081, #226.)
- **Rendering** (`render/mod.rs`). Walks the layout emitting Typst markup; PNG via `typst-render`,
  sheets as one clipped box per slot via `typst-pdf`. `render/helpers.rs` holds string escaping,
  length formatting, QR-SVG generation (`qrcode`), and `ttf-parser`-based text fitting for
  `font_size: {min, max}` (auto-shrink plus ellipsis truncation).
- **Errors.** `TemplateError` (parse/validation, carries a path) quarantines rather than aborting
  startup. `AppError` is the HTTP error, serializing to `{ "error": { code, message, details } }`. Add
  new kinds as `AppError` constructors so `code` strings stay stable.
- **OpenAPI.** Every model exposed in the API must be registered in `src/openapi.rs`.

## Notes

- `CLAUDE.md` is a symlink to this file, so the two names are one file. Put personal,
  machine-specific instructions in `CLAUDE.local.md` instead; it is gitignored and loads alongside.
- Do not delete `openspec/specs/.gitkeep` or `openspec/changes/archive/.gitkeep` while those
  directories are empty.
- Fonts: Inter loads via `typst-kit` from `fonts/InterVariable.ttf`; Typst is told to use
  `"Inter Variable"`/`"Inter"`.
