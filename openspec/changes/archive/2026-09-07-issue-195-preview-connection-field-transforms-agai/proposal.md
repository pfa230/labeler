## Why

Implements GitHub issue **#195**.

A connection-scoped field transform is written blind. The rule editor takes a regex in a text box, and
the only feedback is the save: `400 connection_transform_invalid` naming a rule index, or silence.
Silence is not success, because a pattern that compiles and collides with nothing can still match no
row at all, and nothing says so. The derived columns are visible after the fact through browse, but
only after a save, never beside the source value they came from, and never with a count of which rows
failed to match.

The editor makes the blindness worse. `source` is a free-text input and the resource list is a
hardcoded map in the UI (`ui/src/pages/connect/ConnectionsSection.tsx:17-19`) rather than the
connector's schema, so two of the save-time faults, `source` is not a field of that resource and
`source` is multi-valued, are reachable only by typing a name that a picker could have offered.

## What Changes

- New endpoint `POST /api/connections/{id}/transforms/preview`. It takes the **candidate** rules
  currently in the editor and the index of the one to preview, validates the whole list exactly as a
  save does, browses one page of that rule's resource upstream, and reports for **every** row: the
  source value, the derived fields and whether the rule matched, plus how many of the evaluated rows it
  matched. Nothing is written; the stored connection is untouched whether the preview succeeds or
  fails.
- The preview runs only against a **saved** connection. The credential and base URL come from the
  store, exactly as browse and materialize take them; no credential travels in a preview body, and
  there is no literal sample-string mode.
- **The request names one rule, and the response covers that rule.** This deviates from the request
  shape sketched in the issue, and it is forced: the issue requires a row's detail for *each* row and
  fixes `page_size` bounds at browse's 200, which a response covering as many as 32 candidate rules
  cannot satisfy under any bound that does not drop rows. One rule per request removes the
  multiplication instead, matches the issue's own per-rule panel, and costs no fidelity, because no
  rule can read another's output. The whole list is still validated, so ordering and collisions stay
  real. The request carries no separate `resource`, that being the named rule's own, and the change
  publishes no new `details.reason` slug: a rule naming an unknown resource is already
  `connection_transform_invalid`, and a `rule` index naming nothing is the `request_body_invalid` the
  error-envelope mapping already publishes.
- **BREAKING (UI)**: the field-transform rule editor moves to the **edit** form only. The create form
  drops it in favour of a line saying rules are added after saving, and sends no `transforms` key. The
  schema and the preview are both `{id}`-scoped, so a connection being created has neither, and
  keeping a free-text `source` alive for the create form would be a second spelling of the same field.
- The delta fixes the wire format rather than describing it: every request and response key, its type
  and whether it is always present, because only the synced spec is normative and two implementations
  of the same prose would not interoperate.
- The preview response is bounded by contract, without omitting anything the issue asks for:
  `page_size` follows browse's bounds (`1..=200`, default 10, `0` clamping to 1), the preview evaluates
  no more rows than that even if the upstream over-returns, and every reported value is truncated to
  512 bytes and flagged. Every evaluated row is reported; only value text is shortened, and matching
  runs on the whole value, so the counts and the derived key set are exactly a save's.
- `GET /api/connections/{id}/schema` gains three fields. `FieldSpec.transform_source` says whether a
  transform may source that column, computed from the same rule the save-time validation applies:
  `tier` cannot say it, because Homebox's `item_url` and `location_url` are `derived`-tier text columns
  a save accepts, while a transform-derived column is `derived`-tier and no save accepts it.
  `ResourceSpec.dynamic_source_prefix` reports the prefix under which the connector accepts a source it
  does not enumerate. `ResourceSpec.fields_incomplete` reports a resource whose runtime field discovery
  failed, which Homebox swallows today, answering with a short column list that looks complete.
- The rule editor becomes schema-driven, from that schema: `resource` is a select over the connector's
  real resource ids, and `source` offers exactly the sources a save accepts, which is the resource's
  `transform_source` columns plus, where a `dynamic_source_prefix` exists, naming a field under it. The
  operator types a field's name, never a source key, and the prefix comes from the schema.
  `CONNECTOR_RESOURCES` is deleted.
- Each rule in the editor gains a **Preview** control and a panel showing that rule's matched count
  over the evaluated rows and, per row, its source value and its derived fields. A displayed result is
  the answer to the last request made for that rule over the rules now on screen: any edit, addition,
  removal or reorder discards it, and of two overlapping requests for one rule only the later one's
  response is ever displayed, whichever arrives first.
