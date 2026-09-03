## 1. The value model in `src/connector/mod.rs`

- [x] 1.1 Add `multi_valued: bool` to `FieldSpec` and to `ColumnDef`, serialized unconditionally (no
  `skip_serializing_if`), so every `FieldSpec` on the wire carries the key including `false`.
- [x] 1.2 Set `multi_valued: false` at both existing `FieldSpec` construction sites: `homebox.rs`'s
  `field()` helper (`:454-461`) and the derived-column push in `Connectors::schema` (`:486-491`).
- [x] 1.3 Add a `List(Vec<String>)` variant to `CellValue`, keeping the enum `#[serde(untagged)]` so a
  `Text` cell still serializes as a bare JSON string and a `Number` cell as a bare JSON number.
- [x] 1.4 Add an untagged, serialize-only `RowValue` enum of `Text(String) | List(Vec<String>)` and
  change `LabelRow.data` to `BTreeMap<String, RowValue>`. Keep it a separate type from `CellValue`:
  `data` must have no way to hold a JSON number (design.md, "Two value types, not one").
- [x] 1.5 Change `apply_to_map` (`:127-141`) to read a `RowValue` map, matching only `RowValue::Text`
  as a rule's source and inserting derived values as `RowValue::Text`, mirroring how `apply_to_cells`
  (`:143-146`) already matches only `CellValue::Text`.
- [x] 1.6 Add the multi-valued `source` refusal to `validate_transforms`: a `source` naming a column
  whose `multi_valued` is `true` is rejected with a message naming the source, alongside the existing
  unknown-resource and not-text refusals, and returned as `400 InvalidRequest` with `details.reason`
  `connection_transform_invalid` by the existing save path.

## 2. The Homebox `tags` column

- [x] 2.1 Add a `tags` entry to `ENTITIES_COLUMNS`: key `tags`, label `Tags`, `ty` `FieldType::Text`,
  `tier` `Tier::Cheap`, `multi_valued: true`. Leave `LOCATIONS_COLUMNS` unchanged.
- [x] 2.2 Add the upstream `tags` field to `EntitySummary` (`:428-452`) as an optional array of tag
  objects, reading each element's `name` only and ignoring id, colour, icon and description.
- [x] 2.3 In `summary_to_row` (`:463`), insert a `CellValue::List` of the tag names, in the upstream's
  order with no sorting, deduplication or trimming, on the `entities` branch only; insert
  `CellValue::List(vec![])` when the item carries no tags, never an absent key.
- [x] 2.4 Change `extract_field` (`:541`) to return `RowValue`: a `"tags"` key yields a
  `RowValue::List` of the detail's tag names (`[]` when there are none), and every other key yields
  the `RowValue::Text` it produces today, the stringified upstream number and the empty-string
  catch-all included.
- [x] 2.5 Leave the outbound `tag` browse filter (`EffectiveHomeboxFilters`, `:334-336`) untouched,
  and add no column for `attachments`, `children` or any other upstream array.

## 3. OpenAPI

- [x] 3.1 Register `RowValue` in `src/openapi.rs` components and confirm `FieldSpec`, `CellValue` and
  `LabelRow` re-emit with the new shapes.

## 4. Server tests

- [x] 4.1 HTTP test: `GET /api/connections/{id}/schema` reports a `tags` column on `entities` with
  `ty` `text`, `tier` `cheap` and `multi_valued` `true`, and no `tags` column on `locations`.
- [x] 4.2 HTTP test: every other `FieldSpec` in that response carries `multi_valued` `false` as a
  present key, and a connection carrying a field transform reports its derived column with `tier`
  `derived` and `multi_valued` `false`.
- [x] 4.3 HTTP test: `POST /api/connections/{id}/browse` over an item tagged `KIDS` then `CONSUMABLE`
  returns that row's `tags` cell as `["KIDS","CONSUMABLE"]` in that order, while its other cells are
  the strings and numbers they are today.
- [x] 4.4 Test that browsing a page makes the same number of upstream requests as before the column
  existed, asserted against the mock server's received-request count rather than inferred.
- [x] 4.5 HTTP test: `POST /api/connections/{id}/materialize` for `fields: ["name","quantity","tags"]`
  returns `data["tags"]` as a JSON array of the tag names, and `data["name"]` and `data["quantity"]`
  as the exact JSON strings they are today, asserting `quantity` is a string and not a JSON number.
- [x] 4.6 HTTP test: an item with no tags browses and materializes `tags` as `[]`, asserting it is
  neither `""`, nor `null`, nor an absent key.
- [x] 4.7 Test that an array-valued upstream key no column declares keeps today's answer: the empty
  string on materialize, and no cell on browse.
- [x] 4.8 HTTP test: saving a connection with a transform whose `source` is `tags` is refused with
  `400`, `details.reason` `connection_transform_invalid` and a message naming `tags`, and the
  connection is left unstored. Assert the refusal fires before it: confirm the same body with a
  scalar `source` saves.

## 5. UI types and the one display text

