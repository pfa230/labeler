# 21. Homebox & Connect hardening

## Status

Accepted. Implements milestone M7 ([#60](https://github.com/pfa230/labeler/issues/60),
[#61](https://github.com/pfa230/labeler/issues/61), [#62](https://github.com/pfa230/labeler/issues/62),
[#58](https://github.com/pfa230/labeler/issues/58), [#59](https://github.com/pfa230/labeler/issues/59)).
Refines [ADR-0018](0018-api-integration-spine.md) (the API integration spine); does not supersede it.

## Context

ADR-0018 shipped the connector spine and the Homebox connector. Hands-on testing surfaced correctness and
UX defects: the `entities` resource mixed items and locations in one list; locations were browsed via
`/v1/entities/tree`, whose node carries only `id/name/type/children`, so the `description`/`itemCount`
columns rendered empty; browse rows had no link back to Homebox; the Connect page asked for a template
before connecting; and the row selection persisted across filter/drill/tab changes but off-view selections
were invisible, so a bulk "add" silently materialized rows the user could not see ("ghost selection").

## Decision

- **Items vs locations via `isLocation`.** Homebox's `GET /v1/entities` (v0.26 entity-merge) returns all
  entities by default and honors an **`isLocation` boolean** the swagger does not document but the handler
  applies (verified in `repo_entities.go`/`v1_ctrl_entities.go`). The connector exposes two flat resources:
  items (`isLocation=false`) and locations (`isLocation=true`). The `/v1/entities/tree` path is dropped;
  locations now come from the flat `EntitySummary`, which carries `description` and `itemCount`. Drill-into-
  contents (the `parentIds` relationship) is unchanged.
- **Row link.** Each browse row carries an optional `url` (`DisplayRow.url`) = `{base_url}/entity/{id}`
  (the same form the materialize path already used), and the table renders the row name as a link. The
  mappable `item_url`/`location_url` derived columns are **kept** (the link is navigation; the column is a
  field-mapping target, so a template can still bind the Homebox URL into label data).
- **Connect layout.** The page header is Connection-only; template selection and field mapping render
  above the browser. Flow: connection -> template + mapping -> browse/select -> add -> grid -> batch.
- **Visible cross-view selection.** Cross-view selection stays (selecting from several locations into one
  batch is intended), but is made transparent: the selection model carries a display snapshot per row
  (`SelectedRow { resource, key, label, breadcrumb?, lastSeen }`, captured at toggle); a persistent summary
  shows the whole selection with a visible/hidden split where "in this view" means the currently-loaded
  rows; a reviewable, removable list groups selections by resource; and selecting is blocked at the 200-row
  materialize cap. Browse switches clear the loaded rows so the split reflects the current view.

## Consequences

- The items/locations split depends on an **undocumented** Homebox query param. If Homebox removes
  `isLocation`, the two resources stop separating. The source of truth for `/v1/entities` parameters is the
  handler source, not the swagger (which also omits `negateTags`, `includeArchived`, `fields`, `orderBy`).
- **Stale-selection handling is deferred.** A row that disappears upstream between browse and materialize
  is not surfaced as "Unavailable"; it fails the whole materialize (which aborts on any per-row error).
  Graceful handling needs a `/materialize` partial-result contract, out of scope here.
- The flat locations list uses Homebox's server-side order (deterministic per query); no client-side
  per-page sort is applied (it would be misleading under pagination). True alphabetical ordering could use
  the undocumented `orderBy` param later.
- `View::Tree` is now unused by the Homebox connector (locations render as a `Table`); the variant remains
  in the schema model for future connectors.
