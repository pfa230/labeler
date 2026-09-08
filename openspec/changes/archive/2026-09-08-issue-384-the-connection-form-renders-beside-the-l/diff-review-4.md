TREE_SHA256: 0bfca88e9c28400c4f5291bf2ed1ba884f3fb1e6dc2b216df5b29537eacb2b56
SPECS_SHA256: 8e05c93ec646b33bec3879affb23ebe8e85196be5d074283f371426e01e26d4f

## Review: issue-384 implementation diff

**Gates, all run here, all green** [verified]: `ui` lint exit 0; `vitest run` 51 files / 565 tests passed; `ui` build exit 0; `cargo fmt --check` OK; `cargo clippy --all-targets --all-features` exit 0; `cargo test` 902+2+1 passed, 0 failed. So task 9.1/9.2 are earned.

**Conformance checks that came back clean**: the four `REMOVED` requirement names match published ones in `openspec/specs/{connections,default-connection,connector-field-transforms}/spec.md`; the rule editor, its preview, its suspension rules and the create-form note are carried over unchanged from `ConnectionsSection.tsx` (compared line by line against `git show HEAD:`); `useSetDefaultConnection` sends the same `{ value }` body as `useUpdateSetting` (`api/queries.ts:211`); the origin guard now rejects `//host`, `/\host` and tab/newline evasion, closing diff-review-3's blocker; the four mutation keys and the per-write eviction map match the delta exactly; no leftover `ConnectionsSection`, `refreshToken` or window-event consumer remains.

---

### 1. BLOCKING. A failed background refetch of the connections list destroys the open form's unsaved draft

`ConnectionForm.tsx:630-635` decides loading, failure and not-found from `useConnections()` on **every** render, not just before the entry has resolved a connection. TanStack v5 keeps `data` and sets `status: "error"` on a failed *background* refetch, so `isError` goes true while the form is on screen, the shell returns `<p>Failed to load connections.</p>`, and `<EditConnectionForm>` unmounts with every draft field inside it.

The app's client is `new QueryClient()` with no overrides (`ui/src/main.tsx:9`), so `refetchOnWindowFocus: true` and `staleTime: 0`. Tabbing away and back while the server hiccups is enough.

Failure scenario, run against this tree in an out-of-repo harness [verified]: open `/connections/c1`, type into **name**, let `GET /api/connections` return 500 on the next refetch. The body becomes exactly `Failed to load connections.` and the draft, including a typed **api key** and any edited transform rules, is gone. It does not come back when the list recovers; the subtree remounts from stored values.

This is a regression from the code being replaced: `ConnectionsSection.tsx:580` held the editing target in `useState` and rendered the form at `:629-631` unconditionally, so a list refetch could not touch it. `Connect.tsx:60-62` already carries the correct guard for this exact situation (`connections && !connectionsFailed ? ... : effectiveId`), and the spec scenario "A failed connections refetch does not clear the selection" pins it there. The form shell has no equivalent.

Nothing in the delta sanctions it. "A list that failed to load SHALL report that failure rather than the id being unknown" is a rule for deciding not-found on load; "Each entry into the form is its own editor" says drafts belong to the entry, and a network blip is not the operator leaving it.

Fix shape: resolve the connection once for the entry and keep rendering the editor from it, or fall back to held list data when `isError` and `data !== undefined`, the way Connect does.

### 2. Non-blocking. The "by way of the list" return-path tests never go by way of the list

`ConnectionForm.test.tsx:902` and `:910` are named for the spec scenarios "Saving/Cancelling a form reached from Connect by way of the list", but they mount `ConnectionForm` directly with `state: { from: "/connect" }`; `ConnectionsList` is not in the tree. The list's relay onto **Edit** (`ConnectionsList.tsx:89-96`) is checked only by `href` at `ConnectionsList.test.tsx:378`; the click-and-assert-state check at `:384-385` covers **Add connection** alone. Delete the `state` prop from the Edit link and the whole suite still passes, while the spec's stated path `/connect` → `/connections` → `/connections/{id}` → save quietly returns to `/connections`.

The code is correct. What is not earned is task 7.6's claim to cover that path, and task 3.5's claim for the Edit half of the relay. One composed test clicking **Edit** from a list reached with an origin would close both.

### 3. Low. Dead branch in the route shell

`ConnectionForm.tsx:627` reads `id === undefined || id === "new"`. React Router v6 ranks a static segment above a dynamic one, so `/connections/new` always matches the static route at `App.tsx:38` and `id` is never the string `"new"` there. The `?? "new"` in the key at `:649` and `:657` is the same condition again. Harmless, but it invites a reader to think both routes reach `:id`.

---

Finding 1 loses operator-entered credentials on an ordinary transient error and regresses behavior the diff replaces, so it must be fixed before this lands.

VERDICT: REVISE
