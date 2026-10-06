# Labeler

A stateless label-rendering REST service (Rust/axum). It loads YAML label templates from
`{LABELER_CONFIG_DIR}/templates/` and renders a single label to PNG or a batch to PDF/ZIP, by
generating [Typst](https://typst.app/) source on the fly and compiling it in-process via
`typst-as-lib`.

## Simplicity

- **Minimal by design (KISS).** Every mechanism, option, field and check must serve a concrete need of the accepted issue. Do not add speculative configuration, plugin points, extra layers, silent fallbacks, or generality for problems nobody has hit. When two designs both work, use the one with fewer moving parts.
- **Cut before presenting.** After the last artifact or edit and before any summary, run a cut pass. List each mechanism, option, check, field and task on its own line, and name the spec requirement it serves, as the change amends it, or the issue's explicit request where no spec applies, such as docs or tooling. Remove whatever names neither, and whatever duplicates what the repository already has. For each item that stays, name its simpler version and the concrete failure that version causes; when there is none, use the simpler version. The presentation includes the cut list: what was removed, and why each kept item stayed.

## Isolation: one change, one worktree, one issue

Every piece of work gets its own git **worktree**, not just a branch, and this one does not care what
kind of work it is: a branch shares one working directory, so two sessions collide, and sessions here
do run concurrently. Create one explicitly:

```bash
git worktree add .worktrees/issue-<N> -b issue-<N>-<slug>   # start
cd .worktrees/issue-<N>                                     # work here, only here
```

Need an unrelated hotfix while a change is in flight? Another worktree. Never switch branches inside
a change's worktree, and never carry one change's worktree into another's work.

`/.worktrees/` is gitignored.

## Tracking work

GitHub issues and milestones are the sole live tracker. No markdown TODOs, no roadmap docs. File with
`gh issue create` and reference from commits (`Fixes #12`). Work you won't do now becomes an issue,
never a TODO in code or docs, and never an unchecked task in `tasks.md`.

**A claim you did not earn is worse than an admitted gap.** A checked box, a ticked criterion, a
"verified" in a report: each is a claim the next reader trusts instead of redoing the work. Make one
only after performing the thing, and never write a step whose completion nothing can show. A task
saying to add an HTTP test is not satisfied by a unit test one layer below the status code.

**A transcript belongs in a log, not in this context and not in the repository.** Run artifacts go to
`.agent-runs/` at the worktree root. `.gitignore` excludes `.agent-runs/`, so a `git add -A` stages the work and
nothing else. Untracked was not enough: it left every commit depending on whoever ran it noticing the
dotfiles (#255). An earlier convention committed raw agent captures into the repository, and 19 of
them reached 47,190 lines against 893 lines of actual record in the worst case (#244).

The superpowers `writing-plans` step is retired here (ADR-0057); the change folder is the plan, and it
is committed. `superpowers:brainstorming` still helps for fuzzy ideas before a change exists; its
scratch stays under `docs/superpowers/` (gitignored).

## Which changes need a reviewed contract

Behavior changes, and nothing else. **Behavior means labeler's**: the API, the template schema, the
layout model, the coordinates and the error contract, which is what `docs/SPEC.md` froze and what
every capability under `openspec/specs/` names.

The harness is not that, however much its own behavior changes. The root npm manifests, `.claude/`, `.agent/`, `.agents/`, `.opencode/`, this file, `docs/WORKFLOW.md` and
`openspec/config.yaml` say how a change gets made, not what the service does, and no capability under
`openspec/specs/` is theirs to name.

The discriminator is the **spec delta**, and it needs no declaring: a change to labeler's behavior
always produces one, because the first-touch rule makes the first change to any documented behavior
write the complete post-change contract. A change with no delta has no contract to review.

So a documentation fix, anything under the paths above, a CI change, a dependency bump or a refactor
that keeps behavior identical goes: issue, worktree, the gates, one commit with `Fixes #N`, human
approval, fast-forward merge and push. No change folder, no plan review. Nothing else relaxes: it still starts as an issue and still
ends as one commit that closes it.

A correction to a published spec under `openspec/specs/` is not that lane, however much it reads like
a documentation fix. Those files are written by archive and never by hand, so the correction arrives
as a delta, and a delta is what makes it a change. What it does not have is code: the deliverable is
the delta itself; the proposal states that no implementation is required (#313).

Size decides nothing, and neither does effort. A nine-line handler check that alters behavior is a
full change; a five-hundred line documentation rewrite is not. There is no lane to declare, no
criteria to qualify under and no step that promotes one to the other. Writing a delta is what makes a
change one, so discovering mid-work that you need one costs the review and nothing else.

What no gate can decide is whether a diff *should* have carried a delta. A commit with no change
folder is checked by nobody, which `docs/WORKFLOW.md` records under what is not guaranteed.

## Making a change

A change to labeler's behavior uses plain [OpenSpec](https://github.com/Fission-AI/OpenSpec) with its built-in `spec-driven` schema. The CLI is pinned in the root npm manifest: run `npm ci`, then `npx --no-install openspec`; do not substitute a global installation.

In the change's worktree: `/opsx:propose` writes the proposal, delta specs, design and tasks, with literal `Fixes #N` in the proposal. A person reviews that plan before implementation. `/opsx:apply` implements, checking each task only after performing it. Run the gates below and `npx --no-install openspec validate --all --strict`, then `/opsx:archive` syncs the deltas into `openspec/specs/` and moves the folder to `openspec/changes/archive/`. Commit the result as one commit with `Fixes #N`.

See [`docs/WORKFLOW.md`](docs/WORKFLOW.md) for the full sequence and what no check covers.

## Commands

```bash
LABELER_CONFIG_DIR=./config-dev cargo run   # needs a writable config dir; /config is not one
cargo test                                  # unit + HTTP integration
cargo fmt --check                           # the spelling CI and the gates use
cargo clippy --all-targets --all-features
```

```bash
cd ui && npm run lint && npm run test && npm run build
```

`config-dev/` is gitignored and created on first run.

**Before reporting any change**, run every gate its diff touches: the four above for `src/`, the three
`ui/` ones for `ui/src/`. CI runs both and `build` needs `[rust, ui]`, so a green Rust suite over a
broken UI ships nothing. Never silence a lint with `#[allow(clippy::...)]`; fix the root cause.

**Every gate command is read-only.** `cargo fmt` runs as `--check`, so a gate reports a mis-formatted
tree rather than repairing it (#326). A gate must not change the candidate it checks. Repairing
formatting is an edit like any other, and it needs an author.

`rust-toolchain.toml` pins the compiler those gates run on, so a local pass and a CI pass mean the
same thing (#186). rustup installs the pinned toolchain on first `cargo` call; you need do nothing.
It is **not** an MSRV: it says "build with this", not "this is the oldest compiler we support", and
`Cargo.toml` declares no `rust-version`. One thing silently beats it: a per-directory
`rustup override set` from an earlier session. If your gate results stop matching CI, run
`rustup override unset` in the repo root and `rustc --version` to confirm.

Nothing bumps the pin for you. Dependabot has no updater for the file, which is the point: a new
stable can add a lint, and here that arrives as a deliberate commit with its fallout attached rather
than as a red build on someone else's PR. To bump: edit `channel`, run the gates, fix what the new
toolchain flags, and commit the bump on its own.

For non-trivial changes, web-search first to confirm current API behavior, especially for Typst, axum,
and utoipa, whose APIs shift between versions.

`scripts/render_avery_sheet.sh` exercises the batch endpoint end-to-end against a running server.

## Committing

Commit without prompting; do not wait to be asked. There are no pull requests.

**What a manual commit message says.** An imperative subject under 72 characters naming the change, a blank line,
then a body that answers *why*: the problem, and why this shape of fix over the obvious alternative.
Never inventory the diff. `git show --stat` already lists the files, the symbols and the counts, and a
body that repeats them spends the reader's attention on the one thing the log can already reconstruct
while burying the one thing it cannot. Three sentences is a normal body; a pure deletion usually needs
one line saying what stopped being true. Cite an issue where the reason lives there rather than
restating it. No `Co-Authored-By`, no "Generated with", no AI attribution of any kind, whatever your
harness injects by default.

Nothing is pushed before approval and no branch run is waited for. After human approval, from the default-branch checkout, integrate the change branch:

```bash
git merge --ff-only <change-branch> && git push
git branch -d <change-branch>
# No `git push origin --delete`: the branch was never pushed, so no remote ref to delete.
```

What is given up is a check on a clean machine before the commit lands. A broken commit surfaces on
`main`'s own CI run instead, which is a post-mortem: by the time it fails, the commit is already
integrated. That is acceptable here because publishing is already gated where it matters: `build`
needs `[rust, ui]` and runs only on `main` or a tag, so a broken commit ships
nothing until it is fixed forward.

**A change branch rebases onto `main`; it never merges `main` into itself** (#341). A back-merge
records that a branch outlived `main` and nothing else: of the 163 merges on `main`, 35 bring `main`
into a branch, and 21 of those carry no message beyond `Merge remote-tracking branch 'origin/main'`.
It also breaks every check that reads history through a single base ref, because a merge leaves two
previous commits for that one ref to explain.

Integration is `--ff-only`, which after a rebase always succeeds and leaves no bubble. `--no-ff` stays
for a branch whose boundary says something, which is what the milestone merges did.

**A change lands as one commit.** Never integrate with `git merge --squash`, which discards the
reviewed commit and its message.

**Never rewrite `main`, or any ref another session consumes.** That is the whole scope of the rule, and
a change branch is outside it: it is committed locally and deleted once merged.

If `main` moved, rebase the change branch and rerun the gates before asking for approval. Never
bypass a failed fast-forward by merging `main` into the change or by landing code changed after
review (#342).

## Breaking changes, until 1.0

Until `1.0`, a change that alters behavior breaks what came before, and that is the finished job. No
migration, no desugaring, no deprecation window, no second spelling, and no paragraph explaining the
one being removed. A dropped spelling becomes a parse error naming the file and the key, which
`deny_unknown_fields` gives once the field is gone; a field read and ignored is what this forbids.

Stored user data is the only exception: `store.rs:154-168` migrates the SQLite schema across
releases, because a user's printers and tokens have no author to fix them.

## Where behavior is specified

Two places, and the split is deliberate (ADR-0057).

Freezing a file freezes no behavior. Frozen says the file takes no further edits; changing what it
describes is ordinary work, and the changed contract is written somewhere else.

`openspec/specs/<capability>/spec.md` is that somewhere else. It holds everything added or changed
since 2026-08-19, and it is where a change to behavior lands.

`docs/SPEC.md` is **frozen** at commit `bc7b1ce` (2026-08-19): the baseline for the API, template
schema, layout model, coordinates, and error contract as of that date. Do not edit it, and do not add
changelog entries to it.

**Precedence.** A frozen `docs/SPEC.md` section stays authoritative until an OpenSpec requirement
explicitly names and supersedes it, and then only for that section. To look a rule up: read
`docs/SPEC.md`, then check `openspec/specs/` for a requirement superseding it.

**First-touch.** The first change to behavior documented only in the frozen spec writes an `ADDED`
requirement holding the *complete* post-change contract, not the difference, naming the `docs/SPEC.md`
section it supersedes. A `MODIFIED` delta is only valid against a requirement that already exists in
`openspec/specs/`: the tooling resolves `MODIFIED` by locating that requirement, so a `MODIFIED`
against an unmigrated section has nothing to resolve against.

`docs/adr/` is **frozen** at ADR-0057, for the reason `docs/SPEC.md` is: it was the right record for
its era and a better one superseded it. Do not write ADRs and do not add rows, and do not open one
looking for a rule that binds you now. An ADR records what was decided on a date, not what holds
today; where a later change overturned part of one, the index row in `docs/adr/README.md` says so and
names the issue that replaced it.

What the 57 are worth opening for is the *why* behind behavior `docs/SPEC.md` states without
rationale. The 31 records written after ADR-0057 are gone (#378): each duplicated a change folder that
`openspec/changes/archive/` keeps permanently, and every pointer that named one now names the issue
whose folder holds it. Rationale for a change lives in its `proposal.md` and `design.md`, and the
contract lives in `openspec/specs/` (#285).

`openspec/config.yaml` (`context`, `rules.*`) supplies project guidance to OpenSpec artifacts.
It restates rules from this file on purpose, so the workflow stands alone.
Change a process rule here and change it there too.

`tasks.md` is execution state for one accepted issue, never a backlog. That is what keeps the "issues
are the sole tracker" rule intact.

Also in `docs/`: `WORKFLOW.md` (how changes get made, for humans), `AUTHORING.md` (template model by
worked example), `VISION.md`, `DEPLOY.md`.

## Architecture

Request path `api.rs → render/`; template path `templates.rs → parse.rs → raw.rs → convert.rs`. The
web UI is a separate Vite/React app under `ui/`, gated by its own `lint`, `test` and `build` scripts.

- **Two-stage parsing.** YAML deserializes into `raw.rs` structs (all `deny_unknown_fields`), then
  converts to the domain model via `TryFrom` in `convert.rs`, with `serde_path_to_error` attaching a
  JSON path to every error. This lets the wire format differ from the validated model. *Adding a
  layout field means editing three files together: `raw.rs`, `models.rs`, `convert.rs`.*
- **Template registry.** Loaded and `validate()`d at startup (`main.rs`). A template that fails to
  parse or validate is **quarantined** and the server still starts (#175); so is a file whose id is
  already taken, with the lexicographically first filename keeping the id (#181). No
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
  `resolver.rs` alone.* (#226.)
- **Rendering** (`render/mod.rs`). Walks the layout emitting Typst markup; PNG via `typst-render`,
  sheets as one clipped box per slot via `typst-pdf`. `render/helpers.rs` holds string escaping,
  length formatting, QR-SVG generation (`qrcode`), and `ttf-parser`-based text fitting for
  `font_size: {min, max}` (auto-shrink plus ellipsis truncation).
- **Errors.** `TemplateError` (parse/validation, carries a path) quarantines rather than aborting
  startup. `AppError` is the HTTP error, serializing to `{ "error": { code, message, details } }`. Add
  new kinds as `AppError` constructors so `code` strings stay stable.
- **OpenAPI.** Every model exposed in the API must be registered in `src/openapi.rs`.
- **`src/errors.rs` reads `docs/SPEC.md` at test time** through `env!("CARGO_MANIFEST_DIR")`, so the
  reason-completeness test is keyed to the tree the binary was built in. Never share a `target/`
  between worktrees.

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
YAML files under `tests/fixtures/templates/` are a different thing: they are test inputs, and what
makes them right is the test that reads them.

## Notes

- `CLAUDE.md` is a symlink to this file, so the two names are one file. Put personal,
  machine-specific instructions in `CLAUDE.local.md` instead; it is gitignored and loads alongside.
  Which agents are installed and authenticated is that kind of fact.
- The `openspec-*` skills and `opsx` commands under `.claude/`, `.agent/`, `.agents/`, `.opencode/`
  are **generated** (43 files; the 24 `SKILL.md` manifests record `generatedBy: 1.9.0`). Never
  hand-edit them. To upgrade: upgrade the CLI, `openspec update --force`, review all four trees,
  commit the regeneration alone.
- Do not delete `openspec/changes/archive/.gitkeep` while that directory is empty.
- Fonts: Inter loads via `typst-kit` from `fonts/InterVariable.ttf`; Typst is told to use
  `"Inter Variable"`/`"Inter"`.
