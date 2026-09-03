## Why

Fixes #336. `POST /import/csv` and the Import page treat `option.<name>` as a second spelling for a data column. No template can declare an option, so the prefix names nothing and duplicates the ordinary data-column path. Under the pre-1.0 rule the duplicate spelling is removed, not kept.

## What Changes

- **BREAKING** `POST /import/csv` no longer splits `option.<name>` headers. `option.size` is an ordinary header and, as an unrecognized column, fails with `csv_data_column_unknown`. `csv_option_column_unknown` leaves the error contract and step 4 of the file's refusal order is gone.
- **BREAKING** `Reason::CsvOptionColumnUnknown` (`src/reason.rs:94`) is deleted and canonically withdrawn. The frozen `docs/SPEC.md:758` row is superseded by a first-touch `ADDED` requirement carrying the complete post-change contract; `cargo test` passes with the variant deleted and would fail if it were reintroduced.
- **BREAKING** The `option.` prefix is gone from `src/api.rs` (`ParsedCsvRow::option` at `2260-2261`, fold at `2767`, refusal at `2727-2730`), `ui/src/lib/csv.ts` (`OPTION_PREFIX` at `6`, split at `55-56,61`), and `ui/src/pages/Import.tsx` (`236-237` merge and the dead `validation.option` read at `180`).
- The file's refusal order is otherwise unchanged: `csv_header_invalid`, `csv_row_invalid`, `csv_empty`, then `csv_data_column_unknown`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `request-data-keys`: requirement `A CSV data column names a declared parameter` (spec.md:162, 170, 174, 226) is replaced via `REMOVED` + `ADDED` with distinct title (`…: every header is a data column`) to drop the `option.<name>` sentence, the option step, the both-column-rules sentence and the folded-columns sentence, contracting the 5-step order to 4 and renaming `An option column is judged by its own rule` to `A column option.size naming no declared parameter fails as csv_data_column_unknown`. An `ADDED` requirement withdraws `csv_option_column_unknown` and supersedes the frozen `docs/SPEC.md:758` row with its complete post-change contract.
- `param-resolution`: requirement `A parameter is required unless the template declares a default` is replaced via `REMOVED` + `ADDED` with distinct title (`…: CSV data cells are plain values`) to replace the `Two things` paragraph's `option.<name>` column with a plain data-column description and to rename the two `CSV option cell` scenarios to `CSV data cell` headings, correcting `resolved per the requirement below` directional phrases.
- `batch-validation`: the `Every label is judged before any label is executed` requirement's admission sentence `an unrecognized column of either kind` is narrowed to `an unrecognized data column` to remove the retired vocabulary.
- `template-inputs`: the `The options_not_supported reason is withdrawn` requirement's table cell that still asserts `option.<name>` columns are judged under `csv_option_column_unknown` is updated to state every column is now a data column judged under `csv_data_column_unknown` and `csv_option_column_unknown` is withdrawn.

## Impact

- `src/reason.rs:94` variant deleted; `Reason::ALL` and `spec_documents_every_reason_and_invents_none` (`src/errors.rs:732,785,797-805`) rely on the canonical withdrawal.
- `src/api.rs:2260-2261,2727-2730,2767` prefix split, refusal, and fold deleted; all headers flow as `String(val)` data columns.
- `ui/src/lib/csv.ts:6,10,14-15,22,24,55-56,61,74,79,81,84` (`OPTION_PREFIX`, `optionColumns`, `CsvRow.option`, split and mapping) and `ui/src/pages/Import.tsx:180,236-237` prefix handling deleted; `ui/src/lib/labelGrid.ts:17` `LabelGridRow.option` stays required, so the literal keeps `option: {}` at `Import.tsx:237` (choice recorded in design) and `ui/src/components/LabelGrid.tsx:208-216` continues to read `row.option`.
- Tests that break and are updated here: `src/lib.rs:2568-2612` (three CSV import cases including `csv_option_column_unknown` at `2589-2591` and `option.orientation` routing at `2570`), `:11378-11391` (both-column-rules case) and `:11413-11421` (`option.orientation` alongside declared ones, now `400`), and surrounding CSV precedence cases, `src/reason.rs` / `src/errors.rs` phantom/withdrawal tests, `ui/src/lib/csv.test.ts:5-11,62-63` and `ui/src/pages/Import.test.tsx:92,124,135,287` (`sku,option.color` fixtures). No new endpoint, code, or YAML field; stored CSVs using the prefix are deliberately broken and must rename the header.
