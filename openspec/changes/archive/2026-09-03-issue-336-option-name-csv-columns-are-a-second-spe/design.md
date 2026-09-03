## Context

See proposal.md - Why. `POST /import/csv` currently splits `option.<name>` (`src/api.rs:2260-2261`), folds non-empty cells into `data` (`2767`), and refuses unknown option columns as `csv_option_column_unknown` (`2727-2730`, `src/reason.rs:94`). The browser mirrors it (`ui/src/lib/csv.ts:6,55-56,61`, `ui/src/pages/Import.tsx:236-237`). Frozen `docs/SPEC.md:758` publishes the `csv_option_column_unknown` row and `request-data-keys:170` defines step 4 as that refusal. `#337` landed the withdrawal pattern for `options_not_supported` at `template-inputs:1658-1674`.

## Goals / Non-Goals

**Goals:** delete the duplicate spelling in both server and browser so `option.size` is judged as an ordinary `csv_data_column_unknown`, and make the withdrawal pass both `cargo test` and `openspec validate --strict` without reflowing untouched text.

**Non-Goals:** touch anything else about CSV (`csv_header_invalid`, `csv_row_invalid`, `csv_empty`), `data_key_unknown`, or `template-inputs` rendering. The prior `template-inputs` widening is explicitly out of scope and is removed.

## Decisions

**Delete the prefix, don't alias.** Every header is now a data column. An unknown `option.size` reaches `unknown_param_names` as `option.size` and is refused as `csv_data_column_unknown`.

**Withdraw to delete.** `Reason::CsvOptionColumnUnknown` is deleted and the branches at `src/api.rs:2727-2730` are deleted, but `docs/SPEC.md:758` is frozen. Follow `#337`: add `ADDED` requirement `The csv_option_column_unknown reason is withdrawn` in `request-data-keys` that supersedes the frozen row, states the complete post-change contract, and contains the `The csv_option_column_unknown slug SHALL be withdrawn, and SHALL NOT be raised by any code path:` table in the shape `scan_canonical_withdrawals` scans (`src/errors.rs:732,785,797-805`). This makes the phantom half subtract the slug and makes keeping the variant a failure.

**REMOVED+ADDED with distinct titles, matching wrapping.** No published requirement may specify an “option column” or “option cell” as a category. The two `CSV option cell` headings and `An option column is judged by its own rule` are therefore renamed to `CSV data cell` and to `A column option.size naming no declared parameter fails as csv_data_column_unknown` (example, not category). `MODIFIED` with renamed headings fails `validate --strict` because the old scenario is then reported as omitted, so the deltas use `REMOVED` for the old requirement and `ADDED` with distinct titles (`…: every header is a data column`, `…: CSV data cells are plain values`): the shape that passes `validate --strict` and archives (`+1 added, -1 removed`). Every `MODIFIED` block copies its published block and changes only the retired-category sentences, preserving column width; `ADDED` blocks reproduce the published line breaks verbatim for unchanged text and wrap changed text to the file's ~100-column width, and the `batch-validation` `MODIFIED` changes only `of either kind` → `data`. Relocating the `param-resolution` requirement leaves `resolved per the requirement below` (spec.md:17) and `preview requirement below` (spec.md:55) dangling, so both are rewritten to direction-agnostic forms inside the moved requirement; the `under the requirement below` at spec.md:153 sits in a different requirement that does not move, so it stays correct. Two additional narrow deltas fix dangling cross-references: `batch-validation` narrows `of either kind` and `template-inputs` updates the `csv_option_column_unknown` sentence to `csv_data_column_unknown`.

**Keep `LabelGridRow.option` required.** `LabelGridRow.option` (`ui/src/lib/labelGrid.ts:17`) stays required and `ui/src/pages/Import.tsx:237` keeps `option: {}`; `ui/src/components/LabelGrid.tsx:208-216` continues to read `row.option` and the dead `validation.option` read at `ui/src/pages/Import.tsx:180` is removed. The alternative of making `LabelGridRow.option` and `LabelGridRow.validation.option` optional would touch `labelGrid.ts` and `LabelGrid.tsx` and is owned by #214, so this change keeps the field required.

## Risks / Trade-offs

- **Stored CSVs break deliberately** → A file with `option.size` now `400 csv_data_column_unknown`; rename the header.
- **Pre-archive phantom** → Between code deletion and `archive` sync, `cargo test` is red on the phantom half; this is expected per `layout-sizing:1088-1096` because `scan_canonical_withdrawals` scans `openspec/specs` only.
- **Empty-cell semantics** → The deleted fold skipped empty `option.<name>` cells (omission). A plain data column with an empty cell is now `""` (`src/api.rs:2260-2261` `String(val)`), so `param-resolution` now states `"<name>": ""` rather than omission.

## Migration Plan

Single commit: delete the variant and prefix branches, update the three test files named in Impact (`src/lib.rs`, `ui/src/lib/csv.test.ts`, `ui/src/pages/Import.test.tsx`), run `openspec archive` to sync the `REMOVED`/`ADDED` replacements and the withdrawal, then `cargo test` and `openspec validate --strict` both pass.

## Open Questions

None.
