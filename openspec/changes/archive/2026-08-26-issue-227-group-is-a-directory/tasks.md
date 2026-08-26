## 1. Domain model: identity leaves the file

- [x] 1.1 Split `TemplateDefinition` into `TemplateContent` (name, description, unit, dpi, format, params, layout, version) and `TemplateDefinition { id, group, content }`, with `Deref` and `DerefMut` to the content so existing field access and the render tests' assignments keep compiling.
- [x] 1.2 Remove `id` and `group` from `TemplateDefinitionRaw` in `raw.rs`, and drop their handling from the `TryFrom` in `convert.rs`.
- [x] 1.3 Change `parse_template` to return `TemplateContent`, and move `validate()` onto it, deleting the checks that read an embedded id or group.
- [x] 1.4 Rework `instantiate_with_defaults` to clone the identity and rebuild only the content.
- [x] 1.5 Update every direct `parse_template` caller to supply identity from where it knows it: the loader from the path, the write endpoints from the route, `src/bin/catalog-index.rs` from the catalog file's path, and the render tests' `parse_and_validate` helper from its call sites.

## 2. Registry: walk the tree

- [x] 2.1 Replace `sorted_dir_paths`/`load_from_dir` with a recursive walk that collects `(relative_path, absolute_path)` and sorts by the relative path's raw bytes, using `symlink_metadata` so symlinked directories are not descended.
- [x] 2.2 Skip every directory whose name begins with `.` together with its subtree, and make that skip outrank invalid-directory reporting at any depth.
- [x] 2.3 Derive each template's id from its filename stem and its group from its parent directory path, refusing a stem that is not `[A-Za-z0-9_-]+`.
- [x] 2.4 Refuse every template beneath a directory whose name fails group-name validation, reporting the directory and the rule, without affecting templates elsewhere.
- [x] 2.5 Refuse a file or directory whose name is not valid UTF-8, reporting the path lossily and saying so, and keep ordering over raw bytes.
- [x] 2.6 Restrict id-contest eligibility to loadable files (valid UTF-8 path, no invalid directory on the path, valid stem, content parses and validates) and serve the first such file in relative-path order.
- [x] 2.7 Rename `BrokenTemplate.filename` to `path`, carry paths relative to the templates directory, and update `src/models.rs`, `src/main.rs`'s startup warning, and the OpenAPI schema.
- [x] 2.8 Wrap the parse error so a rejected `id`/`group` key reports as the ordinary unknown-field failure it is, with no special diagnosis.

## 3. Safe filesystem access

- [x] 3.1 Add `rustix` and build the component-wise resolver: from a handle on the templates directory, resolve each segment by exact entry name, `openat` with `O_NOFOLLOW | O_DIRECTORY` when it exists, `mkdirat` when it does not, reading `EEXIST` from `mkdirat` as a filesystem case-alias.
- [x] 3.2 Route every mutation through it fd-relative: `renameat_with(NOREPLACE)` for a move with `linkat`+`unlinkat` as the fallback, `renameat` for a replace, `mkdirat`, `unlinkat`, `unlinkat(AT_REMOVEDIR)`.
- [x] 3.3 Refuse a symlinked source, destination, or staging name, answering `422`/`400` when the caller supplied the path and `500` when the service derived it, with `details.reason` `template_group_unsafe_path`.
- [x] 3.4 Implement pre-publication cleanup: a request refused before it publishes removes the directories it created, innermost first, stopping at the first non-empty one; a request that has published removes nothing.

## 4. Group names and the group surface

- [x] 4.1 Rewrite `validate_group_name` for paths: trim the whole path, split on `/`, and check each segment for non-empty, ≤64 chars and ≤255 bytes, no control characters, none of `/ \ < > : " | ? *`, not `.` or `..`, no leading/trailing whitespace, no leading or trailing `.`, and not a reserved device name including the superscript `COM¹`/`LPT¹` forms and extension-bearing spellings; cap the whole path at 255 chars and 1024 bytes.
- [x] 4.2 Add the case-clash check against exact sibling entries using `str::to_lowercase`, answering `422` `template_group_case_conflict`, and treat a `mkdirat` `EEXIST` with no exact match as the same refusal.
- [x] 4.3 Add `GET /api/template-groups` returning every group path in code-point order, including empty and intermediate directories, excluding dot-directories and invalid names.
- [x] 4.4 Add `DELETE /api/template-groups/{*path}`: reject a malformed percent sequence on the raw encoded path before decoding, resolve every component by exact entry name (`404` on a case mismatch), and remove the directory with `unlinkat(AT_REMOVEDIR)`, mapping `ENOTEMPTY` to `409` and `ENOENT` to `404`.
- [x] 4.5 Add `nested` to `GET /api/templates`, widening a `group` filter to descendants by whole path segments so `Shipping2` is not beneath `Shipping`.
- [x] 4.6 Delete `patch_template_group`, its tests, and the `TemplateGroupUnpatchable` reason.

