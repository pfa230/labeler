# How changes get made

Labeler uses the published [openspec-loop](https://github.com/pfa230/openspec-loop) CLI to take a
behavior change from an accepted GitHub issue through planning, independent review, implementation,
tests and archive. Delivery leaves one local commit; a person approves its fast-forward merge into
`main`.

Agent rules live in [`AGENTS.md`](../AGENTS.md). Project configuration is `.openspec-loop.yml`;
the root npm manifests pin the tool to 0.1.0. The repository no longer vendors the loop implementation.

## Which work uses the loop

Issues and milestones are the only backlog. One change implements one accepted issue; work found
outside that scope becomes another issue.

The loop applies to labeler's behavior: its API, template schema, layout model, coordinates and error
contract. Each such change writes a spec delta. Corrections to published `openspec/specs/` also use a
delta and the loop, even when no code is needed; the brief and plan state that the deliverable is
spec-only. Published specs are written by archive, never edited by hand.

Harness changes, documentation fixes, CI changes, dependency updates and behavior-preserving
refactors take the direct path: issue, isolated worktree, implementation, relevant checks, one commit
with `Fixes #N`, human approval, fast-forward merge and push. They need neither a change folder nor a
plan review. Harness paths include `.openspec-loop.yml`, root npm manifests, agent skills,
`AGENTS.md`, this file and `openspec/config.yaml`. If work changes labeler behavior, its spec delta
puts it through the loop regardless of size.

## Setup

Install the pinned tool from the repository root:

```bash
npm ci
npx --no-install openspec-loop config
```

Agent execution requires Node.js 20.19 or later, Git, Linux with Bubblewrap and unprivileged user
namespaces, `ai-jail` 2.2.0 on `PATH`, and authenticated CLIs for the configured agents. The `change`
skill also uses `gh` to read the accepted issue. The npm package brings its required OpenSpec CLI.

`.openspec-loop.yml` declares implementation and frozen paths, setup and test commands, revision
limits and delivery policy. Machine-specific roles and instances live in the gitignored
`.openspec-loop.local.yml`, which also accepts cleanup overrides. Configure all four roles before
running. Authors and reviewers must be different named instances.

The `openspec-loop` schema is copied into `openspec/schemas/openspec-loop/` and selected in
`openspec/config.yaml`; the npm package does not ship it. Labeler's behavior and authoring rules stay
in that config. The package's `change` skill is copied into `.agents/skills/change/` and
`.claude/skills/change/`. Update these copies with the dependency when upgrading.

The package installs no Git hooks. Existing clones can still have the retired harness selected by
`core.hooksPath`. Inspect it with `git config --local --get core.hooksPath` and unset it with
`git config --local --unset core.hooksPath` once this migration is integrated and no legacy worktree
still needs those hooks. The setting is shared across worktrees; do not unset it while another
session is still using the old workflow.

## Running a change

Invoke `change <issue#>` using the installed skill from a clean default-branch checkout at
`origin/main`. The skill reads the issue and resolves scope questions before launching. It supplies
the issue's requirements in a brief, because the runtime does not fetch GitHub issue contents.

For a prepared brief, the equivalent CLI entry point is:

```bash
npx --no-install openspec-loop run --change issue-181-duplicate-template-id \
  --input /absolute/path/to/brief.md
```

The CLI creates a branch and worktree under `.worktrees/<change>`, commits the brief, writes and
reviews the plan, generates tasks from the approved plan, implements, tests and reviews the result.
It revises rejected work within the configured limit, archives approved changes and checks delivery.
Stages commit their output as they proceed. The final squash combines those commits; work is not
held uncommitted until archive.

The plan is `proposal.md`, `design.md` and the delta specs. Tasks follow plan approval, so rejected
scope does not become an implementation checklist. Reviews are numbered files under `reviews/`,
with `propose-*.md` for plans and `implement-*.md` for implementations. The CLI records evidence
against committed subjects and fork points; do not hand-author these records or reuse the old
`review.md` and `diff-review.md` format. Archived historical records remain historical evidence.

Use the complete run for normal work. The CLI also exposes individual stages, whose syntax is
documented by `npx --no-install openspec-loop --help`; old shell-dispatcher commands no longer apply.

## Stops and recovery

Unresolved decisions are saved in the change's `questions.md`. Answer them using the printed
continuation command from its named worktree. Do not use `--assume` to bypass a decision when
operating through the `change` skill.

An interrupted run preserves its work. After resolving the reported cause, resume from the change
worktree:

```bash
npx --no-install openspec-loop run --resume
```

Resume reads committed state. Editing the plan invalidates its approval and requires another
review. Review exhaustion, failing tests and refused checks stop the run; test repair follows the
configured revision limit. Operational failures also stop rather than silently changing agents or
delivery policy. Read the reported diagnostic and targeted invocation log before continuing.

For `run`, exit `0` means delivery succeeded, `1` means a decision, revision limit, test failure or
refused check stopped it, and `2` means an operational error. `--json` exposes the result while
diagnostics and progress go to stderr. The `change` skill reports a stopped run and waits for an
explicit continuation instruction.

## Delivery and integration

Labeler sets `delivery.mode: commit` and `delivery.squash: true`. The CLI squashes the stage commits,
runs landing checks and leaves the branch local without pushing. Successful delivery removes its
worktree by default and retains the branch; `cleanup: false` in local configuration keeps the
worktree. Report the delivered commit and branch and leave the issue open until integration.

A person approves the merge into `main`. With approval and an up-to-date, checked branch, run from
the default-branch checkout:

```bash
git merge --ff-only <change-branch>
git push
git branch -d <change-branch>
```

Remove any retained worktree before deleting its branch. Nothing was pushed under the change branch
name, so there is no remote branch to delete. The loop generates its squash message from the change
name and proposal text (`openspec-loop` 0.1.0, `src/commands/landing.ts`). The proposal must contain
literal `Fixes #N`, as required by `openspec/config.yaml`, so integration closes the issue. Manual
commits follow the message conventions in `AGENTS.md`.

Commit delivery does not rebase automatically for a later manual merge, and the CLI's `rebase`
command is restricted to merge delivery. If `main` moves, stop integration and arrange a manual
rebase with renewed review evidence and checks before requesting merge approval. A failed
fast-forward never authorizes merging `main` into the change, bypassing checks or rewriting `main`.

## Checks and their limits

The CLI checks review evidence and archive output before delivery. CI installs the pinned package
with `npm ci` and runs `openspec-loop check` over the committed range. It establishes the default
branch ref and uses the event's actual base; review checks need full Git history.

Agents execute through `ai-jail`: reviewers receive a read-only checkout and producers can edit only
their stage's permitted paths. Approval binds to the committed candidate; a changed subject or fork
point requires current evidence. The supplied schema requires an approving plan review before tasks
and implementation. Conditional approvals follow the configured policy and must record their
required changes as applied.

The Rust and UI jobs remain the product checks. Run the relevant commands in `AGENTS.md` for manual
work; loop runs use the declared test commands. Gates must not repair source as a side effect.
Formatting fixes are edits and must be reviewed as part of the candidate.

These boundaries still need judgment:

- No check decides whether a diff should have included a behavior contract. Direct harness work has
  no plan to review, but an omitted product spec is still an error.
- A successful render does not prove a label looks right. Template edits require visual inspection
  against a running server; no unverifiable checkbox substitutes for that judgment.
- There are no pull requests or pre-integration branch CI runs in the normal delivery path. CI on
  `main` runs after integration. Publishing requires the Rust and UI jobs, so a failed build does
  not ship.
- The baseline in `docs/SPEC.md` remains authoritative until a requirement under `openspec/specs/`
  explicitly names and supersedes its section. The new location does not silently replace it.

## Where things live

| Record | Location |
| --- | --- |
| Current behavior, frozen baseline | `docs/SPEC.md` |
| Current behavior, subsequent contracts | `openspec/specs/` |
| Why a change was made | `proposal.md` and `design.md` under `openspec/changes/archive/` |
| Review evidence | The change's `reviews/` directory |
| Decisions predating OpenSpec | `docs/adr/`, frozen and never extended |
| Template authoring guide | `docs/AUTHORING.md` |
| Agent rules | `AGENTS.md` |
| Loop configuration | `.openspec-loop.yml` |
