# Labeler

A stateless label-rendering REST service (Rust/axum) with a React UI under `ui/`. It loads YAML label templates from `{LABELER_CONFIG_DIR}/templates/` and renders a single label to PNG or a batch to PDF/ZIP by generating [Typst](https://typst.app/) source and compiling it in-process via `typst-as-lib`.

## Simplicity

- **Minimal by design (KISS).** Every mechanism, option, field and check must serve a concrete need of the accepted issue. Do not add speculative configuration, plugin points, extra layers, silent fallbacks, or generality for problems nobody has hit. When two designs both work, use the one with fewer moving parts.
- **Cut before presenting.** After the last artifact or edit and before any summary, run a cut pass. List each mechanism, option, check, field and task on its own line, and name the spec requirement it serves, as the change amends it, or the issue's explicit request where no spec applies, such as docs or tooling. Remove whatever names neither, and whatever duplicates what the repository already has. For each item that stays, name its simpler version and the concrete failure that version causes; when there is none, use the simpler version. The presentation includes the cut list: what was removed, and why each kept item stayed.

## Work

- **One issue, one worktree, one commit.** GitHub issues are the only tracker: no TODOs in code or docs, no backlog in `tasks.md`. Every piece of work gets its own worktree (`git worktree add .worktrees/issue-<N> -b issue-<N>-<slug> origin/main`), because sessions run concurrently. Never switch branches inside one.
- **Claim only what you did.** Check a box or write "verified" only after performing the thing. A test that cannot fail is worse than none: before accepting one, name what would have to break for it to fail.
- **Run artifacts** go to `.agent-runs/` at the worktree root (gitignored), never into the repository.

## Spec

The behavior contract lives in `openspec/specs/<domain>/spec.md`, one capability per domain: `templates`, `parameters`, `interpolation`, `layout`, `text`, `rendering`, `errors`, `auth`, `printing`, `settings`, `connections`, `ui`. Read only the domains your change touches. `docs/AUTHORING.md` is the human guide to writing templates; where it disagrees with the spec, the guide is wrong.

**Behavior changes go through OpenSpec; nothing else does.** Behavior means labeler's API, template schema, layout, rendering, errors and UI, which is what the specs describe. The OpenSpec CLI is pinned in the root npm manifest: `npm ci`, then `npx --no-install openspec` (never a global install). In the change's worktree:

1. `/opsx:propose` writes the proposal (with literal `Fixes #N`), delta specs, design and tasks. A person reviews the plan before implementation.
2. `/opsx:apply` implements.
3. `npx --no-install openspec archive <change> --yes` merges the deltas into `openspec/specs/`. Then delete `openspec/changes/archive/`: the tree holds only the current spec, and git keeps the history.
4. Run the gates, then commit everything as one commit with `Fixes #N`.

A docs fix, harness change (root npm manifests, `.claude/`, `.agent/`, `.agents/`, `.opencode/`, this file, `openspec/config.yaml`), CI change, dependency bump or behavior-preserving refactor skips the change folder: issue, worktree, gates, one commit. Nothing checks whether a diff should have carried a delta; that judgment is yours.

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

Integration needs human approval. Then, from the default-branch checkout:

```bash
git merge --ff-only <change-branch> && git push
git worktree remove .worktrees/<dir> && git branch -d <change-branch>
```

A change branch rebases onto `main` and never merges `main` into itself; if `main` moved, rebase and rerun the gates before asking for approval. Never integrate with `git merge --squash`, and never rewrite `main` or any ref another session consumes. CI on `main` runs after integration; `build` needs `[rust, ui]`, so a broken commit ships nothing until fixed forward.

## Architecture

Request path `api.rs → render/`; template path `templates.rs → parse.rs → raw.rs → convert.rs`.

- **Two-stage parsing.** YAML deserializes into `raw.rs` structs (all `deny_unknown_fields`), then converts to the domain model via `TryFrom` in `convert.rs`, with `serde_path_to_error` attaching a path to every error. Adding a layout field means editing `raw.rs`, `models.rs` and `convert.rs` together.
- **Template registry.** Loaded and validated at startup. A template that fails, or whose id is taken, is quarantined and the server still starts. Templates are immutable and shared via `Arc`.
- **Coordinates.** Bottom-left origin, y-up, in the template `unit`. Typst is top-left, so the renderer flips with `frame_height_units - top`. A `Container` re-bases children into its padded inner box via a fresh `RenderContext`.
- **Sizing** (`resolver.rs`). An extent is a number, `content` or `fill`. `source_of` is the only place a spelling is classified, and `resolve`, `available` and `requirement` are shared by load-time validation and render-time resolution, so the two cannot drift. Adding a source or bound means editing `resolver.rs` alone.
- **Rendering** (`render/mod.rs`). Walks the layout emitting Typst markup; PNG via `typst-render`, sheets as one clipped box per slot via `typst-pdf`. `render/helpers.rs` holds escaping, length formatting, QR SVG generation and `ttf-parser` text fitting.
- **Errors.** `TemplateError` quarantines; `AppError` is the HTTP error, serializing to `{ "error": { code, message, details } }`. Add kinds as `AppError` constructors so `code` strings stay stable. A test checks that the `errors` spec's reason table matches `Reason::ALL` exactly.
- **OpenAPI.** Register every API model in `src/openapi.rs`.
- Never share a `target/` between worktrees: tests read spec files through `CARGO_MANIFEST_DIR`.

## Templates are visual artifacts

A template that parses and renders is not proof it looks right. Render to PNG (`POST /api/render/label?format=png`, with `LABELER_NO_AUTH=true` locally), open the image, check it against intent (QR squareness, text inside the printable area, alignment, no clipping), fix and re-render (`POST /api/templates/reload`). No task may claim this check, because nothing can verify it later. Files under `tests/fixtures/templates/` are test inputs, judged by the tests that read them.

## Notes

- `CLAUDE.md` is a symlink to this file. Personal, machine-specific instructions go in the gitignored `CLAUDE.local.md`.
- The `openspec-*` skills and `opsx` commands under `.claude/`, `.agent/`, `.agents/` and `.opencode/` are generated. Never hand-edit them: upgrade the CLI, run `openspec update --force`, review, and commit the regeneration alone.
- Fonts: Inter loads from `fonts/InterVariable.ttf`; Typst is told to use `"Inter Variable"`/`"Inter"`.
