# Making a change

How a behavior change goes from an idea to `main`. The rules live in
[`AGENTS.md`](../AGENTS.md); this is the operating manual.

The loop is **unattended on the converging path**. You describe what you want; the agent runs
everything below and pushes. You are pulled in exactly once, and only on failure: when the
adversarial review cannot reach a verdict in two rounds.

## Who runs what

The rule that makes this work: **whoever wrote an artifact never reviews it.** Splitting the roles
across models satisfies that by construction rather than by good intentions.

| Step | Runs on | Why that one |
| --- | --- | --- |
| Propose: `proposal.md`, `specs/`, `design.md` | Claude | Authoring, with repo context |
| Review the plan | codex | Must differ from the author |
| Apply: write the code | Gemini via `agy` | Author of the implementation |
| Review the diff | Claude | Differs from the implementer |
| Archive, verify, integrate | Claude | Mechanical |

If a second model is unavailable, a fresh-context subagent on the same model is the fallback. Never
"now critique what you just wrote" in the authoring context; that is not a review.

## The loop

### 1. Issue and worktree

```bash
gh issue create --title "..." --body "..."
git worktree add .worktrees/issue-<N> -b issue-<N>-<slug>
cd .worktrees/issue-<N>
```

One change, one worktree, one issue. Everything below happens inside that directory, and it is never
`git checkout`ed. An unrelated hotfix during a change gets its own worktree.

### 2. Propose

```
/opsx:propose <what to build, referencing issue #N>
```

Writes `openspec/changes/issue-<N>-<slug>/`. Planning only: it must not touch code.

For behavior documented only in the frozen `docs/SPEC.md`, the delta is an **`ADDED`** requirement
carrying the complete post-change contract and naming the section it supersedes. A `MODIFIED` delta
against an unmigrated section is unresolvable, because there is nothing in `openspec/specs/` to
modify.

The propose skill advertises "all artifacts in one step", which now includes `review`. It must not
write that one. If `review.md` appears naming Claude as reviewer, discard it and do step 3 properly.

### 3. Review the plan

Judges `proposal.md` + `specs/` + `design.md` together, before a single task is written.

```bash
codex exec --ignore-user-config -s read-only -c model_reasoning_effort=high \
  "$(openspec instructions review --change issue-<N>-<slug>)" \
  < /dev/null > openspec/changes/issue-<N>-<slug>/review.md
```

`< /dev/null` is mandatory. Without it codex blocks reading stdin forever and never calls the API,
which looks identical to thinking hard. A stalled run has ~0 CPU and no TCP connections:

```bash
P=$(pgrep -f "codex exec" | tail -1)
ps -o etime,time,%cpu -p $P     # working: CPU climbing.  stalled: ~0:00.10
lsof -p $P | grep -c TCP        # working: 3-5.           stalled: 0
```

`review.md` ends with exactly one canonical line, `VERDICT:` followed by one of:

| Verdict | Then |
| --- | --- |
| `APPROVE` | Continue to tasks |
| `APPROVE_WITH_CHANGES` | Apply the numbered Required Changes, reviewer re-checks **only those**, set `CHANGES_APPLIED: yes` |
| `REVISE` | Fix the artifacts, re-run the **full** review in a fresh context |

Rules that stop the loop being gamed:

- An open Critical finding **forbids** `APPROVE`. A review listing Criticals and approving is invalid.
- Editing `proposal.md`, `specs/` or `design.md` after a verdict **voids** it, unless the edit is
  applying that review's own Required Changes.
- Rebuttals are not self-certifying. Rebutting a Critical or Moderate counts only once the reviewer
  marks it accepted. Suggestions the author may decline alone.
- A reviewer that errors, stalls, or emits garbage does **not** fall back to self-review and does not
  get a fabricated verdict. Drop to a fresh-context subagent, or stop.

**Escalation.** Two consecutive `REVISE` rounds is a hard stop. The agent does not implement and does
not keep retrying: it surfaces `review.md` and the artifacts and waits. This is the one place you are
asked to read anything, and it means the reviewer and the author could not converge.

### 4. Apply

```
/agy implement the tasks in openspec/changes/issue-<N>-<slug>/tasks.md
```

`agy` is autonomous: it edits files, runs git, and can push. Run it only inside the worktree, and only
once the verdict passes.

If Claude implements instead, `.claude/hooks/review-gate.sh` refuses writes to `src/` and `ui/src/`
until the verdict passes. **That hook only sees Claude Code's tool calls**, so it does not constrain
`agy`. Until the CI check in #188 lands, the verdict must be confirmed before handing work to another
CLI.

Then review the diff. A second, separate review: step 3 judged the plan, this judges the code. Verify
every finding against the actual code with file:line evidence before accepting *or* dismissing it.
Fluent code is not correct code.

### 5. Archive

```
/opsx:archive
```

Always sync every delta into `openspec/specs/`. Archive is advisory and will offer to skip the sync
or accept unchecked tasks; both are forbidden. Out-of-scope tasks get cut and filed as issues rather
than parked unchecked.

Then read the resulting `openspec/specs/` diff. Archive rewrote it after the last review pass, so
nobody has looked at it yet.

### 6. Verify and integrate

```bash
openspec doctor
openspec validate --all --strict --no-interactive
openspec validate --archived --no-interactive
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings   # as CI runs it
cargo test
```

`-- -D warnings` matters: without it clippy exits 0 on findings that fail CI.

```bash
cd <repo root>
git merge issue-<N>-<slug> && git push
git worktree remove .worktrees/issue-<N>
git branch -d issue-<N>-<slug>
```

One commit covering code, ADR, main specs and the archived change, with `Fixes #N` so the issue closes
on push. No pull requests. Never force-push.

## What actually enforces the gate

| Layer | Enforces | Bypassable |
| --- | --- | --- |
| `requires:` DAG | Artifact ordering, and only that files *exist* | Yes, contents are never read |
| `apply` instruction | The verdict gate | Yes, it is instruction text |
| `review-gate.sh` | Writes to `src/`, `ui/src/` | **Claude Code only**; other CLIs unaffected |
| CI | Nothing yet | Tracked in #188 |

Only the hook and CI are real. The schema supplies ordering and the brief.

## Maintenance

`openspec/schemas/labeler/` is a **fork** of the built-in `spec-driven` schema, so it does not inherit
upstream improvements. On a CLI upgrade, diff it against the new built-in and port what matters; the
command is in the schema's header comment. Its `review` artifact is adapted from the `anvil` community
schema by @jikkujoyce, minus the TDD stages.

`requires:` does two jobs. Besides ordering, it populates the `<dependencies>` read-list injected into
an artifact's instructions, and that list is **literal, not transitive**. Do not prune a
"redundant" edge without checking what the artifact must read.
