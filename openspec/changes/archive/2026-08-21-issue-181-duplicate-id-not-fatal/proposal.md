## Why

Implements [#181](https://github.com/pfa230/labeler/issues/181).

A duplicate template id is the last template **content** problem that still kills startup: `TemplateRegistry::load_from_dir` returns `TemplateRegistryError::DuplicateId` (`src/templates.rs:132`), `main.rs:60` turns that into `fatal!`, and one copy-pasted YAML file in the config dir takes down every unrelated template with it. Everything else about a bad template was already made non-fatal by #175; this closes the gap so fatals stay reserved for broken infrastructure (unreadable templates dir, unopenable store, unbindable port).

## What Changes

- The templates directory load no longer fails on a duplicate id. The first file loaded for an id is served; each later file declaring that same id is **rejected** and reported in `broken[]`.
- Directory iteration becomes deterministic: entries are sorted by filename before loading, so "first" is a reproducible property of the directory contents rather than filesystem order. The lexicographically first filename wins the id.
- The rejection message names the id, the rejected file, and the file it collides with, and is produced from the existing `TemplateRegistryError::DuplicateId` variant, so `Reason::TemplateDuplicateId` stays live and `spec_documents_every_reason_and_invents_none` (`src/errors.rs:554`) keeps passing against the frozen `docs/SPEC.md` §10.1 table.
- `POST /api/templates/reload` no longer returns `422` for an on-disk duplicate id: it succeeds, reports the collision in `broken_count`, and picks up the fix once the operator renames or deletes a file.
- The already-accepted template is never ejected. A colliding file cannot evict the template that already holds the id within a load.
- **BREAKING** (operationally, not on the wire): a deployment that relied on startup refusing to run with duplicate ids now starts and serves the winner. `GET /api/templates` `broken[]` and the startup warning are how the collision surfaces.

## Capabilities

### New Capabilities
- `template-registry`: how the templates directory is loaded into the registry: which files are considered, what quarantines a file instead of aborting, how id collisions resolve, and how the rejected files are reported. Supersedes the `docs/SPEC.md` §3 sentence "An invalid template aborts server startup."

### Modified Capabilities
<!-- None. openspec/specs/ is empty; this is the first capability migrated out of the frozen spec. -->

## Impact

- `src/templates.rs`: entry collection and sorting move into a `sorted_dir_paths` helper, and `load_from_dir` pushes a `BrokenTemplate` for each collision instead of returning `Err`.
- `src/errors.rs:417`: the `TemplateRegistryError::DuplicateId` arm of `From<TemplateRegistryError> for AppError` stays (the variant is kept for its message and for the frozen §10.1 reason row).
- `src/main.rs:58`: startup keeps `fatal!` only for `TemplateRegistryError::Io`.
- `src/api.rs:133`: `reload` behavior is unchanged in shape; duplicates now land in `broken_count` rather than a `422`.
- No API schema change: `BrokenTemplateSummary` (`src/models.rs:36`) keeps `{ filename, error }` and `src/openapi.rs` is untouched. The generated document does change in two smaller ways: the now-unreachable `422` responses documented on reload and `DELETE` are removed, and the `broken[]` doc comments stop describing the list as parse/validation only. Two UI comments and one UI test stub that describe a post-write `422` are corrected to `500`.
- `docs/adr/`: adds an ADR recording the non-fatal duplicate rule and the filename-order tie-break.