- The editor is live only while the schema and the preview describe the connection the form is showing.
  A failed schema request suspends it; so does an edited **base url** or a typed **api key**, since both
  decide which upstream answers and as whom while the schema and every preview come from the stored
  connection. Suspended means read-only with the reason, no preview, displayed results discarded, and
  `transforms` omitted from the save, so no save can clear a rule the operator could not see.

Out of scope, per the issue: multi-valued sources (#350), template-field auto-mapping (#204), and any
change to how transforms are stored, validated or applied. This change adds a read-only view of the
shipped behaviour and a picker over the shipped schema.

The three additions to `GET /api/connections/{id}/schema` are named here because they are not free.
None changes what a transform means; all three exist because the issue requires the source control to
offer exactly what a save accepts, which the shipped schema cannot express: `tier` conflates
connector-derived columns with transform-derived ones, the accepted set includes an unbounded family of
prefixed names no column list can enumerate, and a swallowed discovery failure makes a short column
list indistinguishable from a complete one. Without them the control would silently offer the wrong
set, which is the fault this change exists to remove rather than relocate.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `connector-field-transforms`: five ADDED requirements. One states the complete `ResourceSpec`
  contract, first-touch, since that shape lives only in the frozen `docs/SPEC.md`; it carries
  `dynamic_source_prefix`, `fields_incomplete` and what a failed discovery does. One states which
  sources a transform may take and how the schema names them. One states the preview endpoint: its
  request, its validation, what it evaluates, every key it reports and that it writes nothing. One
  states the page and value bounds. One states the rule editor: its schema-driven controls, its
  per-rule panel, what it does when the schema request fails, when the connection details are edited,
  or when a stored rule names something the schema no longer offers, and which response it may display.
- `connector-multi-valued-fields`: MODIFIED "The connector schema marks each column's cardinality",
  because that requirement defines the whole `FieldSpec` shape and `transform_source` joins it. What
  decides the flag stays in `connector-field-transforms`; only the shape is stated here.
- `connections`: MODIFIED "Connection management on the Connect page", because the connection form no
  longer carries one rule editor for both cases: the create form has none and says so, and the edit
  form's editor is the schema-driven one specified under `connector-field-transforms`.
- `request-error-envelope`: MODIFIED "Every JSON endpoint returns the mapping". The requirement is
  already binding on an endpoint added later, but the scenario that holds it enumerates the endpoints
  that exist, and the preview endpoint reads a JSON body, so the enumeration gains it.

No new `details.reason` slug is published, so `src/reason.rs` and the reason-completeness test in
`src/errors.rs:665` are untouched.

## Impact

- `src/api.rs`: one route, one handler, and its request and response models. Validation reuses
  `Connectors::validate_transforms` and the same `rule {idx}: {msg}` message shape as `POST`/`PUT`.
- `src/connector/mod.rs`: one shared per-rule evaluation entry point on `CompiledTransforms`, which
  `apply_to_cells` is then written in terms of, so preview and browse cannot report different results;
  `transform_source` on `FieldSpec`, set from the predicate `validate_transforms` applies;
  `dynamic_source_prefix` and `fields_incomplete` on `ResourceSpec`. No change to what a transform
  means.
- `src/connector/homebox.rs`: the custom-field discovery reports its failure instead of swallowing it
  with `unwrap_or_default()`. The schema still succeeds; it just stops claiming the short list is whole.
- `src/openapi.rs`: the new path and every new model registered.
- `src/lib.rs`: HTTP tests for a matching rule, a rule matching no row, an invalid rule elsewhere in
  the list, an out-of-range `rule`, an unknown connection, the row cap against an over-returning
  upstream, value truncation, and that a preview leaves the stored `transforms` unchanged.
- `ui/src/api/connectors.ts`: preview request/response types and the call, plus the three new schema
  fields on `FieldSpec` and `ResourceSpec`.
- `ui/src/pages/connect/ConnectionsSection.tsx`: `CONNECTOR_RESOURCES` deleted, the resource select,
  the source control with its by-name choice, the create/edit split, the preview panel, the
  candidate-list revision and per-rule request sequence gating its results, and the suspension on
  edited connection details;
  `ConnectionsSection.test.tsx` covers the selects, the omission of columns a save would refuse, the
  by-name construction of a prefixed source, a panel rendering a matched count and a non-matching row,
  a result discarded when a rule is edited, the suspension when the base url is changed, and both
  asynchronous guarantees: an in-flight response stays discarded when the operator edits a rule and
  restores the exact original list before it resolves, and the earlier of two overlapping requests for
  one rule never overwrites the later response when it arrives last.
- No change to the stored connection schema, to `validate_transforms`, or to browse and materialize
  output.
