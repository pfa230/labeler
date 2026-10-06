# How changes get made

Labeler uses [OpenSpec](https://github.com/Fission-AI/OpenSpec) to take a behavior change from an accepted GitHub issue through a written contract, implementation and archive. Every change, with or without a contract, lands as one commit that a person approves before it is fast-forwarded into `main`.

Agent rules live in [`AGENTS.md`](../AGENTS.md). The root npm manifests pin the OpenSpec CLI; `openspec/config.yaml` selects the built-in `spec-driven` schema and carries labeler's project rules.

## Which work needs a change folder

Issues and milestones are the only backlog. One change implements one accepted issue; work found outside that scope becomes another issue.

A change folder is for labeler's behavior: its API, template schema, layout model, coordinates and error contract. Each such change writes a spec delta. Corrections to published `openspec/specs/` also arrive as a delta, even when no code is needed; the proposal states that the deliverable is spec-only. Published specs are written by archive, never edited by hand.

Harness changes, documentation fixes, CI changes, dependency updates and behavior-preserving refactors take the direct path: issue, isolated worktree, implementation, relevant checks, one commit with `Fixes #N`, human approval, fast-forward merge and push. Harness paths include the root npm manifests, agent skills, `AGENTS.md`, this file and `openspec/config.yaml`. If work changes labeler behavior, its spec delta makes it a change regardless of size.

## Setup

Install the pinned CLI from the repository root and invoke it through `npx`, never a global install:

```bash
npm ci
npx --no-install openspec --version
```

## Running a change

1. Create the change's worktree from an up-to-date `main`: `git worktree add .worktrees/issue-<N> -b issue-<N>-<slug> origin/main`. Work only there.
2. Propose with `/opsx:propose` (skill `openspec-propose`). It writes `proposal.md`, the delta specs, `design.md` and `tasks.md` under `openspec/changes/<name>/`. The proposal contains literal `Fixes #N`. Revise with `/opsx:update`.
3. A person reviews the plan before implementation starts.
4. Implement with `/opsx:apply`, checking each task only after performing it.
5. Run every gate the diff touches (see `AGENTS.md`) and `npx --no-install openspec validate --all --strict`.
6. Archive with `/opsx:archive`. It syncs the deltas into `openspec/specs/` and moves the folder under `openspec/changes/archive/`.
7. Commit the implementation, the published specs and the archived folder as one commit with `Fixes #N`.

## Integration

A person approves the merge into `main`. With approval, from the default-branch checkout:

```bash
git merge --ff-only <change-branch>
git push
git worktree remove .worktrees/<dir>
git branch -d <change-branch>
```

If `main` moved, rebase the change branch onto it and rerun the gates before asking for approval. A failed fast-forward never authorizes merging `main` into the change or rewriting `main`.

## Checks and their limits

CI runs the Rust and UI jobs, then `openspec validate` over specs, open changes and the archive. Publishing requires the Rust and UI jobs, so a broken commit on `main` ships nothing until it is fixed forward. Gates must not repair source as a side effect; a formatting fix is an edit like any other.

These boundaries still need judgment:

- No check decides whether a diff should have included a behavior contract. An omitted spec delta is still an error.
- No tool reviews the plan or the implementation. That review is a person's.
- A successful render does not prove a label looks right. Template edits require visual inspection against a running server; no checkbox substitutes for that judgment.
- The baseline in `docs/SPEC.md` remains authoritative until a requirement under `openspec/specs/` explicitly names and supersedes its section.

## Where things live

| Record | Location |
| --- | --- |
| Current behavior, frozen baseline | `docs/SPEC.md` |
| Current behavior, subsequent contracts | `openspec/specs/` |
| Why a change was made | `proposal.md` and `design.md` under `openspec/changes/archive/` |
| Decisions predating OpenSpec | `docs/adr/`, frozen and never extended |
| Template authoring guide | `docs/AUTHORING.md` |
| Agent rules | `AGENTS.md` |
| Project rules for OpenSpec artifacts | `openspec/config.yaml` |
