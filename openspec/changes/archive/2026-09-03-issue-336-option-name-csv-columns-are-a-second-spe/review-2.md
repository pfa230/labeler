Read: `ANSWERS.md`, `proposal.md`, `design.md`, all four delta specs, `review-1.md`, the four published specs they touch, `docs/SPEC.md:755-760`, and every code location cited (`src/reason.rs:94`, `src/api.rs:2255-2265,2720-2764`, `src/render/mod.rs:60-145`, `ui/src/lib/labelGrid.ts:12-21`, `ui/src/pages/Import.tsx:175-241`, `ui/src/components/LabelGrid.tsx:205-215`). I ran `openspec validate --changes --strict` (passes, 1/1) and trial-archived the change into a scratch copy of `openspec/` outside the repo to see what actually lands.

**What is right, verified.** Review-1's findings 1 and 3 are fixed: the blank-cell scenario now says `title: ""` with no `MissingField` and the `enum` case as `422 InvalidEnumValue`, which matches `src/render/helpers.rs` absence-only `MissingField` and `coerce_param_value`'s enum arm (`src/render/mod.rs:68-77`); the requirement title no longer carries "option prefix". Finding 4 is fixed by the new `template-inputs` delta. `REMOVED` + `ADDED` with distinct titles is the mechanism `--strict` accepts. The directional-phrase audit holds under trial archive: the moved requirement lands last in `param-resolution` (`:498`), the only surviving `requirement below` (`:37`) still points at `A default that cannot be resolved` (`:185`), and `request-data-keys:29,76` still point at the CSV requirement (`:145`). The four deltas cover every published mention of `option.` columns; nothing dangles. The withdrawal table is in the shape `scan_canonical_withdrawals` scans and no other table in the file sits under a "withdrawn" heading.

Five real problems.

## 1. MAJOR: `template-inputs` delta:11 publishes a self-contradictory sentence

`specs/template-inputs/spec.md:11` ends:

> Every CSV column is now a data column judged under `csv_data_column_unknown`; `csv_option_column_unknown` is withdrawn alongside this slug (see `request-data-keys`), **not this slug**.

[verified] "not this slug" is the tail of the published sentence it replaced (`openspec/specs/template-inputs/spec.md:1666`: "…judged under `csv_option_column_unknown` (`docs/SPEC.md:758`), not this slug"), left behind after the first half was rewritten. As written it says `csv_option_column_unknown` is withdrawn alongside `options_not_supported` and also not alongside it. This lands verbatim in a published requirement.

## 2. MAJOR: em dashes replaced by colons in three files, breaking two paired appositives

The published text uses paired em dashes. The deltas replace them with colons in text the change does not otherwise touch, and in two places the result is ungrammatical because a colon cannot close what an em dash opened.

`specs/batch-validation/spec.md:57-59` [verified against `openspec/specs/batch-validation/spec.md:61-63`]:

> On `POST /api/import/csv` the file's own refusals**:** an unparsable header, … and an unrecognized data column (`request-data-keys`)**:** are likewise whole-request refusals raised before any label exists.

`specs/template-inputs/spec.md:11` [verified against published `:1666`]:

> … has no `option` field so a carried key is dropped rather than forwarded**:** and `LabelInput`/`RenderLabelRequest` now both carry `deny_unknown_fields` …

The same substitution also hits `specs/request-data-keys/spec.md:28-31` (refusal-order items 1-3) and `:39` ("what a caller can observe**:** that no **label**"), where it reads acceptably but is still an edit to text the issue does not name. `design.md:17` claims each `MODIFIED` block "changes only the retired-category sentences"; it does not.

## 3. MEDIUM: eight unchanged scenario bodies still reflow, against trap 2 and against `design.md:17`

The prose paragraphs were rewrapped since review-1, but the scenarios were not. `design.md:17` claims "`ADDED` blocks follow the file's ~100-column style". [verified] They do not, and archive lands the block verbatim: in the trial archive the reflowed lines land at `param-resolution:559-605` and `request-data-keys:189-217`.

Delta line lengths against the published wrapping of the same, unchanged text:

- `specs/param-resolution/spec.md`: `:72`=126 (published wraps at `:69-70`), `:77`=140 (`:75-76`), `:82`=150 (`:81-82`), `:87`=117 (`:87-88`), `:92`=150 and `:93`=110 (`:93-96`), `:97`=118 (`:100-101`), `:107`=101 (`:111`). Also the two changed scenarios `:113`=116 and `:117-118`=133/263.
- `specs/request-data-keys/spec.md`: `:54-55`=153/120 (published `:195-196`), `:67`=125 (`:210-211`). Also the changed scenario `:81-82`=112/119.

