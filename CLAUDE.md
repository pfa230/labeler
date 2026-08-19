# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A label-rendering REST service (Rust/axum). It loads YAML label templates at startup and renders
either a single label to PNG or a sheet of labels to PDF, by generating Typst source on the fly and
compiling it with `typst-as-lib`.

## Docs and process

Behavior is specified in two places, and the split is deliberate (ADR-0057).

`docs/SPEC.md` is **frozen** at commit `bc7b1ce` (2026-08-19). It is the baseline reference for the
API, template schema, layout model, coordinate system, and error contract as they stood on that date.
Do not edit it and do not add changelog entries to it.

`openspec/specs/<capability>/spec.md` holds behavior added or changed since. It starts empty and
accrues one capability at a time. **Precedence:** a frozen `docs/SPEC.md` section stays authoritative
until an OpenSpec requirement explicitly names and supersedes it, and then only for the named section.
To look a rule up, read `docs/SPEC.md`, then check `openspec/specs/` for a requirement superseding it.

**First-touch rule.** The first change to behavior documented only in the frozen spec writes an
`ADDED` requirement holding the *complete* post-change contract, not the difference, and names the
`docs/SPEC.md` section it supersedes. A `MODIFIED` delta is only valid against a requirement that
already exists in `openspec/specs/`: the tooling resolves `MODIFIED` by locating that requirement, so
a `MODIFIED` against an unmigrated section has nothing to resolve against.

`docs/adr/` holds Architecture Decision Records (Nygard format); ADRs are append-only, so supersede
rather than edit. On every behavior change, add or supersede the relevant ADR and add its row to
`docs/adr/README.md`, in the same change. That requirement is unchanged; only its paired spec artifact
moved from `docs/SPEC.md` to an OpenSpec delta.

`docs/AUTHORING.md` teaches the template model by worked example, `docs/VISION.md` holds the project
vision, and `docs/DEPLOY.md` covers deployment.

Templates are visual artifacts: a YAML edit that parses and renders without error is not proof the label
looks right. When authoring or fixing a template, use a render -> inspect -> fix loop. Render to PNG
(`POST /api/render/label?format=png`; run with `LABELER_NO_AUTH=true` locally so the endpoint is open),
open the image and check it against intent (QR side and squareness, text inside the printable area,
alignment, auto-shrink, no clipping or overflow), fix the YAML, and re-render (`POST /api/templates/reload`
picks up edits without a restart) until it is actually correct, not until it merely renders. See #67.

Track work as GitHub issues and milestones, never as markdown TODOs or roadmap docs. `docs/` holds
reference material only (`SPEC.md`, `AUTHORING.md`, `VISION.md`, `DEPLOY.md`, `adr/`); plans and
proposals live in `openspec/changes/`, not `docs/`. File issues with `gh issue create` and reference
them from commits (e.g. `Fixes #12`). For work you won't do now, open an issue rather than leaving a
TODO in code, in docs, or as an unchecked task in `tasks.md`.

## OpenSpec workflow

Behavior changes go through OpenSpec (CLI 1.9.0, ADR-0057). The order matters, because archive
rewrites the main specs after implementation is done.

1. **Issue.** Every change implements exactly one open GitHub issue. Create it first if it does not
   exist.
2. **Branch.** Short-lived branch off `main`, as below.
3. **Propose.** `/opsx:propose` writes `openspec/changes/<name>/` (proposal, delta specs, design,
   tasks). Name the change `issue-<N>-<slug>`, and link the issue number in `proposal.md`. Stop there:
   propose is a planning step and must not touch code.
4. **Human review.** Review *all* the artifacts, not just `proposal.md`. The delta specs are the part
   that becomes normative.
5. **Apply.** `/opsx:apply` works the task list.
6. **Adversarial review loop.** Run it against the diff, per the section below. Do not skip it because
   the tasks are all checked.
7. **Archive.** `/opsx:archive`. **Always sync every delta spec into `openspec/specs/`.** OpenSpec's
   archive step is advisory: it will offer to archive with unchecked tasks or without syncing. Both
   are forbidden here. If tasks are genuinely out of scope, cut them and open an issue instead.
8. **Review the archive diff.** Archive rewrote `openspec/specs/`, so that diff has not been reviewed
   yet. Read it before committing.
9. **Verify.** `openspec doctor`, `openspec validate --all --strict --no-interactive`,
   `openspec validate --archived --no-interactive`, then `cargo fmt`, `cargo clippy --all-targets
   --all-features`, `cargo test`.
