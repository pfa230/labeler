TREE_SHA256: 8514e877bfa580854dcf7fef2f2092b2e3f20c11fbe186dbaeb5f19b120ba631
SPECS_SHA256: 8e05c93ec646b33bec3879affb23ebe8e85196be5d074283f371426e01e26d4f

## Review: issue-384 implementation diff

**Gates, all run here, all green** [verified]: `npm run lint` exit 0; `npx vitest run` 51 files / 566 tests passed, exit 0; `npm run build` exit 0; `cargo fmt --check` exit 0; `cargo clippy --all-targets --all-features` exit 0; `cargo test` exit 0. Tasks 9.1/9.2 are earned.

**Conformance checks that came back clean** [verified]:

- `openspec validate issue-384-... --strict` reports the change valid; the four `REMOVED` requirement headings match published ones at `openspec/specs/connections/spec.md:273,374`, `default-connection/spec.md:76,246`, `connector-field-transforms/spec.md:811`, and no requirement outside those removed blocks references them or the **Manage connections** block.
- No `ConnectionsSection`, `refreshToken` or window-event consumer survives in `ui/src/` outside `connectors.test.ts:131-164`, which asserts the events are no longer dispatched.
- The eviction map matches the delta exactly (`connectors.ts:80-82,94-96,108,119`): create → `["connections"]`; update → plus `["connector-schema", id]`; delete → plus `["settings"]`; default write/clear → `["settings"]` alone; a failed write evicts nothing. No key-prefix collisions exist across the app's query keys.
- `useSetDefaultConnection` sends the same `PUT /settings/default_connection_id` with `{ value }` as `useUpdateSetting` (`api/queries.ts:211`), and `Settings.tsx`/`settings/SettingsSection.tsx` write no `default_connection_id`, so `/connections` is still the only writer.
- The origin guard (`ConnectionForm.tsx:20-37`) now resolves the value and compares origins rather than denylisting prefixes, which closes the `//host`, `/\host` and tab/newline classes that rounds 2 and 3 blocked on.
- All four findings of `diff-review-4.md` are addressed: the shell falls back to held list data on a failed background refetch (`ConnectionForm.tsx:630-634`, test at `ConnectionForm.test.tsx:763`), the return-path tests now click **Edit** on a real `ConnectionsList` (`:961,973`), and the `id === "new"` dead branch is gone.
- The rule editor, its preview, its suspension rules and the create-form note are carried over unchanged from `HEAD:ui/src/pages/connect/ConnectionsSection.tsx`; the list page and the default-connection control are a faithful port.

Three findings, none blocking.

### 1. Non-blocking. Two buttons labelled **Cancel** sit in the same footer bar during the delete confirmation

`ConnectionForm.tsx:583` is the form's **Cancel** (navigates to the origin) and `:597-604` is the delete confirmation's **Cancel** (abandons the confirm step). While `confirmingDelete` is true both render in the one `flex ... justify-between` row at `:580`, so the operator sees `Save | Cancel ....... Confirm | Cancel`, with the two Cancels differing only by position and colour.

The test has to disambiguate by index — `const cancelButtons = screen.getAllByRole("button", { name: /^cancel$/i }); fireEvent.click(cancelButtons[1]);` with the comment "the second Cancel button on page" (`ConnectionForm.test.tsx:823-824`) — which is the tell. Under the old layout the pair lived inside a table row (`HEAD:ConnectionsSection.tsx:553-566`) where the row was the context; task 4.5 moved it "unchanged" into a footer that already had a Cancel, and nothing in the delta or the design considered the collision. On the one change whose stated deliverable is a form a reader can parse, this is the kind of thing the cited guidance is about. **Naming the second one Keep** (or **Don't delete**) costs a word.

### 2. Non-blocking. A failed settings refetch after a successful default write makes the control assert "no default"

`ConnectionsList.tsx:14` takes only `data` from `useSettings()`, and `:23-25` derives `isDefault = storedDefault?.is_default ?? true`, so an absent answer is read as "no default is stored" and `:110` selects the `(no default)` option.

Before this change the write invalidated `["settings"]`, so the held copy stayed on screen through the refetch. Now `connectors.ts:108` removes it, leaving nothing to serve. Failure scenario: the operator picks `c1` in the control, the `PUT` succeeds, and the refetch that follows the eviction fails (or merely takes a moment). The select shows `(no default)` selected, enabled, with `connectionsKnown` still true — the exact opposite of what was just stored, and a state that reads as "the write did not take". On a transient failure it stays there until the page is reloaded. `default-connection/spec.md` scenario "Choosing a default" requires "the control shows that connection as the stored default"; `ConnectionsList.test.tsx:203-232` asserts it only after `waitFor`, so the transient window and the failure case are both uncovered.

The same reasoning the spec already applies to the connections list — "while the list is still loading or has failed, the control SHALL NOT report a stored default as unavailable, because neither says the connection is gone" — applies to the settings read and is not implemented for it. `diff-review-3.md` raised the transient half; the failed-refetch half is a stronger consequence of the same line. Handling `useSettings().isError`/`isPending` in the control the way `connectionsKnown` handles the list would close both.

### 3. Low. A test name claims something its body no longer shows

`connectors.test.ts:169` is still named "a save while the first schema read is still in flight **starts a fresh read** and ignores the abandoned response", but the body now calls `schemaResult.current.refetch()` by hand at `:228` before asserting `schemaCallCount === 2`. Under the new eviction model the save removes the query and starts nothing; the test starts the read. What survives is the "abandoned response is ignored" half, which is real and worth keeping. The name should say so, or a reader takes the fresh read as proven when nothing proves it.

---

Nothing here changes behavior the delta requires, and the implementation matches `proposal.md`, `design.md`, the three spec deltas and issue #384's acceptance criteria on every clause I traced.

VERDICT: APPROVE
