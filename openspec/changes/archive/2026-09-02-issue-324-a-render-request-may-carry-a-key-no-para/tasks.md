## 1. The refusal vocabulary

- [x] 1.1 Add `DataKeyUnknown => "data_key_unknown"` and `CsvDataColumnUnknown => "csv_data_column_unknown"` to the `reasons!` macro in `src/reason.rs`, in the `InvalidRequest` group beside `CsvOptionColumnUnknown`. Both slugs are already documented in this change's `specs/request-data-keys/spec.md`, which is what `spec_documents_every_reason_and_invents_none` (`src/errors.rs:649`) reads for a reason added after `docs/SPEC.md` was frozen; do not edit `docs/SPEC.md`.

## 2. The shared check

- [x] 2.1 Add `unknown_param_names<'a>(template: &TemplateContent, names: impl Iterator<Item = &'a str>) -> Vec<String>` in `src/render/mod.rs`: return the names that match no key of `template.params`, sorted ascending by code point (`str`'s `Ord`), empty when there are none. It builds no `AppError`, names no `Reason` and decides no status.
- [x] 2.2 Add the label wrapper beside it: takes `&TemplateDefinition` and `&HashMap<String, JsonValue>`, returns `Result<(), AppError>`, and turns a non-empty result into `AppError::invalid_request(Reason::DataKeyUnknown, …)` whose message names every unrecognized key and `template.id` — for example `data keys 'alpha', 'zeta' are not declared parameters of template 'shelf'`. The id is what the message names, never the display `name:`.
- [x] 2.3 Unit-test both in `src/render/mod.rs`: a map whose every key is declared and an empty map both pass; a map with several unrecognized keys yields one error naming them in ascending order; the order is the same across repeated runs over `HashMap`s with different insertion orders; a declared parameter that no layout item reads is accepted.

## 3. Wire the four paths

- [x] 3.1 `POST /api/render/label` (`src/api.rs:2590`): call the wrapper after the `format`, `color_mode` and `resolution` query validation and before the render call, so a request with a bad `format` still reports `format_unknown`.
- [x] 3.2 `render_single_batch` (`src/batch.rs:93`): call the wrapper at the top of each loop iteration, before the render; on failure push a `BatchFailure` carrying that label's index, code and reason and `continue`, exactly as a render failure already does, so every label is still visited.
- [x] 3.3 Widen `render_sheet_pages` (`src/render/mod.rs:887`) from `&TemplateContent` to `&TemplateDefinition`, adjusting the body only where the compiler needs an explicit deref.
- [x] 3.4 `render_sheet_pages`' per-label loop (`src/render/mod.rs:951`): call the wrapper at the top of each iteration, before `resolve_parameters`, pushing a `BatchFailure` and `continue`ing on failure.
- [x] 3.5 Update the call sites that pass a bare `TemplateContent` to `render_sheet_pages`: the unit tests in `src/render/mod.rs` and `tests/acceptance_issue_263.rs:354`. `src/batch.rs:167` already holds a `&TemplateDefinition` and needs no change.
- [x] 3.6 `import_csv` (`src/api.rs:2714-2729`): after the existing `option.` column check and before the `Vec<LabelInput>` is built, run `unknown_param_names` over the file's non-`option.` column names and turn a non-empty result into `AppError::invalid_request(Reason::CsvDataColumnUnknown, …)` naming every unrecognized column in ascending order and the template id — `CSV columns 'alpha', 'zeta' are not declared parameters of template 'shelf'`. Report each column exactly once however many rows the file holds. Leave `parse_csv_rows` and the `option.` check unchanged.

## 4. HTTP tests: the render and print paths

Add to `mod tests` in `src/lib.rs`, using the existing `build_app()` / `oneshot` harness.

- [x] 4.1 `POST /api/render/label` with `{"template": "…", "data": {…}}` carrying one unrecognized key is `400`, `error.code` `InvalidRequest`, `error.details.reason` `data_key_unknown`, the message names the key and the template id, and no image is returned.
- [x] 4.2 A label carrying three unrecognized keys yields one failure whose message names all three in ascending order, not three failures.
- [x] 4.3 A label carrying a declared parameter that no active item reads renders successfully.
- [x] 4.4 `POST /api/batch` with three labels, of which 0 and 2 carry an unrecognized key, is `422 BatchInvalid` with entries for indices 0 and 2 carrying code `InvalidRequest` and reason `data_key_unknown`, and returns no ZIP.
- [x] 4.5 The same batch against a `sheet` template is `422 BatchInvalid` with the same entries and returns no PDF, and a sheet batch spanning two pages whose only failing label is on the second page returns no PDF at all.
- [x] 4.6 `POST /api/print` with `copies: 3` for a `data` map carrying an unrecognized key is `422 BatchInvalid` with three entries at indices 0, 1 and 2 carrying the same code and reason, and dispatches no print job; the same map with `copies` omitted yields exactly one entry, at index 0.
- [x] 4.7 A label carrying both an unrecognized key and an uncoercible value for a declared parameter reports `data_key_unknown`, and the same label without the unrecognized key still reports the failure it reports today.
- [x] 4.8 `POST /api/render/label?format=svg` with a label carrying an unrecognized key reports `format_unknown`, not `data_key_unknown`.
- [x] 4.9 A `POST /api/batch` exceeding the label cap while also carrying a label with an unrecognized key is `413 BatchTooLarge`.

## 5. HTTP tests: the CSV path

- [x] 5.1 `POST /api/import/csv` whose header carries an unrecognized data column is `400` with reason `csv_data_column_unknown`, the message names the column and the template id, and no ZIP or PDF is produced.
- [x] 5.2 The same file with `mode=print` and a valid printer is the same `400`, and no print job is dispatched.
- [x] 5.3 A file with two unrecognized columns and several rows reports one failure naming both in ascending order.
- [x] 5.4 Precedence, one test per step, each asserting the reason unchanged from today: a file with an unrecognized column and an unparsable row reports `csv_row_invalid`; one with an unrecognized column and no data rows reports `csv_empty`; one breaking both column rules reports `csv_option_column_unknown`.
- [x] 5.5 A file exceeding the label cap that also carries an unrecognized data column reports `csv_data_column_unknown`, because the file's columns are judged before any label exists to count.
- [x] 5.6 A file naming only declared parameters, alongside `option.` columns naming declared ones, still imports (covered by `issue_324_5_1_to_5_6_csv_import_tests` at `src/lib.rs:8585` and existing `import_csv_routes_option_columns` at `src/lib.rs:2333`).

## 6. The restated batch contract and the inputs boundary

- [x] 6.1 A `POST /api/batch` whose label 0 carries an unrecognized key and whose label 1 omits a required parameter an active item reads reports both entries, index 0 with `data_key_unknown` and index 1 with `MissingField`, and produces no ZIP.
- [x] 6.2 Confirm the scenarios `batch-validation` restates from the frozen contract are covered — record which test covers each:
  - Batch passes when every label passes: `batch_sheet_single_field_download_returns_pdf` (`src/lib.rs:1822`), `batch_print_summary_ok` (`src/lib.rs:1870`)
  - Entry omits `reason` when its code carries none: `batch_invalid_label_returns_422` (`src/lib.rs:1840`), `issue_324_6_1_batch_mixed_failures_reported` (`src/lib.rs:8732`)
  - Oversized batch is `413 BatchTooLarge`: `issue_324_4_9_batch_admission_cap_precedes_data_key_validation` (`src/lib.rs:8558`) (added because existing `batch_oversized_body_is_413` at `src/lib.rs:7107` tested `DefaultBodyLimit` rather than `max_labels`).
- [x] 6.3 `POST /api/templates/{id}/inputs` for a label carrying an unrecognized key is `200` and returns exactly the list the same label without that key returns; the same label posted to `POST /api/render/label` is `400 data_key_unknown`.
- [x] 6.4 A label failing two ways (both carrying an unrecognized key and omitting a required parameter) contributes one entry with `data_key_unknown`, covered by `issue_324_6_4_batch_label_with_unrecognized_key_and_missing_required_param_reports_data_key_unknown` (`src/lib.rs:8822`).

## 7. Existing coverage and the gates

- [x] 7.1 Audit the existing tests and fixtures for requests that send a `data` key the target template does not declare, and fix each by declaring the parameter or dropping the key. No template under `catalog/` or `tests/fixtures/templates/` needs a content change for its own sake.
- [x] 7.2 Confirm every test added in groups 4, 5 and 6 asserting new refusal behaviour fails against pre-change code and passes after; non-refusal guard tests (4.3 asserting declared unused param renders ok, 4.8 asserting query format validation precedence, 4.9 asserting batch admission cap precedence, and 6.3 inputs endpoint leniency) assert preserved behavior.
- [x] 7.3 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test`, and fix what they report at the root rather than with `#[allow(clippy::…)]`.
