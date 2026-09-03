## 1. Server: error contract and CSV prefix

- [x] 1.1 Delete `Reason::CsvOptionColumnUnknown` variant at `src/reason.rs:94` and ensure `Reason::ALL` and `spec_documents_every_reason_and_invents_none` (`src/errors.rs:732,785,797-805`) rely on the canonical withdrawal in `request-data-keys`
- [x] 1.2 Remove `ParsedCsvRow::option` field and the `option.` prefix handling in `src/api.rs` (`2260-2261` header split, `2727-2730` `csv_option_column_unknown` refusal, `2767` fold) so every header flows as `String(val)` into `data`
- [x] 1.3 Verify the `ADDED` withdrawal requirement in `specs/request-data-keys/spec.md` supersedes the frozen `docs/SPEC.md:758` row `| InvalidRequest | csv_option_column_unknown | An option.* CSV column names an option the template does not declare. |` and contains the `The csv_option_column_unknown slug SHALL be withdrawn, and SHALL NOT be raised by any code path:` table in the shape `scan_canonical_withdrawals` scans

## 2. Browser: Import prefix handling

- [x] 2.1 Remove `OPTION_PREFIX` and the `option.` split/mapping in `ui/src/lib/csv.ts` (`6`, `10`, `14-15`, `22`, `24`, `55-56`, `61`, `74`, `79`, `81`, `84` — `OPTION_PREFIX`, `optionColumns`, `CsvRow.option`)
- [x] 2.2 Update `ui/src/pages/Import.tsx` to delete the option merge at `236-237` (`data: { ...r.option, ...r.data }` / `option: { ...r.option }`) and the dead `validation.option` read at `180`, keeping `LabelGridRow.option` required with `option: {}` at `237` per design
- [x] 2.3 Keep `ui/src/lib/labelGrid.ts:17` `LabelGridRow.option` required and `ui/src/components/LabelGrid.tsx:208-216` reading `row.option` unchanged (alternative of making `option` optional is owned by #214)

## 3. Tests

- [x] 3.1 Update `src/lib.rs` CSV import tests at `2568-2612` (three cases including `csv_option_column_unknown` at `2589-2591` and `option.orientation` routing at `2570`), `11378-11391` (both-column-rules case) and `11413-11421` (`option.orientation` alongside declared ones, now `400` `csv_data_column_unknown`) to assert `csv_data_column_unknown` for `option.size` headers and remove `csv_option_column_unknown` expectations
- [x] 3.2 Update `ui/src/lib/csv.test.ts:5-11,62-63` to remove `optionColumns`/`option` map assertions and use plain `sku,color` fixtures
- [x] 3.3 Update `ui/src/pages/Import.test.tsx:92,124,135,287` fixtures from `sku,option.color` to plain `sku,color` and remove `option` handling expectations

## 4. Verification

- [x] 4.1 Run `cargo fmt`
- [x] 4.2 Run `cargo clippy --all-targets --all-features`
- [x] 4.3 Run `cargo test` (pre-archive phantom `SPEC §10.1 documents reasons that do not exist: ["csv_option_column_unknown"]` is expected per `layout-sizing:1088-1096`; post-`openspec archive` sync it must pass, and reintroducing the slug must fail)
- [x] 4.4 Run `openspec validate --strict` and `npm run lint` / `npm test` in `ui/` (if `ui/` was touched)
