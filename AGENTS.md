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

Also in `docs/`: `AUTHORING.md` (template model by worked example), `VISION.md`, `DEPLOY.md`.

## Tracking work

GitHub issues and milestones are the sole live tracker. No markdown TODOs, no roadmap docs. File with
`gh issue create` and reference from commits (`Fixes #12`). Work you won't do now becomes an issue,
never a TODO in code or docs, and never an unchecked task in `tasks.md`.

The superpowers `writing-plans` step is retired here (ADR-0057); the OpenSpec change folder is the
plan, and it is committed. `superpowers:brainstorming` still helps for fuzzy ideas before a change
exists; its scratch stays under `docs/superpowers/` (gitignored).

## OpenSpec workflow

Behavior changes go through OpenSpec (CLI 1.9.0) on this project's own schema,
`openspec/schemas/labeler/`: `proposal → specs → design → review → tasks → apply`. Order matters,
because the review gates implementation and archive rewrites the main specs *after* it:

1. **Issue**, then a worktree: `git worktree add .worktrees/issue-<N> -b issue-<N>-<slug>`.
2. **`/opsx:propose`** writes `openspec/changes/issue-<N>-<slug>/`. Planning only; it must not touch
   code. Link the issue in `proposal.md`.
3. **Human reviews all artifacts**, not just `proposal.md`. The delta specs become normative.
4. **Adversarial review of the plan**, before any task is written: the `review` artifact, judging
   `proposal.md` + `specs/` + `design.md`. A second model in read-only mode, else a fresh-context
   subagent. **Never** self-review inside the authoring context. It writes `review.md` ending in a
   `VERDICT:` line. `REVISE` → fix and re-run the *full* review; `APPROVE_WITH_CHANGES` → apply the
   listed edits, reviewer re-checks only those, then set `CHANGES_APPLIED: yes`. Two consecutive
   `REVISE` rounds escalate to the human. Editing the specs or design afterwards voids the verdict.
5. **`/opsx:apply`**, then the adversarial review of the *diff* below. Two different reviews: step 4
   judged the plan, this one judges the code. Do not skip it because tasks are checked.
   `.claude/hooks/review-gate.sh` refuses writes to `src/` and `ui/src/` until the verdict passes,
   because OpenSpec only checks that artifacts exist, never what they say.
6. **`/opsx:archive`**, always syncing every delta into `openspec/specs/`. Archive is advisory and
   will offer to skip the sync or accept unchecked tasks; both are forbidden here. Out-of-scope tasks
   get cut and filed as issues.
7. **Review the archive diff.** It rewrote `openspec/specs/` after your last review pass.
8. **Verify**, then one commit covering code, ADR, specs, and the archived change, with `Fixes #N`.

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
cases, tests, and this file. Address every meaningful finding or justify with file:line evidence why
it is not one. Re-review. Repeat until a pass surfaces no meaningful fixes.

Fluent code is not correct code: verify each finding against the actual code before accepting *or*
dismissing it. When the reviewer is **codex**, cap at **5** passes absent an unresolved blocking
issue. Converging on "no MAJOR issues" is the goal, not an empty findings list.

## Isolation: one change, one worktree, one issue

Every change gets its own git **worktree**, not just a branch. A branch shares one working directory,
so two sessions collide, and an OpenSpec change folder is untracked until the final commit, which
means it follows you across `git checkout` and makes "is a change in progress here?" unanswerable.

```bash
git worktree add .worktrees/issue-<N> -b issue-<N>-<slug>   # start
cd .worktrees/issue-<N>                                     # work here, only here
```

Need an unrelated hotfix while a change is in flight? Another worktree. Never switch branches inside
a change's worktree, and never carry one change's worktree into another's work.

`/.worktrees/` is gitignored. See `superpowers:using-git-worktrees`.

## Committing

Commit and push without prompting; do not wait to be asked. No pull requests: from the repo root,
`git merge issue-<N>-<slug> && git push`, then `git worktree remove .worktrees/issue-<N>` and
`git branch -d issue-<N>-<slug>`. Never force-push, never rewrite pushed history.

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
- **Sizing.** `size` is a number or `auto`. `auto` resolves to `max_w`/`max_h` if given, else (for
  containers and lines) the parent frame. *This logic is duplicated between compile-time validation
  (`templates.rs`) and render-time resolution (`render/mod.rs`); keep the two in sync.*
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
