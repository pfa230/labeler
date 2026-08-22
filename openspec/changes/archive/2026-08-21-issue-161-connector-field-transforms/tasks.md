## 1. Connector resource descriptors

- [x] 1.1 Add `ColumnDef { key, label, ty, tier }` and `ResourceDescriptor { id, columns, dynamic_text_prefix }` to `src/connector/mod.rs`, plus `Connectors::resources() -> &'static [ResourceDescriptor]`.
- [x] 1.2 Declare Homebox's `entities` and `locations` descriptors in `src/connector/homebox.rs`, with `dynamic_text_prefix: Some("custom:")` on `entities`.
- [x] 1.3 Rewrite `HomeboxConnector::schema` to build its static columns from the descriptors, keeping the upstream-discovered `custom:` columns appended as today.
- [x] 1.4 Test: the static columns of the `schema()` response equal the descriptor's columns for both resources, so validation and schema cannot drift.

## 2. The transform type and its pass

- [x] 2.1 Add the `regex` dependency to `Cargo.toml`.
- [x] 2.2 Add `FieldTransform { resource, source, pattern }` to `src/connector/mod.rs` (serde + `utoipa::ToSchema`) and register it in `src/openapi.rs`.
- [x] 2.3 Implement the compile step: compile each rule once per call with `RegexBuilder::size_limit(65536)`, keyed by resource.
- [x] 2.4 Implement the pass over a `&BTreeMap<String, String>`-shaped row: skip when the source key is absent, empty, or longer than 8192 bytes; require a match with **every** named group participating; write outputs into a separate map merged after the whole pass, so no rule can read another's output.
- [x] 2.5 Unit tests for the pass: the worked `BOX.123 | Motorcycle parts` split; no match leaves keys absent; a matched-but-unparticipating group yields nothing; a group capturing the empty string yields an empty value; an over-long source value is a non-match.

## 3. Validation

- [x] 3.1 Implement `validate_transforms(connector, &[FieldTransform]) -> Result<(), (usize, String)>` covering every save-time fault in the spec: pattern compiles within budget, at least one named group, known resource, source is a declared text column or under the resource's dynamic prefix, no collision with a declared key, no repeat within a rule or across rules on the same resource, no `datetime` / `datetime.*` / `vars.*` name, at most 32 rules, pattern at most 512 bytes.
- [x] 3.2 Add `Reason::ConnectionTransformInvalid => "connection_transform_invalid"` to `src/reason.rs` and whatever `errors.rs` completeness test requires.
- [x] 3.3 Unit tests, one per rejection cause, each asserting the reported rule index.

## 4. Storage

- [x] 4.1 Add the migration `ALTER TABLE connections ADD COLUMN transforms TEXT;` following the `public_url` step.
- [x] 4.2 Add `transforms: Vec<FieldTransform>` to `store::Connection`; `row_to_connection` reads `NULL` as an empty list.
- [x] 4.3 Extend `create_connection` and `update_connection` (`transforms` as a `store::UpdateField`, so omitted keeps and empty clears).
- [x] 4.4 Test: a row written before the migration reads as an empty list; create/update/read round-trips rule order.

## 5. API

- [x] 5.1 Add `transforms` to `ConnectionInput` (optional) and to `ConnectionView` (always present).
- [x] 5.2 Validate on `POST` against `body.connector`, and on `PUT` against the **stored** connection's connector; reject with `400 InvalidRequest` / `connection_transform_invalid` naming the rule index, before anything is written.
- [x] 5.3 Apply the pass in the three `Connectors` wrappers: append derived `FieldSpec`s (`ty: text`, `tier: derived`) in `schema`, derive cells in `browse`, and in `materialize` rewrite the field list down, run the connector, transform, then project `data` back to exactly the requested fields.
- [x] 5.4 Make a stored rule naming a resource the connector no longer offers inert in all three paths.
- [x] 5.5 HTTP tests with `wiremock`, following the existing connector tests: a derived field appears in `schema`; a browsed row carries derived cells; materialize for a derived field alone returns it without its source; a non-matching row omits the key rather than emptying it; a rejected save leaves the stored connection untouched.

## 6. UI

- [x] 6.1 Add `FieldTransform` and `transforms` to `ui/src/api/connectors.ts` (`Connection`, `ConnectionInput`).
- [x] 6.2 Add the add/remove rule list to `ui/src/pages/settings/ConnectionsSection.tsx`: `resource` as a select over the connector's resources, `source` and `pattern` as text inputs, no client-side regex validation.
- [x] 6.3 Render the server's `400` message against the rule its index names.
- [x] 6.4 Tests in `ConnectionsSection.test.tsx`: rules round-trip through save; a rejected save shows the message on the right rule.

## 7. Docs

- [x] 7.1 Write `docs/adr/0059-connection-scoped-field-transforms.md` (Accepted), recording the connection scope, the per-resource binding, absent-on-no-match, all-or-nothing capture, and save-time-only validation.
- [x] 7.2 Add its row to `docs/adr/README.md`.

## 8. Verify and land

- [x] 8.1 File the follow-up issue for previewing a rule against a live fetched row, and link it from the ADR's Consequences.
- [x] 8.2 Run `cargo fmt`, `cargo clippy --all-targets --all-features`, `cargo test`, and the UI test suite; fix root causes, never `#[allow]`.
- [x] 8.3 End-to-end check against a running server (`LABELER_CONFIG_DIR=./config-dev cargo run`): save the Homebox location-splitting rule, browse the resource with the derived columns enabled in the column picker, materialize a selection, and confirm the grid and the label row carry `location_id` and `location_name`.
- [x] 8.4 Adversarial review of the diff (a different agent than the one that wrote it), addressing every finding or rebutting it with file:line evidence; repeat until a pass surfaces no meaningful fix.
- [x] 8.5 `/opsx:archive` with every delta synced into `openspec/specs/`, then review the archive diff.
- [x] 8.6 One commit covering code, ADR, specs and the archived change, with `Fixes #161`; merge into `main`, push, and remove the worktree.
