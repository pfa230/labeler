## 1. Schema fields the source control needs

- [x] 1.1 Add `transform_source` (bool, always serialized) to `FieldSpec` in `src/connector/mod.rs`,
      set from the same predicate `validate_transforms` applies to a connector's own column
      (single-valued and `FieldType::Text`, whatever its `tier`), and set `false` on the derived
      columns `Connectors::schema` appends.
- [x] 1.2 Add `dynamic_source_prefix` (`Option<String>`, always serialized, `null` when none) and
      `fields_incomplete` (bool, always serialized) to `ResourceSpec` in `src/connector/mod.rs`.
- [x] 1.3 In `src/connector/homebox.rs`, report each resource's `dynamic_source_prefix` from its
      `ResourceDescriptor::dynamic_text_prefix` (`"custom:"` for `entities`, `null` for `locations`).
- [x] 1.4 In `src/connector/homebox.rs`, replace the `unwrap_or_default()` on the custom-field
      discovery call so a failed fetch sets `fields_incomplete` `true` on `entities` and the schema
      request still succeeds with the connector's declared columns.
- [x] 1.5 Unit tests in `src/connector/`: `item_url` and `location_url` carry `transform_source`
      `true`; `tags` and `quantity` carry `false`; a transform-derived column carries `false`; each
      resource reports its prefix; a refused custom-field fetch yields `fields_incomplete` `true` with
      the declared columns; a successful one yields `false` plus a column per custom field.

## 2. One evaluation path for browse and preview

- [x] 2.1 Add a per-rule evaluation entry point to `CompiledTransforms` in `src/connector/mod.rs`
      returning, for a resource and a row's cells, one outcome per matching rule: the rule's index, the
      source value it read if the cell held text, and its captures if it matched.
- [x] 2.2 Rewrite `apply_to_cells` as a caller of that entry point, inserting the captures, and run the
      existing `transform_pass_*` unit tests unchanged to show the behaviour is preserved.

## 3. The preview endpoint

- [x] 3.1 Add the request model (`transforms`, `rule`, optional `page_size`) and the response models
      (`rule`, `resource`, `source`, `row_count`, `matched_count`, `rows[]` of `id`, optional
      `source_value`, `matched`, `value_truncated`, optional `derived`) to `src/api.rs`, with the
      presence rules the delta states.
- [x] 3.2 Add the handler and the route `POST /connections/{id}/transforms/preview` in `src/api.rs`,
      using `crate::extract::Json`/`Path` and `load_conn_and_connector`, so an unknown connection is
      `404`.
- [x] 3.3 Reject a `rule` index that names no entry of `transforms` with `400 InvalidRequest` and
      `details.reason` `request_body_invalid`, before any upstream call.
- [x] 3.4 Validate the whole candidate list through `Connectors::validate_transforms`, returning
      `400 InvalidRequest`, `details.reason` `connection_transform_invalid` and the same
      `rule {idx}: {msg}` message shape `POST`/`PUT` use, before any upstream call.
- [x] 3.5 Browse one page of the named rule's resource: `page_size` defaults to 10 and is passed to the
      shipped `BrowseRequest` clamp of `1..=200`, no cursor is accepted and none is returned.
- [x] 3.6 Truncate the returned page to the effective page size before evaluating, so an over-returning
      upstream cannot enlarge the preview, and report that count as `row_count`.
- [x] 3.7 Evaluate the named rule through the entry point from task 2.1 and build one `rows` entry per
      evaluated row in browse order, omitting no row, with `source_value` absent when the row carries
      no text value for the source and `derived` absent unless `matched`.
- [x] 3.8 Truncate every reported value to 512 bytes on a character boundary and set `value_truncated`
      on any row entry whose value was cut, matching against the whole value so `matched`, the
      `derived` key set and both counts are unaffected.
- [x] 3.9 Register the new path and every new model in `src/openapi.rs`.

## 4. HTTP tests for the endpoint

- [x] 4.1 A matching rule over a 10-row page returns `200`, `row_count` 10, the expected
      `matched_count`, 10 `rows` entries, and a matched row's `source_value` beside its `derived`
      fields.
- [x] 4.2 A rule matching no row returns `200` with `matched_count` 0 and every row entry carrying
      `matched` `false` and no `derived`.
- [x] 4.3 A row carrying no text value for the source reports no `source_value` and `matched` `false`,
      while a row whose value did not match reports its `source_value`; a group capturing the empty
      string reports the key with `""`.
- [x] 4.4 A candidate rule elsewhere in the list with no named capture group returns `400`,
      `details.reason` `connection_transform_invalid`, a message naming that rule's index, and no
      upstream request; the stored connection is unchanged.
