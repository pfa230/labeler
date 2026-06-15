# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A label-rendering REST service (Rust/axum). It loads YAML label templates at startup and renders
either a single label to PNG or a sheet of labels to PDF, by generating Typst source on the fly and
compiling it with `typst-as-lib`.

## Docs and process

`docs/SPEC.md` is the living spec and source of truth for the API, template schema, layout model,
coordinate system, options, and error contract. Read it before non-trivial work. `docs/CAPABILITIES.md`
is the tiered capability list (MVP/P2/Later) that drives the spec and roadmap; it also lists the
project vision and the decisions still to ratify. `docs/adr/` holds Architecture Decision Records
(Nygard format); ADRs are append-only, so supersede rather than edit.

On every behavior change, update `docs/SPEC.md` (including its changelog) and add or supersede the
relevant ADR in the same change.

Track work as GitHub issues, never as markdown TODOs. `docs/` holds plans, specs, and decisions only.
File issues with `gh issue create` and reference them from commits/PRs (e.g. `Fixes #12`). For work
you won't do now, propose opening an issue rather than leaving a TODO in code or docs.

## Commands

```bash
cargo run                  # start the server (PORT env var, default 8080)
cargo test                 # run all tests (unit + HTTP integration in src/lib.rs)
cargo test render_pdf      # run a single test by name
cargo fmt                  # format
cargo clippy --all-targets --all-features   # lint
```

Before reporting any change, run `cargo fmt`, `cargo clippy --all-targets --all-features`, and
`cargo test`. Never silence a lint with `#[allow(clippy::...)]`; fix the root cause.

For any non-trivial change, run at least one web search first to confirm current best practices or API
behavior, especially for Typst, axum, and utoipa, whose APIs shift between versions.

To exercise the batch endpoint end-to-end, run `cargo run` then `scripts/render_test.sh` or
`scripts/render_avery_horizontal.sh`; each writes a PDF.

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

- **Template registry.** `TemplateRegistry::load_from_dir("templates")` runs at startup (`main.rs`),
  parses and `validate()`s every `.yaml`/`.yml` file, and rejects duplicate ids. One invalid template
  aborts startup. Templates are immutable, shared via `Arc` as axum state.

- **Layout model** (`models.rs`). A template's `layout` is a tree of `LayoutItem`s: `Text`, `Qr`,
  `Line`, `Container`. `Container` is recursive: it nests `items` and may carry a `frame` (outline),
  `padding`, and an `option` map gating whether the subtree renders for a given option selection.

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

- **Errors.** `TemplateError` (parse/validation, carries a path; `errors.rs`) surfaces at startup.
  `AppError` is the HTTP error: it maps to a status code and serializes to the stable
  `{ "error": { code, message, details } }` schema. Add new error kinds as `AppError` constructors so
  the `code` strings stay stable.

## Options

Templates may declare `options`, a map of name to allowed values (e.g. `orientation: [horizontal,
vertical]`). A request's `option` selection is validated against this, and `Container.option` entries
gate which subtree renders. See `templates/avery5163.yaml` for the canonical multi-variant example.

## Notes

- `AGENTS.md` is a symlink to this file; edit `CLAUDE.md` and both stay in sync.
- `*.pdf` is gitignored; the sample PDFs in the repo root are local render artifacts.
- Fonts: Inter loads via `typst-kit` from the bundled `fonts/InterVariable.ttf`; Typst is told to use
  `"Inter Variable"`/`"Inter"`.
