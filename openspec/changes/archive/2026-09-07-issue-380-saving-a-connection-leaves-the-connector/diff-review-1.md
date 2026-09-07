TREE_SHA256: 1615d23542e886156e49cfe877c5cacf15a656dfec384a31bab3b5b310d68cd7
SPECS_SHA256: 37a85aa83ce89d0cbacc8744e19844079310eaad839992871adeb9fba68a0fec

Reviewed the working-tree diff (10 UI files, no Rust) against `proposal.md`, `design.md`, `tasks.md`, the four spec deltas and AGENTS.md.

**Gates re-run here** [verified]: `npm run lint` exit 0, `npm run test` 50 files / 525 tests passed, `npm run build` exit 0, `cargo fmt --check` OK, 0 `.rs` files in the diff, `openspec validate <change> --strict` reports valid.

**Red-proof spot-checks** [verified]. I copied the tree to `/tmp/i380` and reverted each production change in turn, to confirm the new tests are load-bearing rather than passing against broken code:

- drop `refreshToken` from the browse effect deps → 3 Connect tests fail;
- drop `qc.cancelQueries(...)` from `useSaveConnection` → both `connectors.test.ts` save tests fail;
- return the override's `visible` set directly instead of resolving it → "a newly derived column appears for a resource customized in this session" fails;
- gut the `ConnectionsSection` delete listener → both editor-retirement tests fail;
- gut the `Connect` delete listener → both delete tests fail.

The plan-review round-4 required changes (cancel-before-invalidate, event-owned editor retirement replacing `onDeleted`) are both present at `ui/src/api/connectors.ts:82` and `ui/src/pages/connect/ConnectionsSection.tsx:583-592`.

Two findings.

---

**1. Blocking. Task 6.3 is ticked and its spec scenario has no test; the test that claims to cover it exercises neither a sort nor a column filter.**

`ui/src/pages/Connect.test.tsx:1778` is named `"guard: the browsing context (sort, filters, visible columns, selection) survives the refresh"`. Between the first render at line 1821 and the save at line 1830 the only view state established is a row selection (line 1825). No column header is clicked, no filter input is changed. The assertions at lines 1833-1842 check the checkbox and two column headers and nothing else.

`tasks.md:109-113` is checked `[x]` and reads "the sort, the column filters, the visible columns and the row selection survive a refresh that removes no column they name". The `connections` delta scenario "The browsing context survives the refresh" (`specs/connections/spec.md`) says "WHEN a save refreshes the rows while **a sort, a column filter** and a row selection are in force ... THEN the sort, the column filter, the visible columns and the selection are all still in force afterwards". Neither of the two named controls is in force in the test.

The `ConnectorBrowser` guard tests do not fill the gap: they cover the opposite case, a schema change that *removes* the sorted/filtered column.

Failure scenario: sort by Category, filter Name for "Dr", save the connection. Nothing in the suite says the sort and the filter are still in force after the refresh, so a later change to the refresh path that resets `sortState` or `columnFilters` ships green.

Aggravating: the test also does not count browse requests, so it passes unchanged when `refreshToken` is removed from the effect deps [verified in the mutation run above, where it was not among the 3 failures]. It asserts nothing about a refresh having occurred at all.

AGENTS.md: "A claim you did not earn is worse than an admitted gap. A checked box, a ticked criterion, a 'verified' in a report: each is a claim the next reader trusts instead of redoing the work."

---

**2. Blocking. `connectorColumns.ts` keeps two dead compatibility wrappers as a second spelling of the new API, and the whole unit suite tests the wrappers rather than the shipped path.**

`ui/src/pages/connect/connectorColumns.ts:105` (`loadSavedColumnKeys`) and `:130` (`saveColumnKeys`) have no production caller. `ConnectorBrowser.tsx:92-93` uses `loadSavedColumnChoice` + `resolveColumnKeys`, and `:119-121` uses `makeColumnChoice` + `saveColumnChoice`. Grep over `ui/src` shows the only references to the two wrappers are in `connectorColumns.test.ts` (lines 3, 34, 40, 41, 47, 53, 59, 91, 101, 102, 116).

`saveColumnKeys` is worse than dead: it carries a three-way overload (`Set | ColumnChoice`, optional `columns`), and its third branch at `:141-144` writes `hiddenDerived: []` unconditionally. Failure scenario: any future caller reaching for the old three-argument name to persist a choice that hides a transform-derived column silently discards the hiding, so the column reappears on the next load, which is the exact defect this change exists to prevent. The branch is live in the test at `connectorColumns.test.ts:40`, so it is preserved and exercised while being wrong for the new contract.

Task 5.5 asked for the existing column-visibility tests to be updated; they were left pointing at the wrappers, which is why the wrappers had to stay. Consequence: the resolver path the app actually runs has no direct unit coverage, only transitive coverage through `ConnectorBrowser` component tests.

AGENTS.md, "Breaking changes, until 1.0": "No migration, no desugaring, no deprecation window, no second spelling, and no paragraph explaining the one being removed." Neither `design.md` nor `tasks.md` asks for these wrappers; `tasks.md:83-86` specifies "the single resolver".

---

Everything else checked out: the cancel/invalidate ordering and its order assertion, the event-before-`removeQueries` ordering on delete, the functional `setEditing` update reading the current editor, the `activeFilters` and `comparedRows` no-new-code claims for a withdrawn column, the `tier: derived && !transform_source` predicate against `item_url`/`location_url`, the legacy array-shaped storage read, and the four delta capabilities all matching what shipped.

VERDICT: REVISE
