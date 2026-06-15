# Design: M2 template management — hot-reload (#7), CRUD (#10), starter library (#11)

**Date:** 2026-06-15
**Issues:** #7 (hot-reload), #10 (upload/replace/delete API), #11 (starter library). Milestone M2.
**Status:** Approved design, pre-implementation.

## Context

Templates load once at startup into an immutable `TemplateRegistry`, shared as `Arc<TemplateRegistry>`
across `app()`, the four handlers, and `main.rs`. M2 makes the registry mutable at runtime so templates
can be reloaded (#7) and created/replaced/deleted over the API (#10), and ships more starter templates
(#11). Templates remain hand-authored YAML files in the `templates/` dir (ADR-0006); the GUI-owned store
is Phase 2.

## Decisions (approved)

- **Reload:** an explicit `POST /templates/reload` endpoint plus automatic reload after every CRUD
  mutation. No file watcher (can be added later).
- **Concurrency:** `arc-swap` — `ArcSwap<TemplateRegistry>` for lock-free reads and atomic whole-registry
  swap on reload. Adds the `arc-swap` dependency.
- **CRUD body:** raw template YAML (the authoring format), parsed/validated with the existing loader.
- **Starter library:** the Brother tape family `brother12mm` (exists) + new `brother18mm`, `brother24mm`,
  plus the existing `avery5163` sheet exemplar. No Dymo/Avery-5160 for now.

## A. Foundation: runtime-mutable registry

```rust
pub struct AppState {
    templates: ArcSwap<TemplateRegistry>, // lock-free reads, atomic swap
    templates_dir: PathBuf,               // source for reload + CRUD writes
    write_lock: tokio::sync::Mutex<()>,   // serialize mutations
}
```

- `app()` and all handlers take `State<Arc<AppState>>` instead of `State<Arc<TemplateRegistry>>`.
- Read handlers call `state.templates.load_full()` to get an owned `Arc<TemplateRegistry>` snapshot
  (Send-safe across the async handler) and use `summaries()`/`detail()`/`get()` on it. The snapshot
  keeps the registry alive across rendering, so the render handlers hold it for the duration (no clone of
  `TemplateDefinition` needed).
- `AppState::reload()` rebuilds via `TemplateRegistry::load_from_dir(&templates_dir)`; on success it
  `store`s the new `Arc`, on failure it returns the error and **keeps the current registry** so a broken
  file on disk never takes the service down.
- `main.rs` constructs the `AppState` (from the `templates` dir) and passes `Arc<AppState>` to `app()`.
- Filesystem work (reload, CRUD writes) runs synchronously in the handlers. Acceptable for the
  single-user, local-dir target and consistent with the synchronous Typst render path; `spawn_blocking`
  is a future option if it ever serves large dirs or remote storage.

This refactor is shared by #7 and #10 and lands in the #7 commit.

## B. #7 — Reload endpoint

`POST /templates/reload` calls `AppState::reload()` and returns `200 { "count": N }` on success, or the
parse/validation error (`4xx`) with the previous registry left intact.

- **AC:** editing a file then calling reload updates `GET /templates`; a newly-invalid file makes reload
  return an error while the service keeps serving the previously-loaded set; tested.

## C. #10 — Create / replace / delete

All write endpoints take a raw YAML body (read via axum's `String` extractor) and parse it with
`parse_template` + `validate()`. API-managed files use the `<id>.yaml` filename convention under
`templates_dir`. Mutations are serialized by `write_lock`; each ends by calling `reload()`.

| Method | Path | Body | Behavior |
| --- | --- | --- | --- |
| POST | `/templates` | YAML | Create. `id` from body; `409 Conflict` if it already exists; write `<id>.yaml`; reload; `201`. |
| PUT | `/templates/{id}` | YAML | Replace. Body `id` must equal path `{id}` (else `400`); `404` if absent; overwrite `<id>.yaml`; reload; `200`. |
| DELETE | `/templates/{id}` | — | Delete `<dir>/<id>.yaml` (`404` if absent); reload; `204`. |

- **Writes are atomic:** write to a temp file then rename over `<id>.yaml`.
- **Validation order:** parse (YAML errors → `400`, path-aware via `TemplateError`) → `validate()`
  (semantic errors → `422`) → id-collision check (POST → `409`) → write → reload. Validate-before-write
  guarantees the written file is valid.
- **Error model additions:** a `409 Conflict` (`TemplateExists`) constructor on `AppError`, and a mapping
  from `TemplateError` (parse) / validation `String` to `AppError`.
- **Edge:** if another hand-edited file in the dir is invalid, the post-write `reload()` surfaces that
  error even though this write persisted. Acceptable for a single-user self-hosted tool; documented.
- **AC:** POST/PUT/DELETE work and persist across restart; invalid YAML rejected before any write;
  duplicate id rejected; tested.

## D. #11 — Starter library

- Keep `avery5163` (sheet exemplar) and `brother12mm`.
- Add `brother18mm` and `brother24mm`: single-format continuous-tape templates. **Heights use the
  nominal tape width (18/24 mm)** to match the existing `brother12mm` convention. Using the reduced
  *usable printable height* across the whole tape family (including `brother12mm`) is a deferred
  refinement that needs the per-tape Brother spec; doing it for only the new two would be inconsistent.
  Each has a layout appropriate to its height (QR + auto-fit text), not a stretched copy of the 12 mm
  layout.
- Each new template renders correctly to PNG via a smoke test, and is listed in the README/SPEC template
  notes.
- **AC:** the new templates are present on a fresh install, render to their format, and are covered by a
  render smoke test.

## E. Delivery and testing

- One branch `feat/m2-template-management`, three commits: `Fixes #7` (incl. the arc-swap foundation),
  `Fixes #10`, `Fixes #11`. Each marks its plan entry DONE (commit hash) in `docs/PLAN-phase-1.md`.
- New dependency: `arc-swap`.
- Tests: reload endpoint (success + invalid-file-keeps-old); CRUD round-trip (create → appears in
  `/templates` → replace → delete → gone), duplicate-id `409`, invalid-YAML rejection, id/path mismatch
  `400`; render smokes for the new tapes. Existing tests updated for the `AppState` signature change.
- After implementation: one adversarial review pass; `cargo fmt --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` all clean. Merge to `main`,
  push.
- SPEC updated: new reload/CRUD endpoints (§2), the template-store/ownership note, and the template list;
  changelog entries.

## Out of scope

GUI-owned template store and the editor (Phase 2, ADR-0006); file watcher; multi-template batch
concerns; per-template asset roots.

## Acceptance criteria

- Registry is runtime-mutable; reads stay lock-free; a broken file never crashes the service.
- `POST /templates/reload` and the CRUD endpoints behave as tabled above, persist across restart, and
  reject invalid input with the stable error contract.
- `brother18mm` and `brother24mm` ship and render; `avery5163` and `brother12mm` still render.
- Full verification suite (fmt/clippy `-D warnings`/test) green; SPEC and plan updated.
