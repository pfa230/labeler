TREE_SHA256: 7609be3aaaedfeee475cec4317c126fcba853e6d48fb1ba3b6cd1c435d9452d0
SPECS_SHA256: 8e05c93ec646b33bec3879affb23ebe8e85196be5d074283f371426e01e26d4f

## Review: issue-384 implementation diff

The change is structurally faithful to its plan: routes, nav item, the split into `ConnectionsList`/`ConnectionForm`, the keyed editor subtree, per-write eviction, the four keyed mutations, the `useIsMutating` gate on Connect, the origin relay and its single-leading-slash guard are all present and match `design.md`. Both window events and `refreshToken` are gone with no dangling references [verified]. All gates pass: `npm run lint`, `npm run test`, `npm run build`, `cargo fmt --check`, `cargo clippy --all-targets --all-features`, `cargo test` [verified, exit 0].

Four findings.

### 1. BLOCKING. Task 7.3 is checked for coverage that does not exist

`tasks.md:87` claims the 7.3 tests include "no default shown after deleting the connection that was the default". `default-connection/spec.md:73` states it as a scenario ("Deleting the default connection ... they land on `/connections` and the control shows no default, without a page reload").

No test exercises it. `ConnectionsList.test.tsx` mentions delete exactly once, at line 141, asserting a row offers no Delete button. `ConnectionForm.test.tsx:738` deletes a connection but renders `/connections` as a stub `<div data-testid="connections-page">`, so no default-connection control is on screen to assert. The other seven clauses of 7.3 are genuinely covered; this one is a checked box nothing can show, which `AGENTS.md` ("A claim you did not earn is worse than an admitted gap") forbids.

### 2. BLOCKING. Task 8.4 is checked on three tests that cannot fail

`tasks.md:118` names "a failed write changing nothing" and "a default write not evicting the connections list" among 8.4's coverage.

- `Connect.test.tsx:932` "failed write changes nothing and Connect proceeds on cached answers" invokes no mutation at all. It renders Connect, unmounts, re-renders against the same fetch stub, and finds "Drill". It passes identically whether or not a failed write evicts, and would pass if `onError` eviction were added.
- `Connect.test.tsx:950` "default write does not evict connections list" never calls `useSetDefaultConnection`; the test body itself does `qc.removeQueries({ queryKey: ["settings"] })`. The hook-level test at `connectors.test.ts:190` asserts only that `["settings"]` *was* removed, never that `["connections"]` was not. So adding `qc.removeQueries({ queryKey: ["connections"] })` to `useSetDefaultConnection` (`connectors.ts:101-111`) breaks no test in the suite, even though `design.md` justifies the per-write mapping precisely so "the freshness rule stays provable".
- `Connect.test.tsx:987` "deleted connection leaves no held schema" calls `removeQueries(["connector-schema","c1"])` on a `QueryClient` that never held that key, then asserts it is undefined. Vacuous. The real behaviour is covered at `connectors.test.ts:104`, so this is redundancy dressed as coverage rather than a hole, but the assertion as written proves nothing.

For contrast, 8.3 is done right: `MutationBridge` (`Connect.test.tsx:190-227`) drives the real hooks, so the gate and its `mutationKey` are actually exercised. 8.4 should be held to the same standard or the box unchecked with the gap stated.

### 3. Non-blocking. `useClearDefaultConnection`'s `mutationKey` is asserted by nothing

`connectors.test.ts:222` is named "...removes settings **and carries mutationKey**" but contains no assertion on the mutation key. `MutationBridge` wires save, delete and set-default, not clear. Deleting `mutationKey: ["connection"]` from `connectors.ts:116` therefore breaks no test, and the Connect gate would silently stop firing for a default *clear*. `design.md` calls out this exact failure mode ("A `mutationKey` typo makes the gate silently never fire"). The same naming problem applies to the save/delete/set-default tests, but those keys are covered transitively by 8.3.

### 4. Non-blocking. Connect can request the schema of a connection the settled write removed

`Connect.tsx:97-104` leaves the connection picker enabled and populated from `connections ?? []` while `isWaiting` is true, and that list is the pre-write copy (nothing is evicted until the write succeeds). An operator who picks there during a pending delete sets `selectedConnectionId` to the doomed id.

When the delete succeeds, the hook evicts first and the mutation then settles, so `useIsMutating` drops to 0 and re-renders Connect before the evicted `["connections"]` query has refetched. On that render `connections` is `undefined` (the observer rebuilds a fresh query via `getOptimisticResult` → `QueryCache.build`), so the guard at `Connect.tsx:63` is skipped, `connectionId` is the deleted id, and `useConnectorSchema` at `Connect.tsx:71` issues `GET /api/connections/{deleted}/schema`. `connections/spec.md:406` and `:424` both say no such request is made.

Self-correcting once the list lands, and the spec's scenario does not have the operator touching the picker mid-wait, so this is a narrow variant rather than a direct scenario failure. Disabling the picker while `isWaiting`, or widening the guard at `Connect.tsx:63` to clear a selection whenever the list is not currently known, would close it.

VERDICT: REVISE
