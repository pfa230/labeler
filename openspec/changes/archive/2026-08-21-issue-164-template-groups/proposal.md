## Why

Implements [#164](https://github.com/pfa230/labeler/issues/164). Templates live in one flat namespace:
`GET /api/templates` returns every template sorted by id, and the Labels page renders them as one
grid with a substring search over id and name. Favorites and Recents are the only structure, and
both are per-user shortcuts rather than an organization of the set. Past a couple of dozen templates
the page stops being browsable, and there is no way to say "these five are the warehouse ones".

## What Changes

- **Template schema.** A new optional top-level `group:` string. One flat level, no separator
  semantics: `Electronics / Cables` is a name, not a path. A template without the key is *ungrouped*,
  which stays the default for every template that exists today.
- **API.** `group` is exposed on `TemplateSummary` and `TemplateDetail`. `GET /api/templates` gains a
  `group` query parameter: absent lists everything, `?group=<name>` lists exact matches, `?group=`
  (present, empty) lists the ungrouped ones.
- **New endpoint.** `PUT /api/templates/{id}/group` sets or clears one template's group by rewriting
  the single `group:` line of its file, leaving every other byte, comments included, untouched. This
  is what the UI's move action calls; it is a deliberate, narrow exception to the no-machine-writes
  rule of ADR-0006, and gets its own ADR.
- **Web UI.** The Labels page gains a group filter row (`All`, each group, `Ungrouped`) that composes
  with the existing search box, a per-card **Move to…** action, and a checkbox multi-select for
  moving several cards at once. The move dialog is a combobox over the groups in use where typing an
  unused name creates it. No group is ever typed into YAML to be assigned, and no client code
  rewrites template source.
- **Docs.** `docs/AUTHORING.md` documents the field. Two ADRs: the group model (YAML-native, flat),
  and the single-key file patch that backs the move endpoint.

Not breaking: every existing template, request, and response stays valid, and `group` is omitted
from responses when unset.

## Capabilities

### New Capabilities

- `template-groups`: how a template declares its group, what a valid group name is, how groups
  appear in and filter the template API, how a template is moved between groups, and how groups are
  browsed in the Labels view.

### Modified Capabilities

None. `template-groups` is additive: it introduces a field, a query parameter, and an endpoint, and
supersedes only the frozen `docs/SPEC.md` sections that enumerate those (§2 endpoint table, §2.0,
§3 top-level field table). No requirement in `openspec/specs/template-registry/spec.md` changes:
loading order, duplicate-id refusal, and broken-file reporting all behave identically whether or not
a file carries a `group:` key.

## Impact

- **Schema path:** `src/raw.rs`, `src/models.rs`, `src/convert.rs` (the three-file rule of ADR-0002),
  plus group-name validation and its `TemplateError` path.
- **API:** `src/api.rs` (list filter, the new route and handler, the line patcher), `src/models.rs`
  (`TemplateSummary`, `TemplateDetail`), `src/reason.rs` (a reason for an invalid group name),
  `src/openapi.rs` (the new parameter, request body, and route).
- **UI:** `ui/src/pages/Templates.tsx`, `ui/src/api/types.ts`, `ui/src/api/queries.ts`, plus a move
  dialog component and its tests.
- **Docs:** `docs/AUTHORING.md`, two new `docs/adr/` records and their `docs/adr/README.md` rows.
- **Not touched:** the render path, the batch path, favorites and recents, the catalog, and the
  templates directory layout, which stays flat.
- **Out of scope:** drag-and-drop onto a group chip, renaming or deleting a group across every
  template that uses it, nested groups, and per-group thumbnails. None of these is queued work: each
  gets its own issue if and when it is wanted.