- [x] 4.5 Two candidate rules deriving the same name on one resource return `400` with
      `connection_transform_invalid`.
- [x] 4.6 An out-of-range `rule` returns `400` with `details.reason` `request_body_invalid`, and an
      unknown connection id returns `404`.
- [x] 4.7 `page_size` 500 asks the upstream for 200 rows and `page_size` 0 asks for 1.
- [x] 4.8 An upstream returning 50 rows for a 10-row request yields `row_count` 10, 10 `rows` entries,
      and a `matched_count` over those 10 only.
- [x] 4.9 A 4000-byte source value that matches reports a `source_value` of at most 512 bytes with
      `value_truncated` `true`, and is still counted in `matched_count`.
- [x] 4.10 A preview carrying candidate rules different from the stored ones, and a preview that is
      refused, both leave the stored connection's `transforms` exactly as they were.
- [x] 4.11 A syntactically invalid JSON body to the new endpoint returns `400`, `InvalidRequest` and
      `details.reason` `json_malformed`, as every JSON endpoint does.

## 5. UI API layer

- [x] 5.1 In `ui/src/api/connectors.ts`, add `transform_source` to `FieldSpec` and
      `dynamic_source_prefix` and `fields_incomplete` to `ResourceSpec`.
- [x] 5.2 In `ui/src/api/connectors.ts`, add the preview request and response types and the call to
      `POST /connections/{id}/transforms/preview`.

## 6. The rule editor

- [x] 6.1 Delete `CONNECTOR_RESOURCES` from `ui/src/pages/connect/ConnectionsSection.tsx` and split the
      form: the create form renders no rule editor, states that rules are added after saving, and sends
      no `transforms` key.
- [x] 6.2 Drive the rule's **resource** from a select over the schema's resource ids.
- [x] 6.3 Drive the rule's **source** from a control offering that resource's `transform_source`
      columns plus, when `dynamic_source_prefix` is not `null`, a choice that takes a field's name and
      composes `source` as prefix followed by name, with no way to type a source outside that
      construction.
- [x] 6.4 Reset a rule's source to a source of the new resource when its resource changes, and state
      when a resource offers neither a `transform_source` column nor a prefix.
- [x] 6.5 Say the field list may be short wherever a resource carrying `fields_incomplete` `true`
      offers its sources, keeping the by-name choice available.
- [x] 6.6 Show a stored `resource` or `source` the schema does not offer as its own option marked
      unavailable, leaving the rule's value unchanged.
- [x] 6.7 Suspend the editor, read-only with the reason and with `transforms` omitted from the save,
      when the schema request failed, and when the form's base url differs from the stored one or an
      api key has been typed; in the second case say the connection details must be saved first, offer
      no preview and discard displayed results. Make a successful save refetch the schema so the editor
      goes live against the saved details.
- [x] 6.8 Add a per-rule preview control that sends every candidate rule in editor order with that
      rule's index, and a panel showing `matched_count` against `row_count` and, per row, the
      `source_value` and either the `derived` fields or that the rule did not match, reading a missing
      `source_value` differently from an empty one and saying when `value_truncated` is set.
- [x] 6.9 Report a refusal naming a rule index against that rule, including when it is not the rule
      being previewed.
- [x] 6.10 Gate a displayed result on a monotonic candidate-list revision, incremented on every edit to
      a rule's resource, source or pattern and on every add, remove or reorder, captured when the
      request is issued, together with a per-rule request sequence number; display a response only
      while both are still current.

## 7. UI tests

- [x] 7.1 The resource select offers exactly the schema's resource ids.
- [x] 7.2 The source control offers the `transform_source` columns, offers neither a multi-valued nor a
      transform-derived column, and offers the by-name choice only where a `dynamic_source_prefix`
      exists.
- [x] 7.3 Naming a field under the prefix composes the rule's `source` as prefix followed by name.
- [x] 7.4 A resource carrying `fields_incomplete` `true` is disclosed as possibly short.
- [x] 7.5 The create form renders no rule editor and its request carries no `transforms`.
- [x] 7.6 A preview panel renders the matched count and a non-matching row.
- [x] 7.7 Changing the base url discards a displayed result, offers no preview, and makes the save send
      no `transforms` key.
- [x] 7.8 An in-flight response stays discarded when a rule is edited and the exact original list is
      restored before it resolves.
- [x] 7.9 The earlier of two overlapping requests for one rule does not overwrite the later response
      when it arrives last.

## 8. Gates

- [x] 8.1 `cargo fmt --check` passes.
- [x] 8.2 `cargo clippy --all-targets --all-features` passes with no new warning and no `#[allow]`.
- [x] 8.3 `cargo test` passes.
- [x] 8.4 `npm run lint`, `npm run test` and `npm run build` pass in `ui/`.