## 5. Write endpoints

- [x] 5.1 Rewrite `PUT /api/templates/{id}/group` as a move: validate the group path, re-read the tree, refuse an occupied destination with `409`, relocate the file, confirm afterwards, and leave the emptied source directory in place.
- [x] 5.2 Make `PUT /api/templates/{id}` create-or-replace, taking `?group=` for a create, rejecting it on a replace whose group differs with `400` `template_group_mismatch`.
- [x] 5.3 Implement `If-None-Match: *` as create-only answering `412 PreconditionFailed`, and reject any other `If-None-Match` value with `400` `unsupported_precondition`.
- [x] 5.4 Implement the single re-classification from create to replace when the destination appears mid-request, after establishing the destination is not a symlink, with a second exclusive failure answering `500`.
- [x] 5.5 Remove `POST /api/templates`, its handler, its route and the `TemplateExists` error code.
- [x] 5.6 Restrict delete-collision refusal to contenders, so a namesake under a dot-directory or an invalid location does not block a delete, and carry only contenders in `details.files`.
- [x] 5.7 Keep `Reason::TemplateIdMismatch` declared with no emit site; add `TemplateGroupCaseConflict`, `TemplateGroupUnsafePath`, `TemplateGroupMismatch` and `UnsupportedPrecondition`.
- [x] 5.8 Register every changed model, route, parameter, header and status in `src/openapi.rs`.

## 6. UI

- [x] 6.1 Add `group` handling and the group tree to `ui/src/pages/Templates.tsx`: nested nodes in code-point order, synthetic `All`/`Ungrouped` kept distinguishable from real groups of the same name, and nodes identified by path rather than label.
- [x] 6.2 Add the include-nested switch, off by default and inert under `All` and `Ungrouped`.
- [x] 6.3 Rework the move dialog to offer the group tree, accept a new path, offer ungrouping, and report `422` and `409` distinctly.
- [x] 6.4 Add group deletion, enabled only for a group with no templates and no subgroup, reporting `409`.
- [x] 6.5 Rework `ui/src/pages/NewTemplate.tsx`: a validated id field, a group picker, a placeholder without `id:`/`group:`, and a conditional `PUT` reading `412` against the id field.
- [x] 6.6 Change `ui/src/pages/Catalog.tsx` install to a conditional `PUT` taking the id from `catalog/index.json`, reading `412` where it read `409`.
- [x] 6.7 Update `ui/src/api/queries.ts` and `ui/src/api/types.ts` for the new routes, the `broken[].path` rename and the removed create mutation.

## 7. Corpus and docs

- [x] 7.1 Remove the `id:` line from all five `catalog/` templates and confirm `catalog/index.json` still generates.
- [x] 7.2 Remove `id:` from every file under `tests/fixtures/templates/`, and make `load_all_for_tests` copy the catalog tree structure instead of flattening it.
- [x] 7.3 Update `docs/AUTHORING.md`: drop `id:` and `group:` from the field table and both worked examples, and describe directories as groups.
- [x] 7.4 Add the downgrade-is-lossy note to `docs/DEPLOY.md`.
- [x] 7.5 Write `docs/adr/0073-group-is-a-directory-id-is-the-filename.md`, recording the read-side symlink boundary, and add its row to `docs/adr/README.md` while marking ADR-0061 and ADR-0062 superseded; re-check the highest ADR number on `main` first.

## 8. Verification

- [x] 8.1 Cover the load path: nesting, dot-directory precedence, invalid directory names, non-UTF-8 names, bad stems, and id contests won by location-valid files.
- [x] 8.2 Cover the write endpoints: move, create, replace, conditional create, re-classification, symlink refusals at every position, and pre-publication cleanup.
- [x] 8.3 Cover the group surface: listing including empty and intermediate groups, exact and nested filtering, case clashes, case-mismatched deletes, and malformed percent sequences.
- [x] 8.4 Prove each new test fails before its implementation, so no assertion passes against the old behaviour.
- [x] 8.5 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test`, fixing root causes rather than silencing lints.
- [x] 8.6 Run the UI test suite and build.
- [x] 8.7 Start the service with `LABELER_CONFIG_DIR=./config-dev`, install a catalog template into a nested group, render it to PNG via `POST /api/render/label?format=png`, and open the image to confirm the label is correct rather than merely rendered.
