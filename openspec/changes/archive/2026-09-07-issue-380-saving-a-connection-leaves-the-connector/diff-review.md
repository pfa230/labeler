# Diff review

AUTHORS: agy
REVIEWER: claude
VERDICT: APPROVE
ROUNDS: 2
TREE_SHA256: 6b745d8fec1599578715e5149bf623b9aad1772318680731670a09f184ac0967
SPECS_SHA256: 37a85aa83ce89d0cbacc8744e19844079310eaad839992871adeb9fba68a0fec

Reviewed the working-tree diff (10 UI files, no Rust) against `proposal.md`, `design.md`, `tasks.md`, the four spec deltas and AGENTS.md.

**Gates re-run here** [verified]: `npm run lint` exit 0; `npm run test` 50 files / 533 tests passed; `npm run build` exit 0; `openspec validate <change> --strict` reports valid; `git status` shows 0 `.rs` files changed, so task 7.4's Rust claim reduces to "the tree is unchanged there", which it is.

**Delta completeness** [verified]: I extracted both MODIFIED requirements from the published specs and diffed them against the delta copies. `default-connection` "Connect opens on a resolved connection" and `connector-browser` "Ordering and filtering are transient..." are reproduced whole with additions only, nothing dropped. Both ADDED requirement names are new against `openspec/specs/`.

**Red-proof by mutation** [verified]. I copied the tree to `/tmp/i380m` and reverted each production change in turn:

- drop `refreshToken` from the browse effect deps (`ConnectorBrowser.tsx:243`) → 4 Connect tests fail, **including** the revised `"guard: the browsing context ... survives the refresh"`, which was the prior review's blocking finding 1;
- drop `qc.cancelQueries` (`connectors.ts:82`) → both `connectors.test.ts` save tests fail;
- revert `defaultColumnKeys` to cheap-only → 7 tests fail across three files;
- return the override's `visible` set ahead of the resolver → `"a newly derived column appears for a resource customized in this session"` fails;
- gut both `labeler:connection-deleted` listeners → 4 delete tests fail.

**Both prior blocking findings are genuinely fixed.** The guard test at `Connect.test.tsx:1778` now clicks the Category header, types `"Dr"` into the Name filter, and asserts `browseRequests === 2`, so it is load-bearing under the deps mutation. The `loadSavedColumnKeys` / `saveColumnKeys` wrappers are gone: `grep` over `ui/src` finds zero references, and the unit suite now tests `resolveColumnKeys` / `makeColumnChoice` / `loadSavedColumnChoice` directly.

Also checked and correct: the `transform_source` predicate against the Rust source (`connector/mod.rs:341` gives `!multi_valued && ty == Text`, `mod.rs:565` gives `false` for transform columns, and homebox's `item_url` / `location_url` are both single-valued Text at `homebox.rs:85-91,116-122`, so both stay hidden); the event-before-`removeQueries` ordering on delete; the functional `setEditing` update; the legacy array-shaped storage read; the deletion of the duplicated invalidation at `ConnectionsSection.tsx:241`; the removal of the whole per-call `onDeleted` path; and the raw event-name string literals, which match the `labeler:unauthenticated` precedent (`client.ts:16`, `RequireAuth.tsx:10`) rather than inventing a second convention.

Two non-blocking findings.

---

**1. Non-blocking. `makeColumnChoice` recomputes `hiddenDerived` from the current schema, so a recorded hiding is lost if the operator touches the picker while that column is out of the schema.**

`connectorColumns.ts:59-64` builds `hiddenDerived` by walking only the `columns` passed in. The prior choice is never consulted, so any key it hid that the current schema no longer offers is dropped from the record on the next write.

Failure scenario: the operator hides the transform-derived `location_id` (choice becomes `{visible:["name"], hiddenDerived:["location_id"]}`); saves the connection with that rule removed, so `location_id` leaves the schema; hides `description` through the picker, at which point `setVisibleKeysForCurrent` (`ConnectorBrowser.tsx:117-121`) writes `hiddenDerived: []`; then restores the rule. `location_id` opens visible again, contradicting `connector-browser`'s "Hiding a transform-derived column SHALL therefore be recorded ... so that the hiding survives a reload and a later save alike".

Not blocking: no scenario in the delta pins this sequence, "A column that comes back resumes" is scoped to sort and filter rather than visibility, and the outcome is one extra visible column rather than a wrong value. Worth recording as a decision instead of leaving it as an accident, since the fix is one merge of the prior `hiddenDerived` into the new one.

**2. Non-blocking. All 11 new Connect tests sit inside `describe("Connect: datetime parameters")`.**

`Connect.test.tsx:727` opens that block and `:1877` closes it; every test added by this change (`:1141` through `:1876`) falls inside it, and none concerns datetime parameters. Failure scenario: CI prints `Connect: datetime parameters > a delete clears the selection before connections list reloads ...`, sending whoever triages it to the datetime fixtures. The block's `beforeEach` also installs the `withPrintedOn` fetch mock, which each new test then replaces wholesale, so the inherited setup is dead weight rather than shared context.

---

Everything the proposal, design, tasks and four deltas ask for is present and covered by tests that fail when the production change is reverted.

