## Context

See `proposal.md` for motivation and `specs/` for the contract. What shapes the approach:

- `app_settings` is a flat instance-wide key/value table (`store.rs:307`). Settings are *resolved*:
  `GET /api/settings` reports every known key with its effective value and `is_default`, and a row
  exists only for an override (frozen `docs/SPEC.md` §12 "Settings", ADR-0024). ADR-0024 also fixes
  that a corrupt stored override surfaces an error rather than silently becoming a default
  (`docs/adr/0024-app-settings-storage-and-api.md:27`).
- `settings::validate(key, &Value) -> Result<String, String>` (`src/settings.rs:62`) is pure and
  synchronous. It has no store handle, so it cannot ask whether a connection exists.
- `PUT /settings/{key}` takes `SettingValue { value: serde_json::Value }` (`api.rs:1121`), so the body
  is `{ "value": ... }`, never a bare JSON scalar.
- `put_setting` (`api.rs:1191`) validates, *then* takes `state.write_lock`, writes, and reflects a
  value back by `canonical.parse::<u32>()` with `body.value` as the fallback (`api.rs:1204`).
- `get_settings` (`api.rs:1131`) takes no lock at all.
- `delete_connection_h` (`api.rs:1736`) holds `state.write_lock`, but each `Store` method takes the
  connection mutex and executes on its own (`store.rs:317`, `store.rs:757`), so two calls are two
  autocommit statements, not one transaction.
- `connections.name` carries no UNIQUE constraint (`store.rs:143-151`) and `list_connections` orders
  by `name` alone (`store.rs:693`), so today's listed order is not total.
- `Connect.tsx` seeds `useState("")` at `:30`. The row selection lives in that component (`:35`) and
  is cleared only by the picker's `onChange` (`:46`), while `ConnectorBrowser` and `Composer` are
  keyed on the connection and reset by remounting (`:70`, `:82`).
- Mounting `ConnectorBrowser` browses immediately: the effect at `ConnectorBrowser.tsx:209` fires on
  `[connectionId, resource, applied, parent]` and issues `browseConnection` with no user action.
- `useDeleteConnection` invalidates only `["connections"]` (`ui/src/api/connectors.ts:74-79`).
- Rust HTTP integration tests live in `src/lib.rs`'s `mod tests` behind `build_app_with_state()`;
  there is no `tests/*.rs` for the API.

## Goals / Non-Goals

**Goals:**

- Make the delete cascade one atomic operation, so the spec's "no reader ever sees a dangling
  default" is a property of the code and not a hope.
- Latch the opening resolution, so a background refetch can never move an operator mid-task.
- Keep `settings::validate` pure. Referential validation belongs to the handler that has the store.

**Non-Goals:**

- No change to how any other setting validates, resolves or is reflected back.
- No gate on the browse that fires when a connection resolves. The upstream request on page open is
  accepted, not mitigated.
- No per-user layer. The resolution order is written as an ordered list precisely so one can be
  inserted ahead of the instance setting later without renegotiating this contract.

## Decisions

### Instance-wide `app_settings`, not a per-user store

Chosen against three alternatives: a dedicated per-user table following `favorites.user_id`
(`store.rs:354`), a generic `user_prefs(user_id, key, value)` table with endpoints, and browser
`localStorage` alongside `connectorColumns.ts`. `app_settings` needs no migration, no new endpoint and
no new UI vocabulary: the key joins three that already exist and inherits the resolved-settings
contract whole.

The cost is stated plainly because #203 warned against exactly this choice for exactly this reason:
one default serves everyone. On an installation where two operators habitually work different
connections, one of them is wrong on every visit, and their only recourse is the picker they were
already using. This is the accepted trade-off, not an oversight. The mitigation is structural rather
than behavioural: the Connect page's resolution is specified as an ordered list, so a per-user
preference is a later insertion at position 1 rather than a rewrite.

### The cascade is one transaction, not two statements

A new store method `delete_connection_and_default(id) -> Result<bool>` opens a `rusqlite::Transaction`,
runs `DELETE FROM connections WHERE id = ?1`, then
`DELETE FROM app_settings WHERE key = 'default_connection_id' AND value = ?1`, and commits. It returns
whether the connection row existed, so `delete_connection_h` keeps returning `404` unchanged. A failure
on either statement drops the transaction, rolling both back, and the handler reports the store error
instead of `204`.