Both files wrap prose at ≤104 throughout. (`request-data-keys:50` and `:100` are table rows and are correctly left long; `template-inputs`'s unwrapped lines correctly match its own unwrapped published block at `:1660,1666,1668`.)

## 4. MEDIUM: the quoted §10.1 row has nested backticks and will not render

`specs/request-data-keys/spec.md:91-93`:

> This requirement supersedes the `docs/SPEC.md` §10.1 row `` `| `InvalidRequest` | `csv_option_column_unknown` | An `option.*` CSV column names an option the template does not declare. |` ``

[verified] The outer span opens at the backtick before `|` and closes at the backtick before `InvalidRequest`, so under CommonMark the row renders as alternating code fragments and bare text rather than one quoted row. The `#337` requirement the issue told this plan to follow avoids exactly this by dropping the inner backticks (`openspec/specs/template-inputs/spec.md:1660`: `` `| InvalidRequest | options_not_supported | An option selection was sent … |` ``). Harmless to `scan_canonical_withdrawals`, which only reads lines beginning with `|`, but it is a formatting defect in normative published text.

## 5. MEDIUM: `proposal.md:29` cites a design decision `design.md` does not contain

`proposal.md:29`: "`ui/src/lib/labelGrid.ts:18` `LabelGridRow.option` is required, so the literal keeps `option: {}` or the type is changed (**choice recorded in design**)".

[verified] `grep` for `labelGrid`, `LabelGridRow`, `option: {}` and `LabelGrid` in `design.md` returns nothing. The fork is real: `LabelGridRow.option` is a required field (`ui/src/lib/labelGrid.ts:17`) and `ui/src/pages/Import.tsx:238` is its only CSV-path writer, so deleting `r.option` is a type error unless one branch is taken. The implementer is pointed at a recorded decision that does not exist.

## Minor, not blocking

- `specs/batch-validation/spec.md` gains a trailing blank line at EOF versus the published block. Cosmetic; archive tolerated it in the trial run.
- `design.md:15` describes the withdrawal requirement as containing "the `The csv_option_column_unknown slug SHALL be withdrawn…` table"; the table is the block at `:98-100` and the quoted string is the lead-in sentence at `:96`. Reads oddly but names the right artifact.

## Required changes

Apply all five. Nothing else in the artifacts needs to move. After edit 3, `design.md:17`'s claim about ~100-column style becomes true, and after edit 2 its claim that each `MODIFIED` block changes only the retired-category sentences becomes true, so `design.md:17` itself needs no edit.

1. **`specs/template-inputs/spec.md:11`** — replace the final sentence of the table cell, currently `Every CSV column is now a data column judged under `csv_data_column_unknown`; `csv_option_column_unknown` is withdrawn alongside this slug (see `request-data-keys`), not this slug.`, with exactly: `Every CSV column on `POST /api/import/csv` is now a data column judged under `csv_data_column_unknown`, and `csv_option_column_unknown` is itself withdrawn (`request-data-keys`).`

2. **Restore every em dash that was replaced by a colon in otherwise-unchanged text**, so the only edits to those sentences are the ones the change means to make. Seven sites:
   - `specs/template-inputs/spec.md:11` — `already unreachable at HEAD: every production call site` → `already unreachable at HEAD — every production call site`; and `dropped rather than forwarded: and `LabelInput`/` → `dropped rather than forwarded — and `LabelInput`/`.
   - `specs/batch-validation/spec.md:57` — `the file's own refusals: an unparsable header` → `the file's own refusals — an unparsable header`; and `:59` — `(`request-data-keys`): are likewise whole-request refusals` → `(`request-data-keys`) — are likewise whole-request refusals`. Leave `an unrecognized data column` as the delta already has it; that narrowing is the intended edit.
   - `specs/request-data-keys/spec.md:28,29,30,31` — restore ` — ` before each of `` `csv_header_invalid`; ``, `` `csv_row_invalid`; ``, `` `csv_empty`; `` and `` `csv_data_column_unknown`. `` in place of the colon, matching published `:167-171`.
   - `specs/request-data-keys/spec.md:39` — `what a caller can observe: that no **label** is` → `what a caller can observe — that no **label** is`.

3. **Rewrap every scenario bullet in both `ADDED` blocks to the published width**, ≤100 columns with continuation lines indented two spaces.
   - `specs/param-resolution/spec.md`: for the eight unchanged scenarios, copy the bullets verbatim from `openspec/specs/param-resolution/spec.md:69-70, 75-76, 81-82, 87-89, 93-96, 100-102, 106-107, 111-112`. Wrap the two changed scenarios' bullets (`:113`, `:117-118`) the same way.
   - `specs/request-data-keys/spec.md`: for the two unchanged scenarios, copy the bullets verbatim from `openspec/specs/request-data-keys/spec.md:195-196` and `:210-211`. Wrap the changed scenario's bullets (`:81-82`) the same way. Leave the table rows at `:50` and `:100` on one line.

4. **`specs/request-data-keys/spec.md:91-93`** — replace the quoted row with the `#337` spelling, dropping the inner backticks, wrapped at ≤100 columns:
   `This requirement supersedes the `docs/SPEC.md` §10.1 row `| InvalidRequest | csv_option_column_unknown | An option.* CSV column names an option the template does not declare. |` and states its complete post-change contract.`
   Leave the two sentences that follow (`The frozen document is not edited; …remains authoritative.`) unchanged.

5. **`design.md`** — add one decision under `## Decisions` recording which branch the UI takes for `LabelGridRow.option` (`ui/src/lib/labelGrid.ts:17`): either `ui/src/pages/Import.tsx:238` keeps `option: {}` with the field left required, or `LabelGridRow.option` and `LabelGridRow.validation.option` become optional. Name the file and line in the decision, and say which of `ui/src/components/LabelGrid.tsx:208-216` and `ui/src/pages/Import.tsx:180` follow from it, so `proposal.md:29`'s "choice recorded in design" is true.

VERDICT: APPROVE_WITH_CHANGES
