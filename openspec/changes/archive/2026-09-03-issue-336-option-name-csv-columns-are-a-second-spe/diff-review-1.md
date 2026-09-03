TREE_SHA256: e806f70f89c6a39cd12ea22524d42c68295fa425c654c509e3e952fa180a2987
SPECS_SHA256: 438d6cf42070910dc83f6e6968ba4c1ffac128313e3e9299a3587c0ff4e0105d

**Findings — issue-336-option-name-csv-columns-are-a-second-spe**

Verified against `openspec/changes/issue-336-option-name-csv-columns-are-a-second-spe/proposal.md`, `design.md`, `tasks.md`, `specs/*`, published `openspec/specs/*`, `src/api.rs`, `src/reason.rs`, `src/lib.rs`, `ui/src/lib/csv.ts`, `ui/src/pages/Import.tsx` and `AGENTS.md`.

**Spec delta mechanism — correct per `ANSWERS.md`**

`specs/request-data-keys/spec.md:1` and `specs/param-resolution/spec.md:1` correctly use `REMOVED` + `ADDED` with distinct titles (`…: every header is a data column`, `…: CSV data cells are plain values`) to avoid `validate --strict` refusing a `MODIFIED` with renamed scenario headings. Verified via `openspec validate --changes --strict` [verified: passes] and `diff` against published specs shows only the retired-category sentences and two directional phrases (`resolved per the requirement below` → `resolved per the declared-default requirement` at `specs/param-resolution/spec.md:17`, `preview requirement below` → `preview requirement` at `:54`) changed, with unchanged paragraphs byte-identical to `openspec/specs/param-resolution/spec.md:18-21` [verified].

**Withdrawal — correct shape**

`specs/request-data-keys/spec.md:93-101` `ADDED` requirement `The csv_option_column_unknown reason is withdrawn` supersedes frozen `docs/SPEC.md:758`, contains `The csv_option_column_unknown slug SHALL be withdrawn, and SHALL NOT be raised by any code path:` and a `| Reason | Why ... |` table with `` `csv_option_column_unknown` `` in the first cell. This matches `src/errors.rs:744-762` `scan_canonical_withdrawals` (`line.to_lowercase().contains("withdrawn")`, `line.startsWith('|')`, `split('|').nth(1)`). The sibling `csv_data_column_unknown` table is not contaminated because its `### Requirement:` heading resets `in_withdrawn_section` [verified].

**Server — correct deletion**

- `src/reason.rs:92-94` `CsvOptionColumnUnknown` variant deleted; `Reason::ALL` at `:19` structurally complete [verified].
- `src/api.rs:2222-2224` `ParsedCsvRow::option` field deleted; `parse_csv_rows` at `:2256-2260` now `data.insert(key.toString(), String(val))` for every header, no `strip_prefix("option.")` [verified].
- `src/api.rs:2718-2742` refusal loop at former `2727-2730` and fold at former `2767` (`if !v.is_empty() { row.data.insert(name, ...) }`) deleted; unknown columns now flow through `unknown_param_names` (`src/render/mod.rs:173` `sort`) → `csv_data_column_unknown` [verified].
- `ui/src/lib/csv.ts:6-8,55` `OPTION_PREFIX`, `optionColumns`, `CsvRow.option`, and `kind` routing deleted; `fields` now `columns.filter(c is string)` and `rows.push({data})` [verified].
- `ui/src/pages/Import.tsx:178-180` `rowInvalid` now `!!v.field` only (dead `validation.option` removed); `:233-237` `data:{...r.data}, option:{}` keeps `LabelGridRow.option` required per `design.md:19` and `ui/src/lib/labelGrid.ts:17` [verified]. `ui/src/components/LabelGrid.tsx:208-216` still reads `row.option` — scoped to #214, accepted [verified].

**Tests — correctly re-pointed**

- `src/lib.rs:2566-2584` `import_csv_routes_option_columns` now `orientation,outline` without prefix; `src/lib.rs:2587-2611` `import_csv_undeclared_option_column_returns_400` now asserts `400 InvalidRequest csv_data_column_unknown` for `option.bogus`; `:2614-2638` `import_csv_disallowed_option_value_is_atomic` now `orientation` [verified].
- `src/lib.rs:11384-11400` scenario `c` now expects `csv_data_column_unknown` with `msg.contains("'bad_data', 'option.bad_opt'")` — ascending order matches `unknown_param_names:173` `sort` (`'b' < 'o'`) [verified].
- `src/lib.rs:11421-11437` new scenario 5.6 `option.orientation,option.outline` → `400 csv_data_column_unknown` and 5.7 valid `orientation,outline` → `200` [verified].
- `ui/src/lib/csv.test.ts:5-11,62-63` now expects `fields: ["sku","name","color"]` and `rows: [{data:...}]` without `option`/`optionColumns` [verified]; `ui/src/pages/Import.test.tsx:89,121,132,285` fixtures `sku,option.color` → `sku,color` [verified].
- `cargo fmt --check` and `cargo clippy --all-targets --all-features` pass [verified]; `openspec validate --changes --strict` and `openspec validate --specs --strict` (27/27) pass [verified].

**Pre-archive phantom — not a defect**

`cargo test` currently fails `errors::tests::spec_documents_every_reason_and_invents_none` with `SPEC §10.1 documents reasons that do not exist: ["csv_option_column_unknown"]` at `src/errors.rs:802` [verified]. This is the documented pre-archive phantom (`design.md:24`, `layout-sizing:1088-1096`): `scan_canonical_withdrawals` reads `openspec/specs` only, the withdrawal lives in `openspec/changes/.../specs` until `openspec archive` syncs it. Post-archive the test must pass; reintroducing the variant must fail — matches `tasks.md:4.3` [verified].

**Cross-reference fixes — applied**

`specs/batch-validation/spec.md:58` `an unrecognized column of either kind` → `an unrecognized data column`; `specs/template-inputs/spec.md:11` `csv_option_column_unknown` sentence → `Every CSV column ... is now a data column judged under csv_data_column_unknown, and csv_option_column_unknown is itself withdrawn` [verified]. `design.md:17` now states `ADDED blocks reproduce the published line breaks verbatim for unchanged text and wrap changed text to the file's ~100-column width` per `review.md:35` [verified]. `proposal.md:30` Impact now cites `ui/src/lib/csv.test.ts:5-11,62-63` per `review.md:43` [verified]. `specs/param-resolution/spec.md:126-127` blank-cell enum clause now reads `contributing a details.failures entry whose code is InvalidEnumValue under 422 BatchInvalid` per `review.md:39` [verified].

No blocking defects found. The diff implements the proposal's breaking removal exactly, the deltas satisfy `ANSWERS.md` (no published requirement specifies an option column/cell category) and `openspec validate --strict`, and the code/tests match the spec.

VERDICT: APPROVE
