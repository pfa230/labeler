TREE_SHA256: b56b307538b368d1ddcda0c7ff810185b75bc423a670c2993699e1fafad7e436
SPECS_SHA256: 8e05c93ec646b33bec3879affb23ebe8e85196be5d074283f371426e01e26d4f

## Review: issue-384 implementation diff

**State of the tree.** All gates pass [verified]: `npm run lint` (clean), `npx vitest run` (51 files, 558 tests, exit 0), `npm run build` (exit 0), `cargo fmt --check` / `clippy --all-targets --all-features` / `cargo test` (exit 0). `openspec validate issue-384-... --strict` reports the change valid, and every `REMOVED Requirement` heading matches a published one exactly [verified]. No reference to `ConnectionsSection`, `refreshToken`, or either window event survives outside the test that asserts the events are no longer dispatched (`connectors.test.ts:123-156`) [verified].

The structure matches `design.md`: three routes, nav item, list/form split, keyed editor subtree, per-write eviction, four `mutationKey: ["connection"]` mutations, the `useIsMutating` gate, the origin relay. The rule editor and preview moved byte-for-byte apart from the container and the `onClose` → `navigate` swap [verified by diff against `HEAD:ui/src/pages/connect/ConnectionsSection.tsx`]. All four findings of `diff-review-1.md` are addressed: the delete-the-default test now exists (`ConnectionsList.test.tsx:308`), the three 8.4 tests now drive real mutations (`Connect.test.tsx:986,1040,1095`), `useClearDefaultConnection`'s `mutationKey` is asserted (`connectors.test.ts:249`), and the picker is disabled while waiting (`Connect.tsx:103`) [verified].

Four findings.

### 1. BLOCKING. The origin guard admits `/\host`, which is an off-site redirect

`ui/src/pages/connections/ConnectionForm.tsx:22` honours any `from` that begins with `/` and does not begin with `//`. `design.md` and `connections/spec.md` ("Where a connection form returns to") both justify that test as the thing that "keeps a value that reached the state from outside the application ... from deciding where the operator lands". It does not.

Failure scenario: `from = "/\evil.com"` passes the guard, so a save or a cancel calls `navigate("/\evil.com")`. `new URL("/\\evil.com", "https://app.example").href` is `https://evil.com/` [verified by running it], because WHATWG URL parsing treats `\` as `/` for special schemes. React Router 7.18.2's `BrowserHistory.push` calls `globalHistory.pushState(state, "", "/\evil.com")`, which throws `SecurityError` for a cross-origin result, and the `catch` falls through to `window.location.assign(url)` (`ui/node_modules/react-router/dist/development/chunk-62JRHF6Z.mjs:310-317`) [verified]. The operator lands on `evil.com`.

`ConnectionForm.test.tsx:902` covers `//evil.com` and nothing covers `/\`. Exploitability is low, since setting router state needs same-origin script, but that is a mitigation, not the guarantee the spec states. The fix is to reject a second character of `/` or `\` and add the case beside the existing one.

### 2. Non-blocking. The "naming a default" test cannot show the half the scenario is about

`default-connection/spec.md` scenario "Naming a default and going straight to Connect" requires the page to resolve "on the newly named default, **not on the one stored before it**". `Connect.test.tsx:830-873` stubs `GET /api/settings` to return `{ value: "c2" }` unconditionally (`:844-846`), so the pre-write and post-write answers are identical. Deleting `qc.removeQueries({ queryKey: ["settings"] })` from `useSetDefaultConnection` (`connectors.ts:107-109`) leaves this test green; only the waiting half can fail. The eviction itself is covered elsewhere (`connectors.test.ts:220`, `Connect.test.tsx:1040`), so this is a weak assertion rather than a hole. Making the settings stub return `c1` before the `PUT` and `c2` after, as `Connect.test.tsx:1095` already does for the delete, closes it.

### 3. Non-blocking. `location.key` in the editor key is load-bearing and unexercised

`ConnectionForm.tsx:638,646` key the draft-owning subtree by `` `${location.key}:${id ?? "new"}` ``. `design.md` states plainly that `location.key` is what makes two entries for the *same* id a fresh editor, "which the id alone would not". The only test of editor identity (`ConnectionForm.test.tsx:776-833`) goes `c2` → `c1` → `c2`, and the id changes at every step, so `` key={id} `` alone would pass it. `NavigationTester` never fires two consecutive navigations to the same id. A regression that drops `${location.key}:` would ship silently.

### 4. Low. Nav order is required by the spec and asserted by nothing

`connections/spec.md` requires a **Connections** item "directly after **Connect**" and its scenario says it "follows the **Connect** item". `Shell.test.tsx:48-52` iterates a label list and asserts presence only, so reordering `NAV_ITEMS` (`Shell.tsx:7-12`) breaks no test.

Finding 1 is the one that must be fixed: the code does not do what the requirement it implements says it does.

VERDICT: REVISE