10. **Commit and integrate.** One commit covering code, ADR, main specs, and the archived change, with
    `Fixes #N`. Then merge to `main` and push.

`openspec/changes/<name>/tasks.md` is execution state for one accepted issue. It is not a backlog:
never park future work there as an unchecked task. That keeps the "issues are the sole tracker" rule
intact.

The `openspec-*` skills and `opsx` commands under `.claude/`, `.agent/`, `.agents/`, and `.opencode/`
are **generated** by the OpenSpec CLI: 43 files, of which the 24 `SKILL.md` manifests carry
`generatedBy: 1.9.0` in their frontmatter. They are committed, but never hand-edit them:
`openspec update` overwrites them. To upgrade the CLI, upgrade it, run
`openspec update --force`, review all four trees together, and commit the regeneration on its own.

Project conventions are also encoded in `openspec/config.yaml` (`context`, `rules.*`,
`operations.*.guidance`), which is what the `opsx` workflows inject into each artifact. It restates
the rules in this file on purpose, so the OpenSpec workflow stands on its own. When you change a
process rule here, change it there too. Note that `operations.*.guidance` is advisory to the agent,
while `rules.*` are stated to it as constraints; this file is what makes a rule hard.

## Working on an issue

After the implementation (coder) work for an issue is complete, do not call it done. Spin up an
**adversarial code reviewer** (a separate reviewer agent, briefed to find real problems, not to
rubber-stamp) and run a review → fix → review loop:

1. Reviewer audits the diff against the issue's acceptance criteria, correctness, edge cases, tests,
   and the conventions in this file.
2. Coder addresses every meaningful finding (fix it, or justify why it is not a problem with evidence).
3. Re-review the updated diff.
4. Repeat until a review pass surfaces no meaningful fixes (nits the author consciously declines do not
   count as meaningful).

Only then is the issue's work complete and ready to commit/PR. The reviewer is adversarial by design:
fluent code is not proof of correct code, so verify findings against the actual code with file:line
evidence before accepting or dismissing them.

When the adversarial reviewer is **codex** (used to review plans, designs, or diffs), absent unresolved
critical (MAJOR/blocking) issues, the hard cap on codex passes is **5**. Stop after 5 even if codex keeps
surfacing minor or stylistic nits; convergence to "no MAJOR issues" is the goal, not an empty findings
list. Only exceed 5 if a genuine critical issue is still open.

## Committing and integrating

In this repo you are allowed and required to commit and push without prompting for approval. Commit
completed, verified work on your own, with a clear message; do not wait to be asked. For a behavior
change that means the full OpenSpec order above: archive and sync first, review the resulting main-spec
diff, then a clean `cargo fmt`/`clippy`/`test`, then one commit covering code, ADR, main specs, and the
archived change.

We do not use pull requests yet. Do feature work on a short-lived branch for isolation, then integrate
by merging it into the default branch (`main`) and pushing directly:
`git checkout main && git merge <branch> && git push`. Reference the issue with `Fixes #N` in the
commit so it closes on push. Never force-push; never rewrite already-pushed history.

GitHub issues and milestones are the sole live tracker. Do not keep roadmap docs in the repo: they go
stale and duplicate the tracker. The superpowers `writing-plans` step is retired here (ADR-0057): the
OpenSpec change folder is the plan, and it is committed. `superpowers:brainstorming` is still useful
for fuzzy ideas before a change exists; its scratch stays under `docs/superpowers/` (git-ignored).

## Commands

```bash
LABELER_CONFIG_DIR=./config-dev cargo run  # start the server; needs a writable config dir
cargo test                 # run all tests (unit + HTTP integration in src/lib.rs)
cargo test render_pdf      # run a single test by name
cargo fmt                  # format
cargo clippy --all-targets --all-features   # lint
```

`LABELER_CONFIG_DIR` is required for local runs (default `/config` is not writable on a dev machine).
`config-dev/` is gitignored; the directory is created automatically on first run.

Before reporting any change, run `cargo fmt`, `cargo clippy --all-targets --all-features`, and
`cargo test`. Never silence a lint with `#[allow(clippy::...)]`; fix the root cause.

For any non-trivial change, run at least one web search first to confirm current best practices or API
behavior, especially for Typst, axum, and utoipa, whose APIs shift between versions.

To exercise the batch endpoint end-to-end, run `cargo run` then `scripts/render_avery_sheet.sh`;
it writes a PDF.

## Endpoints

