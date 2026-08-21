## 1. Schema: the `group` field

- [x] 1.1 Add `group: Option<String>` with `#[serde(default)]` to `TemplateDefinitionRaw` in `src/raw.rs`, keeping `deny_unknown_fields`.
- [x] 1.2 Add `group: Option<String>` to `TemplateDefinition` in `src/templates.rs`, and carry it through the `TryFrom<TemplateDefinitionRaw>` in `src/convert.rs`, stripping leading and trailing whitespace on the way.
- [x] 1.3 Add `validate_group_name(&str) -> Result<String, String>` (non-empty after stripping, at most 64 characters, no control characters) and call it from `TemplateDefinition::validate()` so a bad group quarantines the file with a message naming `group`.
- [x] 1.4 Unit tests: a template with a group loads and reports it; one without is ungrouped; a padded value is stripped; empty, whitespace-only, 65-character, and line-feed values each fail with a message naming `group`; a non-string value fails at parse with `group` in the path; `Shipping/Pallets` and `Shipping` stay unrelated.

## 2. API: exposure and filtering

- [x] 2.1 Add `group` to `TemplateSummary` and `TemplateDetail` in `src/models.rs` with `skip_serializing_if = "Option::is_none"`, and populate it in both `From<&TemplateDefinition>` impls in `src/templates.rs`.
- [x] 2.2 Add a `Query<TemplateListQuery>` with `group: Option<String>` to `list_templates` in `src/api.rs`: absent lists everything, a value filters by exact match after the same stripping, an empty value lists the ungrouped. Leave the id ordering and the `broken` list untouched.
- [x] 2.3 Register the new schema fields and the query parameter in `src/openapi.rs`.
- [x] 2.4 HTTP tests: a grouped summary carries `group`; an ungrouped response has no `group` key; `?group=Warehouse`, `?group=` and `?group=Nonexistent` each return the specified set; `?group=warehouse` returns none against `Warehouse`; `broken` is reported whatever the filter.

## 3. API: the move endpoint

- [x] 3.1 Add `Reason::TemplateGroupInvalid` (`template_group_invalid`) and `Reason::TemplateGroupUnpatchable` (`template_group_unpatchable`) to `src/reason.rs`, with the matching `AppError` constructors in `src/errors.rs`.
- [x] 3.2 Implement the line patcher: locate a top-level `group:` line, replace its value or delete the line, or insert one after the top-level `name:` line (falling back to `id:`, then the document body start). Preserve every other byte, each line's own terminator, and a trailing comment on the patched line, skipping a quoted scalar before looking for ` #`.
- [x] 3.3 Implement the refusals: more than one YAML document; a root that is not a block mapping written one key per line; a matched value that is not a plain or quoted scalar; and a parsed template that has a group while the scan finds no single line to replace, which must never insert a second key.
- [x] 3.4 Add `PUT /api/templates/{id}/group` to the router and a handler that takes `state.write_lock`, resolves the path through `existing_template_file`, validates the name, patches, re-parses and validates the result, asserts the group reads back, then writes atomically and reloads, returning the updated `TemplateDetail`.
- [x] 3.5 Register the route, its path parameter, its `{ "group": string | null }` body, every status and the new reasons in `src/openapi.rs`.
- [x] 3.6 Unit tests for the patcher: comments and key order survive a set, a change, and a clear; a quoted value keeps its quoting rules and a `group: "A # B"  # keep me` line keeps both; a CRLF file stays CRLF; a nested `group:` key inside `params` or a layout item is untouched; a `"group":` key, a flow-mapping root, a multi-document file and a block-scalar value are each refused unchanged.
- [x] 3.7 Byte-equality test across the whole `catalog/` tree: set, then clear, each template's group and assert every line except the patched one is identical to the original.
- [x] 3.8 HTTP tests: `200` with the updated detail; `404` for an unknown id; `400` for a bad id or body; `422` for a name that fails validation and for an unpatchable file, each leaving the file unchanged; idempotent set and clear leave the file byte-identical.

## 4. Web UI

- [x] 4.1 Add `group?: string` to the template types in `ui/src/api/types.ts`, and a `useMoveTemplateGroup` mutation in `ui/src/api/queries.ts` that invalidates the templates query.
- [x] 4.2 Add the group filter row to `ui/src/pages/Templates.tsx`: `All`, each group in use in ascending code-point order (compare `Array.from(name)`, not `<` or `localeCompare`), and `Ungrouped` only while something is ungrouped. Compose it with the search box, and hide the Favorites and Recents rows while a group other than `All` is selected.
- [x] 4.3 Show each card's group, and add the **Move to…** action and the move dialog: an input over a datalist of the groups in use, accepting an unused name, plus a way to make the template ungrouped.
- [x] 4.4 Add checkbox multi-select and a selection bar that moves the selected templates with `Promise.allSettled`, reporting per-template successes and failures.
- [x] 4.5 Component tests in `ui/src/pages/Templates.test.tsx`: filtering by group; group and search composing; the empty-state message when they match nothing; `Ungrouped` absent when everything is grouped; Favorites and Recents hidden under a filter and back on `All`; a move updating the card without a reload; naming a new group; a bulk move reporting a partial failure; a favorited template staying favorited across a move.

## 5. Docs and decisions

- [x] 5.1 Write ADR-0062, "A template's group is a YAML field, not its directory", covering the flat single level and the rollback consequence that an older binary quarantines a file carrying `group:`. Confirm the number against `main` first: 0059 through 0061 are claimed by changes in flight.
- [x] 5.2 Write ADR-0063, "The service may rewrite one key of a hand-authored template", qualifying ADR-0006 and naming the byte-preservation guarantee its tests enforce.
- [x] 5.3 Add both rows to `docs/adr/README.md`.
- [x] 5.4 Document `group` in `docs/AUTHORING.md`, including that it is one flat name and that a slash in it is just a character. Do not touch the frozen `docs/SPEC.md`.

## 6. Verify

- [x] 6.1 Run `cargo fmt`, `cargo clippy --all-targets --all-features`, and `cargo test`; fix any lint at its root rather than silencing it.
- [x] 6.2 Run the UI tests and lint (`npm test`, `npm run lint` in `ui/`).
- [x] 6.3 Exercise the loop by hand against a running server (`LABELER_CONFIG_DIR=./config-dev`, `LABELER_NO_AUTH=true`): move a catalog template into a group, `diff` the file against its pre-move copy to confirm one changed line, filter the Labels page by that group, bulk-move two templates, and clear one back to ungrouped.
- [x] 6.4 Render one moved template to PNG and open it, confirming the move changed nothing about how the label draws.
