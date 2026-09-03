# Diff review

AUTHORS: agy
REVIEWER: claude
VERDICT: APPROVE
ROUNDS: 2
TREE_SHA256: 9fa0d1745b428c838aedac6a34efde741c39f7bbf033b8f091fe049e7103c17b
SPECS_SHA256: c00cf24c42edf79915cb126c0df2ed34925eb689edd0a10e9f89fa5d9fb59990

## Diff review: issue-321 (connector rows carry lists), round 2

Gates run in this worktree, all green [verified]: `cargo fmt --check` = 0, `cargo clippy --all-targets --all-features` = 0, `cargo test` = 0, `npm run lint` = 0, `npm run test` = 49 files / 453 tests passed, `npm run build` = 0.

### Round-1 blocking findings: both resolved [verified]

1. **Flaky batch-submit test.** `ui/src/pages/Connect.test.tsx:837-842` now wraps the click and the `submittedBatch` assertion in a single `waitFor`, so the click retries past the `rowsPending` bail at `ui/src/pages/Connect.tsx:255`. Ten isolated runs of that file: 19/19 passed each time.
2. **Second spelling in `defaultMapping` / `validateMapping`.** `ui/src/lib/connectorRows.ts:17` and `:44` now take `InputSpec[]` and `FieldSpec[]` only. No `typeof c === "string"` branch, no `?? false` coalesce over the required `multi_valued` key. The old test line was updated rather than kept alive.

Round-1 non-blocking items 3 through 7 were also all addressed: arrays now return `null` from `numberKey` (`connectorSort.ts:22`) and `dateKey` (`:34`), with single-element list cases pinned at `connectorSort.test.ts:236` and `:248`; `extract_field` gained a `resource` parameter (`src/connector/homebox.rs:573-580`) with an assertion that `locations` still yields `""` (`:964`); `From<&ColumnDef> for FieldSpec` (`src/connector/mod.rs:288-298`) replaced the three inlined literals; `required: true` is back on the unmapped-list test (`Connect.test.tsx:710`); `displayCellText` is typed `CellValue | ParamValue | undefined`.

### Verified against the delta

`multi_valued` present unconditionally on every `FieldSpec` including derived (`src/lib.rs`, `connection_schema_reports_tags_multi_valued_and_derived_columns`); browse and materialize byte-identity with `quantity` asserted as a JSON string; `[]` asserted as neither `""`, `null`, nor absent on both endpoints; one upstream request asserted against `hb.received_requests()`; the multi-valued transform refusal ordered ahead of the not-text check (`src/connector/mod.rs:196-201`) with a paired positive case; `RowValue` kept distinct from `CellValue`; mapping refusal in both directions blocking "Add rows"; `pruneDataForSubmit` passing `[]` through. `SPECS_SHA256` still recomputes to `c00cf24c...`, so `specs/` is untouched since the plan verdict.

I also confirmed the upstream contract this rests on, which the proposal asserted but nothing in-repo proved: `sysadminsmedia/homebox` `backend/internal/data/repo/repo_entities.go:169` declares `Tags []TagSummary \`json:"tags"\`` on `EntitySummary`, `EntityOut` embeds `EntitySummary` (`:188`), and `repo_tags.go:54` declares `Name string` as a non-pointer, so the strict `TagSummary { name: String }` deserialization at `src/connector/homebox.rs:446-449` cannot fail against a real Homebox and the `cheap` tier claim is real. [verified]

### Non-blocking

**1. The `connector-browser` delta still contradicts itself for a single-element list; the code picked a side.**
`specs/connector-browser/spec.md` says a multi-valued cell is compared "by its display text ... on exactly the terms above, with no rule of its own", then says a multi-valued cell in a `number`, `money` or `date` column "is uninterpretable as that type and orders with the blanks". For `["5"]` in a `number` column those disagree: the first yields 5, the second yields blank. `connectorSort.ts:22` and `:34` implement the second, and `connectorSort.test.ts:239,251` pin it. Unreachable today, since `tags` is the only multi-valued column and its `ty` is `text`. Left as-is deliberately, I assume: correcting the sentence means editing `specs/`, which voids the plan verdict and buys a full re-plan-review for a contradiction no caller can reach. Flagging so the choice is on the record rather than implied.

**2. No test asserts the browse table itself renders the joined text.**
The delta scenario "The browse table renders the joined elements" maps to `NameCell` (`ui/src/pages/connect/ConnectorBrowser.tsx:53`). The `Connect.test.tsx` assertions are all `within(grid)`-scoped to the label-row grid, and no `ConnectorBrowser` test carries an array cell. The behaviour is covered indirectly by `displayCellText`'s unit test and the filter test that pins "filter matches what is on screen", so the risk is small, but the scenario has no direct assertion.

**3. `displayCellText` lives in a connector module that the generic grid now imports.**
`ui/src/components/LabelGrid.tsx:5` imports from `../lib/connectorRows`, so the shared grid used by Import and the batch page now depends on the connector row module. Task 5.2 asked for "a shared module under `ui/src/lib/`" and `connectorRows.ts` satisfies that literally. A small module of its own would keep the dependency pointing the other way.

**4. The mapping's cardinality lookup flattens resources with last-write-wins.**
`ui/src/pages/Connect.tsx:124` builds `connectorColumns` by `flatMap` across resources, and both `defaultMapping` (`connectorRows.ts:22-25`) and `validateMapping` (`:50-53`) collapse it to one `key -> multi_valued` map. A key declared on two resources with differing cardinality would resolve to whichever came last. Homebox's duplicate keys (`name`, `description`) are scalar on both, so it is latent only.

