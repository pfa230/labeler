# Plan review — issue-336

Read: `ANSWERS.md`, `proposal.md`, `design.md`, all four deltas, `review-1/2/3.md`, the four published specs, `docs/SPEC.md:755-760,1063-1096,1394`, `src/errors.rs:660-806`, `src/reason.rs:94`, `src/api.rs:2255-2290,2725-2775`, `src/lib.rs` CSV cases, `ui/src/lib/csv.ts`, `ui/src/lib/labelGrid.ts`, `ui/src/pages/Import.tsx:170-250`, `ui/src/components/LabelGrid.tsx:195-230`, `openspec/config.yaml`. Ran `openspec validate <change> --strict` (passes) and diffed both `MODIFIED` blocks against their published requirements.

**State note.** Every delta file is unmodified since `16:15`; `review-3.md` was written at `16:23` and returned `REVISE`. The artifacts I judged are byte-identical to the ones it judged, so findings 1 and 2 below are the same two defects, independently re-verified rather than taken on its word.

## Blocking

### 1. The withdrawal requirement the whole plan rests on is in no delta; `cargo test` will fail

`proposal.md:8,20` and `design.md:15` state that an `ADDED` requirement withdraws `csv_option_column_unknown` and supersedes the frozen `docs/SPEC.md:758` row. No such requirement exists. `grep -rn -i withdraw specs/` returns only the two `REMOVED` **Reason** lines, the `options_not_supported` requirement carried in the `template-inputs` delta, and two scenario headings. [verified]

The consequence is mechanical. `scan_canonical_withdrawals` (`src/errors.rs:732-766`) takes a slug only from the **first** cell of a table row (`line.split('|').nth(1)`) under a line containing "withdrawn". The deltas' only mention of `csv_option_column_unknown` in such a table is the *second* cell of the `options_not_supported` row (`specs/template-inputs/spec.md:14`), which the scanner never reads. So `canonical_withdrawn_refs` will lack the slug, `docs/SPEC.md:758` still lists it in the §10.1 table `spec_table` is built from (`src/errors.rs:669-685`), and with `Reason::CsvOptionColumnUnknown` deleted the phantom assert at `:797-806` fires: `SPEC §10.1 documents reasons that do not exist: ["csv_option_column_unknown"]`. [verified]

That defeats three things at once:

- the issue's acceptance criterion "A published requirement withdraws `csv_option_column_unknown`, so `cargo test` passes with the variant deleted and would fail if it were reintroduced";
- the first-touch rule (`AGENTS.md`, `openspec/config.yaml` `rules.specs`): the `ADDED` block's supersession paragraph (`specs/request-data-keys/spec.md:44-46`) is the published home of `csv_data_column_unknown` and names nothing about the `:758` row, so that row is superseded by nothing;
- `specs/request-data-keys/spec.md:94-98`, which publishes "**AND** the registry test fails if it is reintroduced without a spec change" inside a requirement whose normative text never withdraws the slug. The reintroduction assert (`src/errors.rs:789-795`) keys off `canonical_withdrawn_refs`, so that clause is false as written. `specs/template-inputs/spec.md:14` likewise publishes a cross-reference to a requirement that does not exist.

`review.md:15` (round 2) reports "The withdrawal table is in the shape `scan_canonical_withdrawals` scans", and its required-change 4 cites `specs/request-data-keys/spec.md:91-93` as the quoted `:758` row. The requirement was present then and was lost while the round-2 edits were applied.

### 2. The `ADDED` requirement publishes a scenario the change makes false

`specs/request-data-keys/spec.md:89-92` is copied verbatim from published `openspec/specs/request-data-keys/spec.md:228-231`:

> **WHEN** the header names only declared parameters, alongside `option.` columns naming declared ones — **THEN** the file imports exactly as it does today

After this change `option.orientation` is an ordinary header naming no declared parameter, so step 4 of the same requirement (`specs/request-data-keys/spec.md:31`) refuses it `csv_data_column_unknown`. The scenario contradicts its own requirement and contradicts the sibling scenario at `:82-87`. It is also the last place the block asserts `option.` columns as a working category, which `ANSWERS.md` forbids. [verified]

Its code twin is `src/lib.rs:11413-11414`, which posts `id,url,name,tags,description,option.orientation,option.outline` and asserts `OK`; that test must invert to `400`/`csv_data_column_unknown`, and it is not among the ranges `proposal.md:30` names (`2568-2612`, `11378-11391`). [verified]

## Non-blocking, but real

3. **`template-inputs`'s `MODIFIED` block reflows two paragraphs the change does not touch.** Diffed against published: the supersession paragraph (`openspec/specs/template-inputs/spec.md:1660`) and the ADR-0052 paragraph (`:1668`) are single unwrapped lines published and are rewrapped to three and three lines in the delta (`specs/template-inputs/spec.md:5-8,16-18`). The only intended edit is the table cell's last sentence. Archive lands the block verbatim, so this permanently rewraps untouched normative text (trap 2 in the issue) and falsifies `design.md:17`'s claim that every `MODIFIED` block "changes only the retired-category sentences, preserving column width". `batch-validation` is clean: its only diff is `of either kind` → `data`, plus a trailing blank line. [verified]

4. **Citation drift across artifacts.** `proposal.md:29` cites `ui/src/lib/labelGrid.ts:18` for `LabelGridRow.option`; it is `:17` (`ui/src/lib/labelGrid.ts:17`), which is what `design.md:19` says. `design.md:19` cites `ui/src/pages/Import.tsx:238` for `option: {}`; the literal is `:237`. And `proposal.md:29` says `ui/src/components/LabelGrid.tsx:210` "is updated accordingly" while `design.md:19` decides the field stays required and that file "continues to read `row.option`" — an implementer following the proposal edits a file the design leaves alone, and `ui/src/components/LabelGrid.test.tsx:294-325` (which asserts `updated[0].option.style`) is not in Impact. [verified]

5. **`design.md:25` overstates the empty-cell continuity.** After the change no former `option.<name>` header survives to reach the data map at all: `option.size` is refused as an unknown column. The `""` behaviour it describes is the pre-existing data-column behaviour (`src/api.rs:2263`), not a new state a former option cell lands in. The delta text at `specs/param-resolution/spec.md:54-58` is right; the risk note reads as though a path is preserved that is not.

## Checks that passed

`openspec validate --strict` passes; the `REMOVED` + `ADDED` mechanism is sound and I raise nothing against it. Directional phrases hold: `specs/param-resolution/spec.md:17,58` are rewritten direction-agnostically, and published `:153` sits in `A declared default is resolved against one request-scoped snapshot` (`:127`) pointing at `A default that cannot be resolved` (`:302`), neither of which moves. No spec or source outside the deltas references either renamed requirement title. `docs/SPEC.md:1394` is a changelog line, not contract, so it needs no supersession. The ascending-order scenario at `specs/request-data-keys/spec.md:82-87` is correct (`option.size` < `sku_legacy` by code point). `batch-validation`'s narrowing and `template-inputs`' scope (no widening beyond the one sentence) both hold.

## Why REVISE

Finding 1 is an entire missing normative requirement that both `proposal.md` and `design.md` describe as present, and it is the mechanism the issue devotes a section to; stating it as an edit would mean authoring the published contract inside a review that no one then reviews. Finding 2 requires deciding what the "file imports" scenario should assert post-change and inverting its code twin. Both belong in a reworked plan reviewed fresh.

VERDICT: REVISE