`GET /health`, `GET /templates`, `GET /templates/{id}`, `POST /render/label` (PNG),
`POST /render/batch` (PDF), plus `GET /openapi.json` and Swagger UI at `/docs`. Routes are wired in
`src/api.rs`; the OpenAPI doc is assembled in `src/openapi.rs`. Every model exposed in the API must be
registered in `openapi.rs`.

## Architecture

Request path: `api.rs` → `render/`. Template path: `templates.rs` → `parse.rs` → `raw.rs` →
`convert.rs`.

- **Two-stage parsing.** YAML deserializes into `raw.rs` structs (all `deny_unknown_fields`), then
  converts into the domain model via `TryFrom` in `convert.rs`. `parse.rs` orchestrates this and
  attaches a JSON-path location to every error via `serde_path_to_error`. The split lets the wire
  format (`padding: 0.06` shorthand or `[t,r,b,l]`; `at`/`size` flattened into the item) differ from
  the validated internal model. When adding a layout field, update all three together: `raw.rs`, the
  matching `models.rs` type, and the `TryFrom` in `convert.rs`.

- **Template registry.** Templates are loaded from `{config}/templates/` (where `{config}` is
  `LABELER_CONFIG_DIR`) at startup (`main.rs`), parsed and `validate()`d, and duplicate ids are rejected.
  A template that fails to parse or validate is **quarantined** and the server still starts (#175); a
  duplicate id ejects both contenders. Nothing is seeded into a fresh config dir. Templates are
  immutable, shared via `Arc` as axum state.

- **Layout model** (`models.rs`). A template's `layout` is a tree of `LayoutItem`s: `Text`, `Qr`,
  `Image`, `Line`, `Container`. `Container` is recursive: it nests `items` and may carry a `frame`
  (outline) and `padding`. Any item may carry `when:`, the universal conditional-visibility predicate
  that gates whether it renders (ADR-0056).

- **Rendering** (`render/mod.rs`). `render_single_label` and `render_sheet_labels` walk the layout via
  `RenderContext::render_items` and emit Typst markup (`#place`, `#box`, `#text`, `#image`, `#line`,
  `#rect` for container frames). The single path renders the first page to PNG with `typst-render`;
  the sheet path places one clipped box per slot and exports PDF with `typst-pdf`. `render/helpers.rs`
  holds Typst-string escaping, length formatting, QR-SVG generation (`qrcode`), and the `fontdue`-based
  text fitting for `font_size: {min, max}` (auto-shrink plus ellipsis truncation).

- **Coordinate system.** Template coordinates use a bottom-left origin, y-up, in the template `unit`
  (`mm` or `in`). Typst uses a top-left origin, so the renderer flips with `frame_height_units - top`.
  A `Container` re-bases its children into its padded inner box via a fresh `RenderContext` carrying
  the inner width/height. Watch this when touching placement math.

- **Sizing.** `size` values are a number or `auto`. `auto` resolves to `max_w`/`max_h` if given, else
  (for containers and lines) the parent frame size. `validate_bounds` enforces that items fit their
  layout bounds. This logic is duplicated between compile-time validation (`templates.rs`) and
  render-time resolution (`render/mod.rs`); keep the two in sync.

- **Errors.** `TemplateError` (parse/validation, carries a path; `errors.rs`) surfaces at load, where
  it quarantines the offending template rather than aborting startup.
  `AppError` is the HTTP error: it maps to a status code and serializes to the stable
  `{ "error": { code, message, details } }` schema. Add new error kinds as `AppError` constructors so
  the `code` strings stay stable.

## Params and conditional visibility

Templates declare typed inputs under top-level `params:` (`string`, `length`, `integer`, `number`,
`boolean`, `enum`), with defaults, range bounds, and UI metadata. A request's values are validated
against them. Any layout item may carry `when:`, a predicate over those params gating whether it
renders. This replaced the legacy top-level `options` map and `container.option` (ADR-0056, #162).
See `catalog/sheet/avery/avery5163.yaml` and `docs/AUTHORING.md` for worked examples.

## Notes

- `AGENTS.md` is a symlink to this file; edit `CLAUDE.md` and both stay in sync. Both are committed
  (`.gitignore` negates a global ignore that would otherwise drop them), so a fresh clone gets these
  instructions.
- `openspec/specs/.gitkeep` and `openspec/changes/archive/.gitkeep` anchor directories git cannot
  otherwise track. Do not delete them while those directories are empty.
- `*.pdf` is gitignored; the sample PDFs in the repo root are local render artifacts.
- Fonts: Inter loads via `typst-kit` from the bundled `fonts/InterVariable.ttf`; Typst is told to use
  `"Inter Variable"`/`"Inter"`.