This is the fix for the reviewer's first Critical finding. Two separately committed statements leave a
window in which `GET /api/settings`, which takes no lock (`api.rs:1131`), reads a default naming a
connection that `GET /api/connections` no longer lists, and leave a dangling setting outright if the
second statement fails. Rejected: `delete_connection` followed by a conditional `delete_setting`, and
the same pair wrapped in the API-level `write_lock`. The lock serialises writers but not the lock-free
reader, and it cannot roll back a committed first statement.

`delete_connection` is **removed**, not kept. This paragraph originally planned to leave it alone, on
the assumption that its remaining callers did not need the cascade. Implementation showed it had no
production caller at all once `delete_connection_h` moved across: only one store test still used it.
Keeping a public method that deletes a connection *without* clearing the setting would leave the exact
footgun the transaction exists to remove, so the transactional method is now the only way to delete a
connection, and the test moved to it.

### Existence is checked in the handler, shape in `settings::validate`

`validate` gains a `DEFAULT_CONNECTION_ID` arm that accepts a JSON string, trims it, and rejects
anything empty after trimming. It stays pure, so it stays unit-testable next to the other keys.

`put_setting` then checks that the id names a connection, and maps a miss to the same
`Reason::SettingValueInvalid` the shape failure uses. The alternative, giving `validate` a store
handle and making it async, would make every existing key pay for one key's referential check.

**Ordering matters here.** The existence check must happen *inside* `state.write_lock`, immediately
before `set_setting`, not before the lock is taken. Outside it, a concurrent
`DELETE /api/connections/{id}` can land between the check and the write, and the setting is stored
naming a connection that no longer exists, with the cascade already run. Both handlers take the same
lock, so holding it across check-and-write closes the window entirely.

A *disabled* connection is accepted. Disabling is a temporary operational act; clearing an admin's
stored choice because a connection was disabled for an afternoon would lose information the operator
has to re-enter. The Connect page's fallback covers the interval.

### The resolver distinguishes corruption from a dangling id

`resolve_default_connection_id_from(stored: Option<String>) -> Result<Option<String>, SettingError>`
follows the shape of the existing resolvers (`settings.rs:113-170`). `None` resolves to `None`. Stored
text that is empty or whitespace-only is `SettingError::Corrupt`, matching every other key and
ADR-0024's rule that corruption surfaces rather than degrades. A well-formed id is returned as-is
without consulting the connections table: whether it still names a connection is the Connect page's
question, not the settings endpoint's, and the dangling case is deliberately supported.

Corrupt text can only arrive by direct database tampering, since `validate` gates every write. It is
specified for parity, not because a path produces it.

### `put_setting` reflects the canonical value for this key

For `default_connection_id` the canonical text is the trimmed id, and `canonical.parse::<u32>()` fails,
so today's fallback would reflect `body.value`, the *untrimmed* input. The response would then disagree
with what was stored and with what `GET /api/settings` returns next.

The reflection therefore returns the canonical string for this key. `datetime_formats` has the same
latent mismatch today (its canonical form reorders keys, and the reflected `body.value` does not); it
is out of this change's scope and is not touched.

### The listed order of connections becomes total

`list_connections` becomes `ORDER BY name, id`. Without it, two connections sharing a name (nothing
enforces uniqueness: `store.rs:143-151`) have an order SQLite is free to vary, and the fallback
"first enabled connection" would then reshuffle between visits, which is precisely the failure #203
called out as worse than no fallback at all.

The tie-break belongs on the server rather than in the Connect page's fallback, so the picker's
displayed order and the order the fallback reads are the same order. A UI-side tie-break would make
the page choose a connection that is not the first one the operator sees.

This refines an existing requirement, so it goes in as a `MODIFIED` delta on the `connections`
capability's "Connection record" rather than as new behavior stated somewhere else.

### The Connect page latches the resolution; it does not set it in an effect

`connectionId` becomes `string | null`, where `null` means "the operator has not touched the picker"
and `""` means "the operator explicitly chose none". A separate piece of state holds the latched
resolution. During render, when the latch is still empty and both queries have produced an answer, the
resolution is computed and stored with a render-phase `setState`, which React re-renders immediately
and which is the documented way to derive state from data that arrives late. The effective connection
is then the operator's choice when they have made one, else the latch.

Latching is what makes the reviewer's second Critical finding impossible. A derived-every-render value
would be recomputed from live query results, so a window-focus refetch, or another operator changing
the instance-wide setting, could change the effective connection without the picker's `onChange`
running. `ConnectorBrowser` and `Composer` would remount on their `key`, but the row selection lives in
`Connect` (`:35`) and is cleared only by that handler (`:46`), so rows selected against one connection
would survive into another and could be materialized against it. With the latch, the effective
connection changes only through the picker, which already resets the selection.

