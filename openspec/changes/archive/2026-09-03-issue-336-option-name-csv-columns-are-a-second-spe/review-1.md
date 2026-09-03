# Plan review: issue-336-option-name-csv-columns-are-a-second-spe

Read: `proposal.md`, `design.md`, `specs/request-data-keys/spec.md`, `specs/param-resolution/spec.md`, `ANSWERS.md`, `AGENTS.md`, `openspec/config.yaml`, the two published specs the deltas replace, and the code every citation names.

What the plan gets right, verified: the `REMOVED` + `ADDED` mechanism is necessary and correct. A same-title `REMOVED` + `ADDED` fails `openspec validate --strict` with `Requirement present in both ADDED and REMOVED` (reproduced in a scratch copy of `openspec/` outside the repo). Both ADDED blocks are otherwise faithful copies of the published requirements with only the intended edits (normalized prose diff, 10/10 scenarios preserved in `param-resolution`, 7 scenarios in `request-data-keys`). The withdrawal requirement's table is in the shape `scan_canonical_withdrawals` scans (`src/errors.rs:731-766`): the heading at `specs/request-data-keys/spec.md:71` contains "withdrawn", the row's first cell is `` `csv_option_column_unknown` ``, and the unrelated error-contract table at `:30-32` sits under a heading without "withdrawn" so it is not falsely captured. `openspec validate --changes --strict` passes today. The directional-phrase audit is right: `param-resolution:17` and `:55` are fixed inside the moved requirement, `:153` points at the `param_default_unresolvable` requirement at `:302` which does not move, and `request-data-keys:29,76` say "the requirement below" about a requirement that stays last in a two-requirement file. The `layout-sizing:1088-1096` citation for the expected pre-archive phantom failure is accurate.

Four real problems.

## 1. MAJOR: the new blank-cell scenario asserts a `422 MissingField` that cannot occur

`specs/param-resolution/spec.md:77-80`:

> **THEN** that row's label carries `orientation: ""` in its `data`, and the import fails with `422 MissingField` naming it if an active item reads it

The two halves contradict each other. [verified] With `""` present in `data`, nothing raises `MissingField`:

- `interpolate` raises it only on absence: `data.get(name).ok_or_else(|| AppError::missing_field(name))` (`src/render/helpers.rs:145-147`). A present `""` resolves to the empty string.
- `resolve_parameters_mode`'s general arm reaches the declared default only when `resolved.get(name)` is `None` (`src/render/mod.rs:332,390-397`), so the default is not consulted either.
- For a `string` parameter, `coerce_param_value` returns `Ok` for `""` (`src/render/mod.rs:139-141`), leaving `""` in place.
- For an `enum` parameter (which is what `orientation` is in the real templates), `""` fails `coerce_param_value` (`src/render/mod.rs:68-78`) and reports `422 InvalidEnumValue` (`src/errors.rs:203-213`), still not `MissingField`.

This also contradicts the requirement it sits in: `specs/param-resolution/spec.md:11-16` says a value supplied by the `data` map is not absent, and `:28` says the cell "reaches this rule as `"<name>": ""` rather than as an omission". The old published scenario (`openspec/specs/param-resolution/spec.md:120-124`) was correct for the old fold, because `src/api.rs:2767` skipped empty option cells and produced a genuine omission. The delta changed the first half of the THEN and kept the second. Published, this is a false contract, and the acceptance criterion built on it cannot be pinned by a passing test.

## 2. MAJOR: the ADDED blocks are unwrapped and would reflow both published requirements

The issue's trap 2 and `design.md:17` both require matching the published column width, and `design.md:17` claims "`ADDED` blocks follow the file's ~100-column style". [verified] They do not. Archive lands the block verbatim (confirmed against `openspec/changes/archive/2026-09-03-issue-337-.../specs/template-inputs/spec.md` versus `openspec/specs/template-inputs/spec.md:1666`), so the delta's wrapping is the wrapping that lands.

Published widths: `openspec/specs/request-data-keys/spec.md` max 140 (a table row), `openspec/specs/param-resolution/spec.md` max 115, prose wrapped at ~100 throughout. Delta paragraph lengths: `specs/request-data-keys/spec.md:11` = 436, `:26` = 644, `:73` = 354; `specs/param-resolution/spec.md:20` = 706, `:22` = 549, `:28` = 464, `:30` = 848. Every paragraph of both requirements is a single line, including roughly twenty paragraphs this change does not mean to touch (the `repeat:` clauses, the `#261` paragraph, all ten scenarios). That is exactly the permanent collateral reflow trap 2 names.

## 3. MEDIUM: the new requirement title carries the retired vocabulary into published text

`specs/request-data-keys/spec.md:9`: `### Requirement: A CSV data column names a declared parameter without option prefix`.

`ANSWERS.md` states the goal as: no published requirement may specify an "option column" or "option cell" as a category, and a heading that names it while its body describes something else fails this. The heading names the option prefix as the distinguishing property of a data column while the body says every column is a data column and no header carries an option. The distinct title is genuinely forced by the validator (finding above), but any distinct title satisfies it. This one need not name the thing being deleted.

## 4. MEDIUM: the change falsifies a published requirement in `template-inputs`, which the plan puts out of scope

`openspec/specs/template-inputs/spec.md:1666`, inside the `options_not_supported` withdrawal that this plan cites as its model:

> A caller can still supply `option.<name>` columns on `POST /api/import/csv`, but those are judged under `csv_option_column_unknown` (`docs/SPEC.md:758`), not this slug.

[verified] After this change no caller can supply an option column and `csv_option_column_unknown` does not exist. `design.md:9` declares `template-inputs` a non-goal and `proposal.md:18-21` lists only two capabilities, so this sentence survives as a published requirement asserting deleted behaviour and naming a withdrawn slug as live. Correcting it is not the widening trap 3 forbids, which was behaviour for template inputs; it is trap 3's own second half, "leave no dangling cross-reference".

## Minor, worth folding into the revision

- `proposal.md:28` points at `src/lib.rs:11378-11391` "and surrounding CSV precedence cases", but `src/lib.rs:2568-2612` holds three more CSV import cases built on the prefix, including the `csv_option_column_unknown` case at `:2589-2591` and the `option.orientation`/`option.outline` routing case at `:2570`. The file is named, so the acceptance criterion is met, but the pointer misdirects.
- `proposal.md:9,27` name `ui/src/lib/csv.ts:6,55-56,61`; the prefix also owns `csv.ts:10,14-15,22,24,74,79,81,84`. And `LabelGridRow.option` is a required field (`ui/src/lib/labelGrid.ts:18`), so deleting `Import.tsx:237` is a type error unless the literal keeps `option: {}` or the type changes. The plan names neither `labelGrid.ts` nor that choice.
- `design.md:15` says the withdrawal requirement contains a "`These slugs SHALL be withdrawn…` table"; the delta actually reads "The `csv_option_column_unknown` slug SHALL be withdrawn" (`specs/request-data-keys/spec.md:75`). Harmless for the scanner, but the design describes an artifact it does not have.

## Why REVISE rather than APPROVE_WITH_CHANGES

Finding 1 changes what the contract says, finding 2 requires rewrapping roughly 180 lines of both ADDED blocks, finding 3 renames a published requirement, and finding 4 adds a third capability delta with a `MODIFIED` block plus a new entry under Modified Capabilities. That is rework of the contract rather than a list of edits, and `specs/` moving voids the digest anyway, so the re-review is owed.

VERDICT: REVISE
