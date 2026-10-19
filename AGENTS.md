# Labeler

A stateless label-rendering REST service (Rust/axum) with a React UI under `ui/`. It loads YAML label templates from `{LABELER_CONFIG_DIR}/templates/` and renders a single label to PNG or a batch to PDF/ZIP by generating [Typst](https://typst.app/) source and compiling it in-process via `typst-as-lib`.

## Simplicity

- **Minimal by design (KISS).** A change contains only what a concrete need of the accepted issue requires. This holds for each part of the plan on its own:
  - Each sentence and scenario a spec delta adds or rewrites states behavior that the existing specs in `openspec/specs/` do not already imply. A delta line that restates an existing requirement, or a scenario for a case the delta's own condition already excludes, breaks this rule.
  - Each mechanism, option, field, check, test and task serves a requirement of the existing specs, a delta line that meets the point above, or, where no spec applies, the issue's explicit request, such as docs or tooling. None duplicates what the repository already has.
  - Each item is the smallest version that serves its requirement or request. A larger version needs a failure of the smaller one, shown by a run, a test or the code as it stands. A test, fixture or check that guards only against a risk no run has shown breaks this rule.
  - It adds no speculative configuration, plugin points, extra layers, silent fallbacks, or generality for problems nobody has hit.
- **Build only what the contract and the issue require.** The contract is `openspec/specs/`, as the change amends it. Leave out handling for cases neither names, for failures nobody has observed, and for keeping behavior the change replaces. Labeler relies on what other systems promise, such as a printer honouring a standard IPP attribute or a client obeying the API. It neither detects a violation nor works around one. "Safer", "more robust" and "in case" are not reasons to add anything.
- **Cut before presenting.** After the last artifact or edit and before any summary, run a cut pass against "Minimal by design". List each spec sentence and scenario the change adds or rewrites, then each mechanism, option, field, check, test and task, each on its own line with what it serves and the failure its smaller version causes. Remove what fails the rule. The presentation includes the cut list: what the draft contained and the pass removed, and why each kept item stayed.

## Work

- **One issue, one worktree.** GitHub issues are the only tracker: no TODOs in code or docs, no backlog in `tasks.md`. Every piece of work gets its own worktree (`git worktree add .worktrees/issue-<N> -b issue-<N>-<slug> origin/main`), because sessions run concurrently. Never switch branches inside one.
- **Claim only what you did.** Check a box or write "verified" only after performing the thing. A test that cannot fail is worse than none: before accepting one, name what would have to break for it to fail.
- **Run artifacts** go to `.agent-runs/` at the worktree root (gitignored), never into the repository.

## Spec

The behavior contract lives in `openspec/specs/<domain>/spec.md`, one capability per domain: `templates`, `parameters`, `interpolation`, `layout`, `text`, `rendering`, `errors`, `auth`, `printing`, `settings`, `mcp`, `ui`. Read only the domains your change touches. `docs/AUTHORING.md` is the human guide to writing templates; where it disagrees with the spec, the guide is wrong.

**Behavior changes go through OpenSpec and openspec-loop; nothing else does.** Behavior means labeler's API, template schema, layout, rendering, errors and UI, which is what the specs describe. The OpenSpec CLI is pinned in the root npm manifest: `npm ci`, then `npx --no-install openspec`. openspec-loop is installed globally (`npm install -g openspec-loop`) and needs `openspec` 1.14.0 or later on `PATH`; `openspec/loop.yaml` configures it. In the change's worktree:

1. `/opsx:propose` writes the proposal (with literal `Fixes #N`), delta specs, design and tasks. A person reviews the plan, then commits it on the issue branch. Proposing stops there: start the loop only when the user asks, and never apply a behavior change by hand.
2. `loop run <change>`, supervised through the `loop-run` skill, reviews the plan, implements it, runs the gates, reviews the patch, archives the change and opens a pull request. The change lands as the loop's commits, its folder kept under `openspec/changes/archive/`.

A docs fix, harness change (root npm manifests, `.claude/`, `.agent/`, `.agents/`, `.opencode/`, this file, `openspec/config.yaml`, `openspec/loop.yaml`), CI change, dependency bump or behavior-preserving refactor skips OpenSpec and the loop: issue, worktree, gates, one commit. So does code that catches up to a contract already in `openspec/specs/`: there is no delta to write, and the issue names the requirements it implements. Nothing checks whether a diff should have carried a delta; that judgment is yours.

**Breaking changes, until 1.0.** A behavior change replaces what came before: no migration, no deprecation window, no second spelling, no explanation of the removed one. A dropped key becomes a parse error via `deny_unknown_fields`. Stored user data is the one exception: `store.rs` migrates the SQLite schema.

## Commands and gates

```bash
LABELER_CONFIG_DIR=./config-dev cargo run   # needs a writable config dir; config-dev/ is gitignored
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
npm --prefix ui run lint && npm --prefix ui run test && npm --prefix ui run build
npx --no-install openspec validate --all --strict --no-interactive
```

Before reporting any change, run every gate its diff touches: the Rust ones for `src/` and `tests/`, the UI ones for `ui/src/`, `openspec validate` for `openspec/`. Gates are read-only: never let one repair the tree it checks. Never silence a lint with `#[allow(clippy::...)]`; fix the cause. `rust-toolchain.toml` pins the compiler; if local results stop matching CI, run `rustup override unset` and check `rustc --version`. Bump the pin only in a commit of its own.

For non-trivial work, web-search current API behavior first, especially Typst, axum and utoipa.

## Committing

Commit without prompting. A manual message is an imperative subject under 72 characters, a blank line, and a body that says why: the problem, and why this fix over the obvious alternative. Never inventory the diff. No `Co-Authored-By`, no "Generated with", no AI attribution of any kind.

Integration needs human approval; for a loop run, that is the review of its pull request. Then, from the default-branch checkout:

```bash
git merge --ff-only <change-branch> && git push
git worktree remove .worktrees/<dir> && git branch -d <change-branch>
git push origin --delete <change-branch>   # only for a pushed branch, such as a loop run's
```

Pushing `main` with the pull request's commits marks it merged. A change branch rebases onto `main`; if `main` moved, rebase, rerun the gates, and push a pushed branch with `--force-with-lease` before asking for approval. The loop rebases by itself and merges `main` in only when the rebase conflicts. Never integrate with `git merge --squash`, and never rewrite `main` or any ref another session consumes. CI on `main` runs after integration; `build` needs `[rust, ui]`, so a broken commit ships nothing until fixed forward.

## Architecture

Request path `api.rs → render/`; template path `templates.rs → parse.rs → raw.rs → convert.rs`.

- **Two-stage parsing.** YAML deserializes into `raw.rs` structs (all `deny_unknown_fields`), then converts to the domain model via `TryFrom` in `convert.rs`, with `serde_path_to_error` attaching a path to every error. Adding a layout field means editing `raw.rs`, `models.rs` and `convert.rs` together.
- **Template registry.** Loaded and validated at startup. A template that fails, or whose id is taken, is quarantined and the server still starts. Templates are immutable and shared via `Arc`.
- **Coordinates.** Bottom-left origin, y-up, in the template `unit`. Typst is top-left, so the renderer flips with `frame_height_units - top`. A `Container` re-bases children into its padded inner box via a fresh `RenderContext`.
- **Sizing** (`resolver.rs`). An extent is a number, `content` or `fill`. `source_of` is the only place a spelling is classified, and `resolve`, `available` and `requirement` are shared by load-time validation and render-time resolution, so the two cannot drift. Adding a source or bound means editing `resolver.rs` alone.
- **Rendering** (`render/mod.rs`). Walks the layout emitting Typst markup; PNG via `typst-render`, sheets as one clipped box per slot via `typst-pdf`. `render/helpers.rs` holds escaping, length formatting, QR SVG generation and `ttf-parser` text fitting.
- **Errors.** `TemplateError` quarantines; `AppError` is the HTTP error, serializing to `{ "error": { code, message, details } }`. Add kinds as `AppError` constructors so `code` strings stay stable.
- **OpenAPI.** Register every API model in `src/openapi.rs`.

## Templates are visual artifacts

A template that parses and renders is not proof it looks right. Render to PNG (`POST /api/render/label?format=png`, with `LABELER_NO_AUTH=true` locally), open the image, check it against intent (QR squareness, text inside the printable area, alignment, no clipping), fix and re-render (`POST /api/template-registry/reload`). No task may claim this check, because nothing can verify it later. Files under `tests/fixtures/templates/` are test inputs, judged by the tests that read them.

## Notes

- `CLAUDE.md` is a symlink to this file. Personal, machine-specific instructions go in the gitignored `CLAUDE.local.md`.
- The `openspec-*` skills and `opsx` commands under `.claude/`, `.agent/`, `.agents/` and `.opencode/` are generated. Never hand-edit them: upgrade the CLI, run `openspec update --force`, review, and commit the regeneration alone.
- The bundled `loop-*` skills under `.claude/skills/` and `.agents/skills/` are copied verbatim from the installed package. After `npm update -g openspec-loop`, run `loop setup` and commit the refreshed copies alone.
- Fonts: Inter loads from `fonts/InterVariable.ttf`; Typst is told to use `"Inter Variable"`/`"Inter"`.
