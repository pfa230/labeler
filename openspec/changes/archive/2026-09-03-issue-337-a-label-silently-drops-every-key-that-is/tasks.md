## 1. Server model: strict label envelope

- [x] 1.1 Add `#[serde(deny_unknown_fields)]` to `LabelInput` at `src/models.rs:1254` (struct `1255-1257`) so any key other than `data` on a label in `Vec<LabelInput>` paths is rejected as `json_malformed`
- [x] 1.2 Replace `RenderLabelRequest` at `src/models.rs:1220` (struct `1221`, fields `1222-1224`) from `#[serde(flatten)] pub label: LabelInput` to explicit `#[serde(deny_unknown_fields)] pub struct RenderLabelRequest { pub template: String, pub data: HashMap<String, Value> }` (wire shape `{ "template": "...", "data": {…} }` unchanged; fixes `unknown field` diagnostics per type and OpenAPI `allOf` break)
- [x] 1.3 Update `POST /api/render/label` handler at `src/api.rs:2591` / `src/api.rs:2673` to read `req.data` instead of `req.label.data` after the explicit struct; keep `Json<RenderLabelRequest>` extractor ordering (deserialization before handler) and `FormatUnknown` at `src/api.rs:2667` preceding `validate_label_data_keys` for data-inside-data keys
- [x] 1.4 Add a test asserting the generated OpenAPI `RenderLabelRequest` schema (registered at `src/openapi.rs:141`) carries no `allOf`, lists `template` and `data` among its required properties, and sets `additionalProperties: false`, alongside `openapi_print_request_is_strict` at `src/lib.rs:7808`

## 2. Error contract: withdraw `options_not_supported`

- [x] 2.1 Delete `Reason::OptionsNotSupported => "options_not_supported"` at `src/reason.rs:69` (and its match arm) so the withdrawn slug is absent from `Reason::ALL`; satisfies `spec_documents_every_reason_and_invents_none` at `src/errors.rs:665` / `scan_canonical_withdrawals` at `src/errors.rs:732-765` after `archive`
- [x] 2.2 Remove the `OptionsNotSupported` branch in `normalize_option` at `src/render/mod.rs:1224-1229` (`if option.is_some() { Err(invalid_request(OptionsNotSupported)) }`); branch is already unreachable at HEAD (`src/api.rs:2677,2681`, `src/batch.rs:105-106`, `src/api.rs:1254` all pass `None`) and CSV `option.<name>` remains live under `csv_option_column_unknown` (`docs/SPEC.md:758`)

## 3. UI: remove option surfaces

- [x] 3.1 Delete `PreviewInput.option`, `hasOpt` guard, both `option` spreads and the `option` branch in `previewKey` from `ui/src/lib/livePreview.ts` (lines `8,12-14,19,44,46` per review) so preview builds `label = { data: input.data }` and `body = { template, data }` / `labels: [{ data }]` only
- [x] 3.2 Remove `option?: Record<string, string>` from `TemplateInputsRequest.labels[]` at `ui/src/api/types.ts:101-102` (keep `{ data?: Record<string, unknown> }[]` only); verify no other UI file sends `option` on a label (`labelGrid.ts:17` / `connectorRows.ts:101` maps never reach the wire per `design.md:68`)

## 4. Tests: fix fixtures and pin rejections

- [x] 4.1 Update existing fixtures that send `{"option":{…},"data":{…}}` and expect `200` at `src/lib.rs:2022` (`batch_sheet_download_returns_pdf`), `2199` (`batch_sheet_print_failure_marks_all`), `2233` (`batch_sheet_print_success_one_job`) to send only `{"data":{…}}`; confirm `src/lib.rs:7740` (`DefaultBodyLimit` 2.1 MiB) stays green
- [x] 4.2 Add HTTP test pinning `option` envelope rejection on `POST /api/render/label` (`{ "template":"shelf","data":{…},"option":{…} }` → `400 InvalidRequest` / `json_malformed` / ``unknown field `option`, expected `template` or `data``` in `details.error`)
- [x] 4.3 Add HTTP test pinning `option` envelope rejection on `POST /api/batch` and `POST /api/templates/{id}/inputs` (label `{ "data":{…},"option":{…} }` → `400` / ``unknown field `option`, expected `data```)
- [x] 4.4 Add HTTP test pinning misspelled `dataa` envelope rejection on all three endpoints (`{ "template":"shelf","dataa":{…} }` on render/label → ``unknown field `dataa`, expected `template` or `data```; `LabelInput` paths → ``unknown field `dataa`, expected `data```); reuse the `MODIFIED` scenarios `An option key is ignored` (historical heading, now `400`), `A label carrying an unknown envelope key is refused`, `A label carrying a misspelled envelope key is refused`, `A batch label carrying an unknown envelope key is refused`, `A single label carrying an unknown envelope key is refused` and the `ADDED` scenario `The published schema for the single-label body is a strict object`

## 5. Verification

- [x] 5.1 Run `cargo fmt` (no diff)
- [x] 5.2 Run `cargo clippy --all-targets --all-features` (no warnings; do not add `#[allow(clippy::...)]`)
- [x] 5.3 Run `cargo test` and note expected pre-archive phantom failure `SPEC §10.1 documents reasons that do not exist: ["options_not_supported"]` (`scan_canonical_withdrawals` scans only `openspec/specs` at `src/errors.rs:732-765` vs additive `src/errors.rs:771-773`; specified at `openspec/specs/layout-sizing/spec.md:1088-1096`); after `openspec archive` the suite is green. Do not edit `src/errors.rs` on that red; `run-change.sh:534-573` archives before gating.
- [x] 5.4 Run `npm run lint` and `npm test` in `ui/` (when `ui/` is touched)
