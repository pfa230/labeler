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

`docs/adr/` holds append-only Nygard ADRs. Every behavior change adds or supersedes an ADR and adds
its row to `docs/adr/README.md`, in the same change. Supersede rather than edit.

Also in `docs/`: `WORKFLOW.md` (how changes get made, for humans), `AUTHORING.md` (template model by worked example), `VISION.md`, `DEPLOY.md`.

## Tracking work

GitHub issues and milestones are the sole live tracker. No markdown TODOs, no roadmap docs. File with
`gh issue create` and reference from commits (`Fixes #12`). Work you won't do now becomes an issue,
never a TODO in code or docs, and never an unchecked task in `tasks.md`.

The superpowers `writing-plans` step is retired here (ADR-0057); the OpenSpec change folder is the
plan, and it is committed. `superpowers:brainstorming` still helps for fuzzy ideas before a change
exists; its scratch stays under `docs/superpowers/` (gitignored).

## OpenSpec workflow

### Which changes go through it

Behavior changes, and nothing else. The discriminator is the **spec delta**, and it needs no
declaring: a behavior change always produces one, because the first-touch rule makes the first change
to any documented behavior write the complete post-change contract as an `ADDED` requirement. A change
with no delta has no contract to review, and the loop below has nothing to gate.

So a documentation fix, a workflow script, a CI change, a dependency bump or a refactor that keeps
behavior identical goes: issue, worktree, the three gates, one commit with `Fixes #N`, push, merge. No
change folder, no plan review, no `diff-review.md`. Nothing else relaxes: it still starts as an issue
and still ends as one commit that closes it.

Size decides nothing. A nine-line handler check that alters behavior is a full change; a five-hundred
line documentation rewrite is not. There is no lane to declare, no criteria to qualify under and no
step that promotes one to the other. Writing a delta is what makes a change one, and
`review-gate-check.sh` demands a passing `review.md` from the moment a change folder exists, so
discovering mid-work that you need a delta costs the review and nothing else.

What no gate can decide is whether a diff *should* have carried a delta. A commit with no change
folder is checked by nobody, which `docs/WORKFLOW.md` records under what is not guaranteed.

### The loop

OpenSpec (CLI 1.9.0) on this project's own schema, `openspec/schemas/labeler/`:
`proposal → specs → design → review → tasks → apply`. Order matters, because the review gates
implementation and archive rewrites the main specs *after* it:

1. **Issue**, then a worktree: `git worktree add .worktrees/issue-<N> -b issue-<N>-<slug>`.
2. **`/opsx:propose`** writes `openspec/changes/issue-<N>-<slug>/`. Planning only; it must not touch
   code. Link the issue in `proposal.md`.
3. **Adversarial review of the plan**, before any task is written: the `review` artifact, judging
   `proposal.md` + `specs/` + `design.md`. A second model in read-only mode, else a fresh-context
   subagent. **Never** self-review inside the authoring context. It writes `review.md` ending in a
   `VERDICT:` line. `REVISE` → fix the artifacts and re-run the *full* review in a fresh context;
   `APPROVE_WITH_CHANGES` → apply the listed edits, reviewer re-checks only those, then set
   `CHANGES_APPLIED: yes`. Editing `specs/` afterwards voids the verdict, and the gate detects it;
   editing `proposal.md` or `design.md` does not, because they are context and not the contract.

   **This is the only place a human enters the loop, and only on failure.** Three consecutive `REVISE`
   rounds is a hard stop: do not implement, do not keep retrying. Surface `review.md` and the
   artifacts, and wait. On the converging path the loop runs unattended through to the merge.
4. **Apply and review the diff**, as a named pair:
   `.workflow/apply.sh <implementer> <reviewer> [change]`, or `/apply` with the same arguments. The
   pair is named first because it is the guarantee; the change is last and optional, resolved from the
   worktree you are standing in, or from the single one in flight across `.worktrees/`, and refused
   rather than guessed when several are. `--rounds N` moves the three-round cap and `--dry-run` shows
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
   record, and the reviewer's stdout redirected into them *is* its output rather than a summary of
   it, so there is nothing left to preserve alongside. An earlier convention committed the raw
   `codex exec` capture next to the review, banner and session id included; 19 such files reached
   47,190 lines, against 893 lines of actual planning record in the worst change, and they are gone
   (#244).

   `apply.sh` records the outcome as `diff-review.md` in the change folder, carrying `AUTHOR:`,
   `REVIEWER:` and `VERDICT:`, with each round kept alongside as `diff-review-<n>.md`. That file is
   what the gate reads, so a verdict living only in a transcript is a verdict nothing can check.

   **Apply ends at implementation.** It does not commit, archive, sync deltas into
   `openspec/specs/`, or move the change folder. A checked box is a claim the next reader trusts
   instead of redoing the work, so check one only after performing it: a task saying to add an HTTP
   test is not satisfied by a unit test one layer below the status code. `openspec/config.yaml`
   (`operations.apply.guidance`) says the same to every agent.
5. **`/opsx:archive`**, always syncing every delta into `openspec/specs/`. Archive is advisory and
   will offer to skip the sync or accept unchecked tasks; both are forbidden here. Out-of-scope tasks
   get cut and filed as issues.
6. **Verify**, then one commit covering code, ADR, specs, and the archived change, with `Fixes #N`.

## What the gates check

Two scripts, run by `.githooks/pre-commit` and by CI, so no agent is judged differently from another.
They inspect files, never which tool produced them. Enable them once per clone with
`.workflow/setup-hooks.sh`.

They read file contents from the working tree rather than the index, so the hook first refuses a
commit whose `openspec/`, `src/` or `ui/src/` files differ between disk and what is staged. Otherwise
an unstaged fix would be judged in place of what is being committed, and CI, which sees only what
landed, would refuse what the hook allowed.

`.workflow/review-gate-check.sh` judges a change at two different points.

**Landing**, meaning the commit that carries the change's folder into
`openspec/changes/archive/`. Checked whatever the commit touches, because there is no later moment:
the plan verdict must pass with `AUTHOR:` and `REVIEWER:` differing, `specs/` must still match the
digest that verdict recorded, and `diff-review.md` must pass with its own two roles differing.

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

`.workflow/gate-tests.sh` asserts both scripts against a throwaway repo, mostly on the refusals: a
gate that stops firing looks exactly like a gate that passes, and both of these did that once during
development. `.workflow/apply-tests.sh` does the same for `apply.sh`'s change resolution, through
`--dry-run`, so no agent is launched. CI runs both. Change any of those scripts and run them.

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
git push -u origin issue-<N>-<slug>     # runs the checks; publishing stays bound to main
# once that run is green, from the repo root:
git merge issue-<N>-<slug> && git push
git push origin --delete issue-<N>-<slug>
git worktree remove .worktrees/issue-<N> && git branch -d issue-<N>-<slug>
```

Do not merge on a red or absent branch run. CI on `main` is not a gate, it is a post-mortem: by the
time it fails, the commit is already integrated. Never rewrite history that has been pushed.

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
  `Container`. `Container` nests `items` recursively and may carry `frame` and `padding`. Any item may
  carry `when:`, the universal conditional-visibility predicate over `params` (ADR-0056, #162).
  Write new templates with `params` + `when`, but note the legacy top-level `options:` map and
  `container.option` still **parse**: they desugar into an enum `params` entry and into `when`
  (`convert.rs:284`, `convert.rs:107`). Do not treat a template using them as invalid.
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
