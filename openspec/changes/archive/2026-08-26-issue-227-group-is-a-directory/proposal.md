## Why

Implements [#227](https://github.com/pfa230/labeler/issues/227). A group has no identity: it is a string copied into every
template that belongs to it (ADR-0061), and the group list is recomputed by scanning the loaded
templates. Nothing on disk holds "the name of this group", so renaming one means rewriting the
`group:` line in every member file, and #202 becomes an N-file loop with partial-failure states and a
group that can end up split across two names.

The machinery that exists to make that loop safe is substantial and exists only for this:
`patch_template_group` does line-level YAML surgery to preserve comments and key order, refuses four
classes of file it cannot patch unambiguously, and publishes a `template_group_unpatchable` 422.
ADR-0062 exists to authorize it. A directory replaces all of it with `rename(2)`.

Two of ADR-0061's three arguments against directories have since been contradicted by the repo
itself. "Risks collisions across folders": the catalog is already nested and already enforces
filenames unique tree-wide, with a hard assert (#135). "Conflates physical storage with logical
categorization": the catalog is grouped by directory, flattened on install, and then has `category`
and `vendor` re-attached as fields in `index.json` — we build the structure, discard it, and
reconstruct it as metadata. The third, that nesting invites complexity, is answered by specifying
nesting rather than by refusing it.

## What Changes

- **BREAKING** A group is a directory under `{LABELER_CONFIG_DIR}/templates/`, at any depth. A
  template's group is its directory path relative to `templates/`, with `/` between segments;
  a file at the root of `templates/` is ungrouped. Renaming a group becomes one `rename(2)`.
- **BREAKING** A template's id is its filename stem, unique tree-wide. `id:` becomes an unknown
  top-level key and a file carrying it is quarantined. The id-vs-filename divergence class disappears
  with it, so nothing emits `template_id_mismatch` any more; the slug itself stays declared, because
  `docs/SPEC.md` §10.1 is frozen and the registry gate asserts against that table in both directions.
- **BREAKING** `group:` becomes an unknown top-level key on the same terms.
- **BREAKING** `POST /api/templates` is removed. `PUT /api/templates/{id}` creates or replaces, since
  the client supplies the id (RFC 9110 §9.3.4); `?group=<path>` places a newly created file, and
  `If-None-Match: *` makes it create-only, answering `412` where `POST` answered `409 TemplateExists`.
- The registry walks `templates/` recursively. **BREAKING** a `broken[]` entry becomes
  `{ path, error }` and `TemplateIdCollision.details.files` carries paths relative to `templates/`:
  a bare filename no longer identifies a file.
- `GET /api/templates?group=<path>` stays an exact match and gains `?nested=true`, which includes
  every descendant group. `?group=` (empty) still selects the ungrouped templates at the root.
- `GET /api/template-groups` lists every group in the tree. The group list can no longer be derived
  from the loaded templates, because a group now outlives its members.
- An empty directory is a real group: it is listed, and templates can be moved into it. Groups
  therefore outlive their last member, so `DELETE /api/template-groups/{path}` removes one, refusing
  with `409` unless the directory holds no template and no subdirectory. There is no create-group
  route: naming a new group in a move still creates it.
- A group name whose last segment differs from an existing sibling only by case is refused with
  `422`, so the group model does not depend on filesystem case folding.
- `PUT /api/templates/{id}/group` keeps its route and request shape and becomes a file move.
  `patch_template_group`, ADR-0062 and the `template_group_unpatchable` reason go away.
- The Labels view's group filter becomes a tree: nested folders read as nested groups, a node shows
  only its own templates, and an "include nested" switch widens it to the whole branch.
- **BREAKING** No migration ships, automatic or manual, and the service rewrites nothing under
  `templates/` on its own. A file still carrying `id:` or `group:` is simply invalid, and is refused
  and reported as broken like any other invalid template.
- No path the service writes to may cross a symbolic link. Group directories are resolved component
  by component and a symlinked component is refused, so a `templates/Outside -> /elsewhere` an
  operator plants cannot make a create or a move write outside the templates tree.

## Capabilities

### New Capabilities
<!-- None. Both the directory model and the group-delete route belong to the existing
     template-groups capability. -->

### Modified Capabilities
- `template-groups`: the group is a directory, not a field; group names are paths; nested filtering;
  the move endpoint moves a file; empty groups exist and are deletable; case-only clashes are refused;
  the Labels view browses a tree.
- `template-registry`: the load walks the tree recursively; the id comes from the filename; `id:` and
  `group:` are rejected keys; refused files are reported by relative path; `POST /api/templates` is
  replaced by a create-or-replace `PUT`; the service writes nothing to the templates tree unasked.

## Impact

- **Code**: `src/templates.rs` (recursive load, id from filename, `validate_group_name`, deletion of
  `patch_template_group`), `src/raw.rs` and `src/convert.rs` (drop `id` and `group` fields),
  `src/api.rs` (create/replace/move/delete routes, path resolution, `template_file_path`),
  `src/reason.rs` (remove `template_group_unpatchable`; keep `template_id_mismatch` declared but stop
  emitting it, since `docs/SPEC.md` §10.1 is frozen and the registry gate asserts against that table
  in both directions; add `template_group_case_conflict`, `template_group_mismatch`,
  `template_group_unsafe_path` and `unsupported_precondition`),
  `src/openapi.rs`, `src/store.rs` (favorites keyed by id are unaffected by a move), `src/main.rs`
  (its startup warning reads `b.filename`, which this change renames to `path`), `src/parse.rs` (the
  parser returns content, not a located template), `src/models.rs` (`BrokenTemplateSummary`'s field
  rename, and the create/replace payloads), `src/errors.rs` (`PreconditionFailed` added,
  `TemplateExists` removed with the endpoint that raised it), and `src/bin/catalog-index.rs` (it
  derives each id from the catalog file's own path).
- **Dependencies**: one addition, `rustix`, for `openat`/`O_NOFOLLOW` on the write path. `std`
  exposes neither, and without them the containment guarantee is lexical and therefore false.
- **Test corpus and docs**: every file under `tests/fixtures/templates/` loses its `id:` line too, or
  the suite loads nothing. `docs/AUTHORING.md` still teaches `id:` and a flat `group:` in its field
  table and repeats `id:` in both worked examples; it is not frozen, so leaving it would instruct
  readers to write templates the service quarantines.
- **Templates on disk**: every file in `catalog/` loses its `id:` line; `catalog/index.json` keeps
  carrying the id, which the UI now supplies on install. A template already installed in a live
  `{config}/templates` carries `id:`, so it becomes invalid and is reported as broken.
- **UI**: `ui/src/pages/Templates.tsx` (group tree, nested switch, move dialog, delete-group),
  `ui/src/pages/NewTemplate.tsx` (**BREAKING** for the page as it stands: its placeholder YAML opens
  with `id: my-label` and it creates through the removed `POST`, so it gains a separate id field and a
  group picker and submits a conditional `PUT`), `ui/src/pages/Catalog.tsx` (install becomes a
  conditional `PUT`, `412` where it read `409`), `ui/src/api/queries.ts` (`useCreateTemplate`),
  `ui/src/api/types.ts`.
- **ADRs**: supersedes ADR-0061 (group as a YAML field) and ADR-0062 (service may rewrite a single
  template key); revisits ADR-0058 (duplicate id refuses the file) for tree-wide filenames.
- **Deploy**: `docs/DEPLOY.md` records one fact, that a downgrade is lossy. An older binary scans
  only the root of `templates/` and reports an empty list, not a broken one, so any template in a
  directory is silently invisible to it.
- **Unblocks**: #202 (rename a group), whose mechanism shrinks to one directory rename.
