TREE_SHA256: f0d5bf55e3f1ffd9a3787e50efaa9bfea9a66690dcda9c6602613b0c99bac79c
SPECS_SHA256: 8e05c93ec646b33bec3879affb23ebe8e85196be5d074283f371426e01e26d4f

## Review: issue-384 implementation diff

**State of the tree.** UI gates pass on the tree as it stands [verified]: `npm run lint` clean, `npx vitest run` 51 files / 561 tests exit 0, `npm run build` exit 0. `openspec validate issue-384-... --strict` reports the change valid [verified]. The diff touches no Rust, so the cargo gates are unaffected. No reference to `ConnectionsSection`, `refreshToken`, or either window event survives outside the test asserting the events are gone [verified by grep over `ui/src/`]. Every cross-reference to the four removed requirements lives inside a removed block [verified]. The structure matches `design.md`: three routes, the nav item, the list/form split, the `location.key`-keyed editor subtree, per-write eviction, four `mutationKey: ["connection"]` mutations, the `useIsMutating` gate, the origin relay. All four findings of `diff-review-2.md` are addressed.

Six findings.

### 1. BLOCKING. The origin guard still admits an off-site redirect, now via a tab or newline

`ui/src/pages/connections/ConnectionForm.tsx:20-31` honours any `from` that begins with `/` and begins with neither `//` nor `/\`. A tab, LF or CR as the second character passes all three tests and still resolves cross-origin, because the WHATWG URL parser strips those before parsing:

```
"/\t/evil.com" -> https://evil.com/     [verified, node, base https://app.example/connections/c1]
"/\n/evil.com" -> https://evil.com/
"/\r/evil.com" -> https://evil.com/
```

Failure scenario: `from = "/\t/evil.com"`, the operator saves or cancels, `navigate("/\t/evil.com")` reaches `history.push`, which calls `globalHistory.pushState(state, "", "/\t/evil.com")`. That throws `SecurityError` for a cross-origin result and the `catch` falls through to `window.location.assign(url)` (`ui/node_modules/react-router/dist/development/chunk-62JRHF6Z.mjs:309-317` [verified]), landing the operator on `evil.com`.

`openspec/changes/issue-384-.../specs/connections/spec.md:240-243` states the rule and its purpose in one sentence: honoured only when it begins with a single `/`, "so a value that reached the state from outside the application cannot decide where the operator lands". Read literally, `"/\t/evil.com"` does begin with a single `/` and the code complies. But `"/\evil.com"` did too, and this change's own previous round treated the purpose clause as binding and fixed it. Applying that reading consistently, the guard fails its stated purpose again.

The reason it fails twice is that the fix chosen was a third literal prefix in a denylist rather than a check on what the value resolves to. `new URL(from, window.location.origin)` compared against `window.location.origin` closes the class; a prefix list will not.

Exploitability is low, since router state can only be set by same-origin script. That is a mitigation, not the guarantee the requirement states.

### 2. Non-blocking. The 8.4 test named "browses new rows" asserts nothing about rows

`ui/src/pages/Connect.test.tsx:926-948`. It renders Connect, finds "Drill", unmounts, points the stub at a connection whose `base_url` is `http://hb-updated`, calls `queryClient.removeQueries` by hand for `["connections"]` and `["connector-schema","c1"]`, re-renders, and asserts one thing: `picker.value === "c1"`. The stub's browse handler ignores `base_url` entirely (`Connect.test.tsx:79`), so nothing distinguishes the old upstream from the new one. The spec scenario it stands for, "A save reaches Connect on the next visit → the rows shown are browsed from the new upstream", is unverified, and `tasks.md:119` is checked over it. It also simulates the write with `removeQueries` instead of running one, so it cannot catch a wrong eviction mapping either.

### 3. Non-blocking. "No pre-write answer being presented" is checked and untested, and the test that covered it was deleted

`tasks.md:120` lists "no pre-write answer being presented" among 8.4's cases; the describe block holds five tests and none of them is that one. The nearest, the credential-only save (`Connect.test.tsx:950`), asserts that a request was made after the write, not that the copy read before it is never presented.

Meanwhile `ui/src/api/connectors.test.ts` loses "a save while the first schema read is still in flight starts a fresh read and ignores the abandoned response" (present at `HEAD`), which was exactly the pre-write-copy race for the schema. The behavior survives: `Query.destroy()` calls `cancel({ silent: true })` (`@tanstack/query-core@5.101.4`, `build/modern/query.js:86-89` [verified]), so `removeQueries` aborts the in-flight read the old code cancelled explicitly. So this is coverage lost, not a regression, but the scenario at `specs/connections/spec.md:383-389` now rests on library behavior nothing pins.

### 4. Non-blocking. The form's titled sections, the point of the issue, are asserted by nothing

`specs/connections/spec.md:93-94` requires the form to render "as a single-column page form in titled sections: **Details**, then **Field transforms**", and the deep-link scenario at `:142-147` restates it. `grep -n "Details" ui/src/pages/connections/ConnectionForm.test.tsx` returns nothing [verified], and the cold-deep-link test (`ConnectionForm.test.tsx:686-692`) asserts the page heading and two field values only. The same holds for `:96-97`, "**connector**, which is fixed at creation and SHALL NOT be editable": no test asserts the control is disabled. Deleting both `<h2>`s and the `disabled` attribute leaves the suite green, on the one requirement that is the issue's actual deliverable.

### 5. Non-blocking. The default-connection control shows "(no default)" immediately after a successful write

`ui/src/api/connectors.ts:99-110` evicts `["settings"]` on success, so `ConnectionsList`'s `useSettings()` holds no answer while the refetch is in flight, and `ui/src/pages/connections/ConnectionsList.tsx:23-25,110` falls back to `isDefault = true` and `value = ""`. The select therefore snaps to "(no default)" for the duration of the refetch, immediately after the operator picked a connection, where the previous `invalidateQueries` kept the value on screen. `specs/default-connection/spec.md:34-44` requires "the control shows that connection as the stored default" and "the control shows no stored default"; both are satisfied only eventually, and `ConnectionsList.test.tsx:203-231` asserts only that the `PUT` and the `DELETE` were sent, so neither **AND** clause is covered. `design.md:238-242` reasons about this trade for the list after a delete but not for the control after its own write, where the old rows were not the defect.

### 6. Low. Connect presents pre-write answers while it reports that it is waiting

`ui/src/pages/Connect.tsx:81-96` renders "Waiting..." alongside the empty-list call to action and the load-failure message, both computed from the `["connections"]` copy held before the in-flight write. Concretely: an operator whose installation has no connections follows the Connect call to action, presses **Save**, then **Cancel** before the response, and lands on a page showing "Waiting..." and "No connections configured." together, from a copy the create is about to falsify. The spec's enumeration at `specs/connections/spec.md:351-353` covers resolving, reading a schema and browsing rows, so the literal text is met; the gap is between the gate's stated purpose ("what Connect presents must agree with every write this browser has completed") and its extent.

---

Finding 1 is the one that must be fixed: the guard the requirement's own sentence justifies does not do what that sentence says, for the second time, and the shape of the previous fix is why.

VERDICT: REVISE
