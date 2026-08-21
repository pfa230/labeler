## Context

See `proposal.md` — Why, and the requirements in `specs/template-registry/spec.md`.

Current state, verified in the tree at branch point:

- `TemplateRegistry::load_from_dir` (`src/templates.rs:66`) walks `read_dir` in filesystem order. `Parse` (`:109`) and `Validation` (`:121`) build a `TemplateRegistryError` only to render its `Display` into a `BrokenTemplate` and `continue`. `Io` (`:72`, `:78`, `:99`) and `DuplicateId` (`:132`) `return Err`.
- The only callers are `main.rs:58` (`fatal!` on `Err`) and `AppState::reload` (`src/api.rs:133`, `?` into `AppError`). `From<TemplateRegistryError> for AppError` (`src/errors.rs:404`) maps `Io` to `RenderFailed`/`template_registry_io` (500) and the other three to `TemplateInvalid` (422).
- `docs/SPEC.md` is frozen and lists `template_duplicate_id` in its §10.1 table (`docs/SPEC.md:714`). `spec_documents_every_reason_and_invents_none` (`src/errors.rs:554`) asserts the §10.1 table and `Reason::ALL` hold exactly the same slugs **in both directions**, so a slug cannot be dropped from the enum while the frozen table still lists it.
- `BrokenTemplate` (`src/templates.rs:44`) carries `{ filename, error }` where `filename` is a basename; `BrokenTemplateSummary` (`src/models.rs:36`) is its wire form.

## Goals / Non-Goals

**Goals:**

- One failure mode for every unusable template file: quarantine and report, never abort.
- A load result that is a pure function of the directory contents.
- No change to any wire schema, so the UI and `src/openapi.rs` are untouched.

**Non-Goals:**

- Reworking how broken templates are reported. `{ filename, error }` stays as is.
- Changing `POST`/`PUT`/`DELETE /api/templates` collision handling. `POST` already returns `409` for an id the registry holds and refuses to write over an existing path (`src/api.rs:402`), and `PUT` writes in place through the registry's path map.
- Repairing collisions automatically (renaming, merging, refusing to serve either file).

## Decisions

### ADR

This change adds **ADR-0058, "A duplicate template id refuses the file, not the server"**, plus its row in `docs/adr/README.md`. It records both the non-fatal rule and the filename-order tie-break, and supersedes nothing: no earlier ADR covers duplicate-id handling (ADR-0057 froze the spec; #175 landed quarantine without an ADR).

### Refuse the later file, in the existing loop position

The duplicate check stays where it is — after `parse_template` and `validate()`, before the id is inserted — and changes from `return Err(...)` to constructing the same `TemplateRegistryError::DuplicateId`, rendering its `Display` into a `BrokenTemplate`, logging the same `tracing::warn!` the other two quarantine paths log, and `continue`.

This makes all three content faults structurally identical in the loop, which is the point of the change. Reusing the variant's `Display` ("duplicate template id '{id}' found in {first} and {second}") gives a message that names the id and both files, matching the full-path style of the `Parse` and `Validation` messages, and satisfies the issue's "names both the id and the colliding filename" criterion without inventing a second message format.

Alternative considered: a bespoke message string built at the call site. Rejected — it would leave `DuplicateId` constructed nowhere, and the issue explicitly wants the rejection path to carry the typed reason rather than a bare string.

### Keep `TemplateRegistryError::DuplicateId` and its `AppError` arm

The variant stays even though it can no longer escape `load_from_dir`, exactly as `Parse` and `Validation` already do. `Reason::TemplateDuplicateId` must stay regardless: `docs/SPEC.md` §10.1 is frozen and still lists `template_duplicate_id`, and the both-directions completeness test fails the moment the enum stops declaring it. Keeping the variant keeps that reason attached to something real, and keeps `src/errors.rs:417` compiling — the prior attempt at this issue deleted the variant and left the match arm, which broke the build.

Alternative considered: delete the variant, the arm, and the reason. Rejected — it requires editing the frozen spec table to keep the test green.

### Filename order decides the winner

`read_dir` entries are collected into a `Vec`, sorted by file name, and then loaded, so "first wins" means "lexicographically first filename wins". Ordering by the `OsString` file name is total within one directory (names are unique), needs no `stat`, and is identical on every machine and in every container.

Chosen by the user over two alternatives:

- **Prefer a file literally named `<id>.yaml`/`<id>.yml`, then filename order.** Would stop a hand-copied sibling from ever taking an API-managed id, at the cost of a second rule to specify, test and explain.
- **Oldest mtime wins.** Literally "the file that was there first keeps the id", but mtimes are assigned by checkout, copy and image build, so the winner would not survive a redeploy — the exact non-determinism this change is removing.

The accepted consequence: adding a colliding file whose name sorts earlier moves the id to that file at the next load. Within a load nothing is ever ejected, which is what the issue requires; across loads the registry is rebuilt from scratch and has no memory of the previous winner.

### No `reason` field on broken entries

`BrokenTemplateSummary` keeps `{ filename, error }`. Giving clients a machine-readable discriminator would mean adding a field for all three quarantine kinds, registering it in `src/openapi.rs`, and superseding more of the frozen spec than this issue asks for. If the UI ever needs to tell a duplicate from a parse failure without matching prose, that is a separate issue.

### Sorting is specified, so it is tested directly

No test can force a filesystem to enumerate out of order, so the collision test (`a.yaml` and `z.yaml` sharing an id, both creation orders, `a.yaml` always served) pins the outcome but cannot prove the sort is what produced it — on a filesystem that already returns names in order it passes either way. The sort therefore lives in its own function, `sorted_dir_paths`, with a test asserting its output is ordered. That test has teeth wherever enumeration is unordered, which includes the Linux CI runner.

## Risks / Trade-offs

- **`POST`/`PUT /api/templates` can answer `2xx` with a different template than the caller submitted**, when the write creates a collision the registry did not already know about: the reload no longer fails, and `registry.detail(&id)` may resolve to the file that won. Verified against the code, not hypothetical → Out of this issue's scope (the proposal lists template CRUD collision handling as a non-goal), filed as [#184](https://github.com/pfa230/labeler/issues/184) with the one-line fix described.
- **A colliding file that sorts earlier silently takes over an id at the next reload** → It is not silent: the displaced file appears in `broken[]` with a message naming the collision, in the startup log and in the reload response. Deterministic and explainable beats mtime-dependent.
- **`DELETE /api/templates/{id}` removes the winning file, and the loser is then promoted at the following reload, so the id appears to survive a delete** → Consequence of keying on ids, made reachable by this change. The collision is visible in `broken[]` before the delete. Blocking or cascading the delete is out of this issue's scope, and is filed as [#183](https://github.com/pfa230/labeler/issues/183).
- **A duplicate id in the shipped catalog would now be quarantined instead of failing loudly in CI** → `template_ids_are_unique_and_match_filenames` (`src/render/mod.rs:4796`) reads the ids off disk itself and panics on a duplicate, independent of `load_from_dir`, so the CI gate is unaffected.
- **Operators who relied on startup refusing to run** → Called out in the proposal as an operational break; the startup warning and `broken[]` are the replacement signal, and this is the explicit intent of #181.

## Migration Plan

No data or config migration. The change is a behavior relaxation: a directory that started before still starts and serves the same templates, since a directory with no duplicates loads identically (the sort changes only iteration order, not the resulting map). Rollback is reverting the commit.

## Open Questions

None.