Rejected: a `useEffect` calling `setConnectionId` once both queries settle. It adds a render with
nothing selected, it fights `react-hooks/set-state-in-effect`, and it stores "what is selected" in two
places. Also rejected: deriving every render, for the reason above.

Resolution reads `useConnections()` and `useSettings()`. `useSettings` already exists
(`ui/src/api/queries.ts:135`) and `/api/settings` is readable by any authenticated user, since this
application has flat accounts and no roles, so no permission work is implied. While either query is
still loading, nothing resolves and the page renders today's empty-picker state for that moment.

A settings *error* resolves as though no default were stored, and the fallback runs. `GET /api/settings`
fails as a whole when any single stored setting is corrupt (`api.rs:1148-1175`), so a corrupt
`datetime_formats` would otherwise strand the Connect page on an empty picker forever. Degrading to the
fallback keeps the page usable and loses only the admin's stored preference for that session. A
connections-list error resolves nothing, because there is then nothing to select.

### Deleting a connection invalidates the settings cache too

`useDeleteConnection` invalidates only `["connections"]` (`ui/src/api/connectors.ts:74`). Since the
server now clears `default_connection_id` in the same operation, the mutation also invalidates
`["settings"]`; otherwise the mounted default-connection control keeps showing the deleted id after the
server has cleared it.

### The Settings control lists every connection, marking the disabled and the missing

The control offers all connections, not only enabled ones, because a disabled connection is a valid
stored default; disabled entries are marked. Entries carry the connection id alongside the name so two
connections sharing a name are distinguishable.

A stored id that names no connection gets its own explicit entry, showing the id and that it is
unavailable, selected so the operator sees the real state and can clear it. Silently showing "no
default" would misreport the stored setting, and silently showing some other connection would be worse.
This state is reachable by a rollback to a build without the cascade, and by direct database edits.

It sits in `ConnectionsSection.tsx`, beside the connections it names, rather than in
`SettingsSection.tsx`, which is a single-key numeric editor and would have to grow a second, differently
typed control to host an id no one should type by hand.

### ADR

This change adds **ADR-0069, "Connect opens on a default connection named by an instance-wide
setting"**, plus its row in `docs/adr/README.md`. It supersedes nothing: no existing ADR states how the
Connect page chooses its connection. ADR-0024 (app settings storage and API) is the context it builds
on and stays accepted.

## Risks / Trade-offs

- **Every Connect page open now hits the upstream connector.** `ConnectorBrowser`'s first browse is an
  imperative call in an effect, not a cached query, so navigating to Connect repeatedly issues a
  request every time where previously it issued none until a click. → Accepted deliberately: the
  point of the change is that the page is ready to work. It is bounded by one page-size request per
  navigation, and a slow or failed upstream renders the browser's existing error state rather than
  breaking the page.
- **One default for every operator.** → Stated above; the ordered resolution list is the seam for a
  per-user layer later.
- **Latching means an admin's change to the default does not reach an open Connect tab.** → Correct
  and intended: the alternative moves an operator mid-task. The next page load picks it up.
- **A connection disabled while it is the default silently stops being used.** Nothing tells the
  admin. → Accepted for this change: the fallback is deterministic and the Settings control shows
  which connection is stored and that it is disabled, which is where an admin looks.
- **The referential check makes `PUT /api/settings/default_connection_id` behave unlike every other
  settings key**, which validates shape only. → Contained: the difference is one arm in one handler,
  it is specified, and the reason code is the one already used for a rejected setting value.
- **Tests that pass against the unfixed code.** The resolution order, the latch, the cascade's
  atomicity and the trimming each have a plausible test that passes before the change exists. → Each
  new test is run against the current tree first and must fail for the stated reason before the
  implementation lands.

## Migration Plan

None. The new key has no stored row until an admin writes one, so an existing installation resolves
the fallback and behaves as if the feature had always been there with no default set.

Rollback is the revert. A `default_connection_id` row left in `app_settings` is inert to a build that
does not know the key: `get_settings` reports only keys it knows and `is_known` gates writes. Rolling
forward again can therefore surface a stored id whose connection was deleted while the older build was
running, which is exactly the dangling case the resolver and the Settings control are specified to
handle.
