TREE_SHA256: feb18128883af20bb209b6c029351f0479e264c4878bf08ba92219b38f03202a

Gates run locally, all green [verified]: `cargo fmt --check` (exit 0), `cargo clippy --all-targets --all-features` (0 warnings), `cargo test` (794 passed, 0 failed, 2 ignored), `npm run lint`, `npm run test` (49 files / 441 tests), `npm run build`. `specs-digest.sh` recomputes `b5c04696…`, matching `review.md`'s `SPECS_SHA256`, and `review-gate-check.sh . --plan-only` exits 0, so the plan verdict is not voided.

Round 2's findings were addressed: the placeholder fill is now pinned at `src/templates.rs:6519-6523` (`assert_eq!(ph_list_no_def.get("tags"), Some(&json!(["tags"])))`), `json_to_param_value`'s array arm fails loudly instead of coercing (`src/render/mod.rs:441-451`), and task 5.5's two missing claims are asserted (`src/lib.rs:8836-8841`, `:8862`).

---

## BLOCKING 1 — The fix for round 2's CSV-grid finding has no test that can fail, and task 6.3 is checked

Two lines answer round 2's MAJOR 2. Neither is exercised.

- `ui/src/pages/Import.tsx:128-140`: `listNames` is collected from `detail.inputs`, and `displayedFields` filters it out of the column set.
- `ui/src/lib/labelInputs.ts:251-256`: `pruneDataForSubmit` drops a `list` entry unless its value is an array, which is what stops a CSV string reaching the batch body.

Verified by mutation [verified]: I copied the tree to `/tmp`, deleted the `listNames` filter alone (441 passed), deleted the `pruneDataForSubmit` branch alone (441 passed), and deleted both (441 passed). All 49 files / 441 tests stay green in every case.

The test written for task 6.4, `ui/src/pages/Import.test.tsx:688-737`, loads the CSV `sku\n123\n` (`:729`). It has no `tags` column, so `expect(screen.queryByText("tags")).toBeNull()` (`:734`) passes for the pre-fix reason: `requiredUnion` already excluded `list` before `listNames` existed (`Import.tsx:114-118`), so the column was absent either way. The assertion cannot distinguish the fixed code from the code round 2 rejected.

Failure scenario the suite would miss: a template declares `tags: { type: list }`; the operator pastes `sku,tags\n123,red;blue\n`. Without `Import.tsx:139`, `csvFields` puts `tags` on screen and every cell draws an inert `—` (`LabelGrid.tsx:151,155`). Without `labelInputs.ts:251-256`, `row.data.tags` (set by `Import.tsx:234`, which copies every CSV column) is a string, `activeMap.get("tags")` finds the reported entry so the `!input` skip at `:250` does not fire, `typeof v === "string"` at `:258` passes, and the batch POST carries `tags: "red;blue"`. The server answers `400 InvalidRequest` / `request_body_invalid` (`src/render/mod.rs:157`) for the whole batch, with nothing in the grid pointing at the cause.

Connect got the equivalent test and it does bite: `ui/src/pages/Connect.test.tsx:558` asserts `queryByLabelText("map tags")` is null, which exercises `Connect.tsx:125`. Import did not, which is what shows this is an omission rather than a decision.

AGENTS.md: "A checked box is a claim the next reader trusts instead of redoing the work, so check one only after performing it." Task 6.3 names the CSV import grid explicitly, and task 6.4 claims the grids are tested.

## MINOR 2 — `list-params`' "An empty array is not an omission" is a scenario no test observes

`src/lib.rs:8249-8259` sends `tags: []` against a template declaring `default: [KIDS, CONSUMABLE]` and asserts `status == OK` and nothing else. A 200 PNG is produced whether `[]` is honoured or silently replaced by the declared default, so the scenario's actual claim ("the joined text is empty, rather than the declared default being used") is unverified. The nearest test, `src/templates.rs:6588-6600`, covers the other case: a declared `default: []` with the request omitting `tags`.

Risk is low, because `resolve_parameters_mode`'s `List` arm (`src/render/mod.rs:254-258`) has no empty-array branch to get wrong: `Some(Array([]))` simply falls to `coerce_param_value`. One assertion on `resolve_parameters(...).data.get("tags")`, as `templates.rs:6598` already does for the sibling case, would close it.

## MINOR 3 — The `LabelGrid` list handling is unreachable through either consumer

`ui/src/components/LabelGrid.tsx:151,155,196` guard on `spec.control === "list"`, but no `cellInput` can ever return a `list` spec: Import passes `fields={displayedFields}` with lists filtered out (`Import.tsx:139`), and Connect's `displayedFields = requiredUnion` also excludes them (`Connect.tsx:158,165`). The only thing that reaches those branches is the synthetic `LabelGrid.test.tsx:329-350`, which injects a `list` spec directly.

Task 6.3 asked for all three files, so this is not a deviation from the plan, and defence in depth against #318's editor is defensible. Noting it so the next reader knows the `—` behaviour those lines describe is not a state any screen currently produces.

---

Finding 1 must be fixed before this lands: it is a checked task whose stated verification does not exist, on the exact lines a prior round rejected, and I confirmed by mutation that deleting them changes no test result.

VERDICT: REVISE