- [x] 5.1 In `ui/src/api/connectors.ts`: add `multi_valued: boolean` to `FieldSpec`, widen `CellValue`
  to `string | number | string[]`, and widen `LabelRowResult.data` to `Record<string, string |
  string[]>`.
- [x] 5.2 Add one exported `displayCellText` helper in a shared module under `ui/src/lib/`: `""` for
  an absent cell, `String(value)` for a string or number, and the elements joined with `", "` in order
  for an array, so an empty list yields `""`. This is the single definition the spec requires.
- [x] 5.3 Use it in `NameCell` (`ui/src/pages/connect/ConnectorBrowser.tsx:52,60`), so a multi-valued
  cell renders `KIDS, CONSUMABLE` rather than concatenated elements.
- [x] 5.4 Use it in `displayedCell` (`ui/src/lib/connectorFilter.ts:9-12`), replacing the bare
  `String(value)`.
- [x] 5.5 Use it in `connectorSort.ts`'s key extractors (`:9-13` and the number and date extractors),
  so a multi-valued cell is interpreted as the column's declared display type applied to its display
  text: a `text` or `badge` column orders by that text case-insensitively, an empty list and a
  multi-valued cell in a `number`, `money` or `date` column are uninterpretable and order with the
  blanks, and `textKey` no longer calls `.toLowerCase()` on an array.

## 6. Field mapping and the row grid

- [x] 6.1 In `ui/src/lib/connectorRows.ts`, make `defaultMapping` (`:8-13`) match on cardinality as
  well as key: a parameter is pre-filled to a same-named column only when the column's `multi_valued`
  matches whether the parameter is declared `list`, and is left unmapped otherwise.
- [x] 6.2 Add a mapping-validation function there that reports each `(parameter, column)` pair whose
  cardinality does not match, in both directions, with a message naming both the column and the
  parameter.
- [x] 6.3 Change `rowsFromMaterialized` (`:22-41`) to build `data` as `Record<string, ParamValue>`,
  copying an array value through unchanged; an unmapped field keeps the `""` it gets today.
- [x] 6.4 In `ui/src/pages/Connect.tsx`, stop filtering `list` inputs out of `templateFields` (`:125`)
  and out of the displayed-column union (`:152`, `:158`), so a `list` parameter is mappable and its
  column is shown. Leave the `validateRow` skip (`:177`) as it is: validating a list cell is not this
  change's.
- [x] 6.5 In the same file, surface the mapping refusal from 6.2 and block "Add rows" while any pair
  mismatches, so no rows are added for a mapping the batch would reject.
- [x] 6.6 In `ui/src/components/LabelGrid.tsx`, render a `list`-control cell read-only as
  `displayCellText` **when the cell holds an array** (`:155-157`), and keep today's em-dash otherwise.
  Leave the non-editable rules at `:151` and `:196` unchanged, so no list cell becomes editable and
  the CSV, manual and batch rows, which never hold an array, keep the em-dash.

## 7. UI tests

- [x] 7.1 `connectorSort.test.ts`: a multi-valued `text` column holding `["KIDS","CONSUMABLE"]`,
  `["ATTIC"]` and `[]` orders `ATTIC`, `KIDS, CONSUMABLE`, then the empty list ascending, and
  `KIDS, CONSUMABLE`, `ATTIC`, then the empty list descending; sorting a multi-valued cell in a
  `number` or `date` column places it with the blanks; neither direction throws.
- [x] 7.2 `connectorFilter.test.ts`: the needle `kids, cons` matches a row whose cell is
  `["KIDS","CONSUMABLE"]`, case-insensitively against the displayed text.
- [x] 7.3 `connectorRows.test.ts`: the pre-fill leaves a `string` parameter named `tags` unmapped when
  the same-named column is multi-valued, and does pre-fill a `list` parameter named `tags` to it; the
  validator reports both mismatch directions with a message naming column and parameter, and reports
  nothing for a matching pair or an unmapped parameter.
- [x] 7.4 `Connect.test.tsx`: a `list` template parameter appears in the field mapping; mapping the
  multi-valued `tags` column to it and adding rows sends a batch whose label carries
  `data: {"tags": ["KIDS","CONSUMABLE"]}` as a JSON array; an untagged item sends `{"tags": []}`.
- [x] 7.5 `Connect.test.tsx`: mapping the multi-valued `tags` column to a `string` parameter, and
  mapping a scalar column to a `list` parameter, each show a refusal naming both and add no rows.
- [x] 7.6 `LabelGrid.test.tsx`: a `list`-control cell holding `["KIDS","CONSUMABLE"]` renders
  `KIDS, CONSUMABLE` and is not editable, while a `list`-control cell holding no array still renders
  the em-dash.

## 8. Gates

- [x] 8.1 Run `cargo fmt` and `cargo clippy --all-targets --all-features`, fixing root causes rather
  than adding `#[allow(...)]`.
- [x] 8.2 Run `cargo test`.
- [x] 8.3 Run `npm run lint`, `npm run test` and `npm run build` in `ui/`, which is what CI runs for
  the frontend.
