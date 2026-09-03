Read `ANSWERS.md` first; it settles the delta mechanism (author's choice) and fixes the goal as: no published requirement may specify an "option column"/"option cell" as a category, and `openspec validate --strict` plus `cargo test` must pass. Findings below are judged against that, `AGENTS.md`, and `openspec/config.yaml`.

I verified: `openspec validate --strict` passes on the change as written [verified], and the design's premise that a `MODIFIED` with renamed scenarios fails strict validation is true (reproduced in a scratch copy: `MODIFIED "A parameter is required unless the template declares a default" omits scenario(s) the current spec still has`) [verified]. So the `REMOVED` + `ADDED` mechanism itself is sound and I raise nothing against it.

## Blocking

**1. The withdrawal requirement the plan is built around does not exist in any delta. `cargo test` will fail.**

`proposal.md:20` states "An `ADDED` requirement withdraws `csv_option_column_unknown` and supersedes the frozen `docs/SPEC.md:758` row with its complete post-change contract", and `design.md:15` specifies it by name, with "the `The csv_option_column_unknown slug SHALL be withdrawn, and SHALL NOT be raised by any code path:` table in the shape `scan_canonical_withdrawals` scans". No such requirement, sentence, or table row is in `specs/request-data-keys/spec.md` or anywhere else: `grep -rn "withdraw" specs/` returns only the `options_not_supported` requirement carried in the `template-inputs` delta, the two `REMOVED` **Reason** lines, and two scenario headings [verified].

`scan_canonical_withdrawals` (`src/errors.rs:731-765`) captures a slug only from the **first cell** of a table row (`line.split('|').nth(1)`) under a line containing "withdrawn". The only mention of `csv_option_column_unknown` the deltas add sits in the *second* cell of the `options_not_supported` row (`specs/template-inputs/spec.md:14`), which the scanner never reads. So after archive, `canonical_withdrawn_refs` will not contain the slug, `docs/SPEC.md:758` still lists it in §10.1, and with `Reason::CsvOptionColumnUnknown` deleted the phantom assert at `src/errors.rs:796-806` fails with `SPEC §10.1 documents reasons that do not exist: ["csv_option_column_unknown"]`.

Consequences that follow from the same gap:
- Acceptance criterion 5 ("A published requirement withdraws `csv_option_column_unknown`, so `cargo test` passes with the variant deleted and would fail if it were reintroduced") is unmet.
- The issue's Spec delta section names "a withdrawal requirement for `csv_option_column_unknown`, and a first-touch `ADDED` requirement over the frozen `docs/SPEC.md:758` row". The `ADDED` block's supersession paragraph (`specs/request-data-keys/spec.md:44-46`) is the published one for `csv_data_column_unknown` and says nothing about the `:758` row, so first-touch over that row is also unwritten.
- `specs/request-data-keys/spec.md:94-98` publishes a scenario, "A withdrawn slug is unreachable", inside a requirement whose normative text never mentions `csv_option_column_unknown` and never withdraws it. Its "**AND** the registry test fails if it is reintroduced without a spec change" is false as the delta stands: the reintroduction assert (`src/errors.rs:788-794`) keys off `canonical_withdrawn_refs`, which will be empty of this slug.
- `specs/template-inputs/spec.md:14` publishes the cross-reference "`csv_option_column_unknown` is itself withdrawn (`request-data-keys`)" pointing at a requirement that does not exist.

**2. The `ADDED` requirement publishes a scenario the change makes false, and keeps the retired category in normative text.**

`specs/request-data-keys/spec.md:89-92`:

```
#### Scenario: A file naming only declared parameters imports
- **WHEN** the header names only declared parameters, alongside `option.` columns naming declared ones
- **THEN** the file imports exactly as it does today
```

This is copied verbatim from `openspec/specs/request-data-keys/spec.md:228-231`, where it was true because the prefix was split off and folded in. After this change `option.orientation` is an ordinary header naming no declared parameter, so step 4 of the very same requirement (`:31`) refuses it with `csv_data_column_unknown`. The scenario contradicts its own requirement.

It is also the one place the `ADDED` block still asserts `option.` columns as a working category, which is exactly what `ANSWERS.md` forbids ("No published requirement may specify an 'option column' or 'option cell' as a category").

Its code twin confirms the direction: `src/lib.rs:11412-11421` ("5.6 File naming only declared params alongside option. columns naming declared ones still imports") posts `id,url,name,tags,description,option.orientation,option.outline` and asserts `StatusCode::OK` [verified]. That test must invert to a `400`/`csv_data_column_unknown`, so the plan currently tells the implementer to satisfy a scenario and a test that state opposite things.

## Non-blocking, but real

**3. The `template-inputs` `MODIFIED` block reflows two paragraphs the change does not touch.** Diffing the block against its published form, the only intended edit is the last sentence of the table cell. But `specs/template-inputs/spec.md:5-8` and `:16-18` rewrap the supersession paragraph and the ADR-0052 paragraph, which are single unwrapped lines in the published file (`openspec/specs/template-inputs/spec.md:1660`, `:1668`) [verified by diff]. Archive lands a `MODIFIED` block verbatim, so this permanently rewraps text this change did not mean to touch, which is trap 2 in the issue, and it falsifies `design.md:17`'s own claim that "Every `MODIFIED` block copies its published block and changes only the retired-category sentences, preserving column width". The `batch-validation` `MODIFIED` is clean by comparison: its only diff is `of either kind` → `data` [verified].

**4. Both `ADDED` blocks rewrap unchanged prose at a narrower width than the file they land in.** Of the `param-resolution` `ADDED` block's 91 non-blank lines, 41 match the published text line-for-line and the rest are rewrapped without a word changing; for `request-data-keys` it is 36 of 64 [verified]. The `param-resolution` block wraps to a mean of 90.5 columns against the published requirement's 95.8. `design.md:17` says `ADDED` blocks "follow the file's ~100-column style"; they do not. With `REMOVED` + `ADDED` the block is re-emitted anyway, but copying each unchanged paragraph line-for-line and rewrapping only the changed ones would keep the archived diff readable.

**5. Line citations drift between artifacts.** `proposal.md:29` cites `ui/src/lib/labelGrid.ts:18` for `LabelGridRow.option`; it is at `:17` (`:19` is `validation`), which is what `design.md:19` says. `design.md:19` cites `ui/src/pages/Import.tsx:238` for `option: {}`; the literal is at `:237`. Also, `proposal.md:29` says "`ui/src/components/LabelGrid.tsx:210` is updated accordingly" while `design.md:19` decides the field stays required and `LabelGrid.tsx:208-216` "continues to read `row.option`", i.e. is not updated. An implementer following the proposal edits a file the design leaves alone, and `ui/src/components/LabelGrid.test.tsx:325` (`updated[0].option.style`) is not in the Impact list that would then break.

## Checks that passed

- `param-resolution` cross-references: `:17` and `:55` are rewritten to direction-agnostic forms inside the moved requirement, and `:153` ("under the requirement below") sits in `A declared default is resolved against one request-scoped snapshot` (starts `:127`) pointing at `A default that cannot be resolved…` (`:302`), neither of which moves, so the design's reading is right [verified].
- The empty-cell semantics change is real, not invented: `src/api.rs:2258-2263` inserts `String(val)` for every non-prefixed header including an empty one, while the deleted fold at `:2764-2769` skipped empty option cells [verified].
- The new ascending-order scenario is correct: `option.size` sorts before `sku_legacy` by code point.
- `batch-validation` narrowing and the scope discipline on `template-inputs` (no widening beyond the `csv_option_column_unknown` sentence) both hold.

## Why REVISE rather than APPROVE_WITH_CHANGES

Finding 1 is a whole missing normative requirement that the proposal and design both describe as present, and it is the mechanism the issue devotes a section to. Writing it is not an edit I can state completely without authoring the contract, and finding 2 requires deciding what the "file imports" scenario should assert post-change. Both belong in a reworked plan reviewed fresh.

VERDICT: REVISE
