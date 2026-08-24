## 1. Store

- [x] 1.1 Change `list_connections` to `ORDER BY name, id` (`src/store.rs:693`), and add a store test
      proving two connections sharing a name list in id order on every call.
- [x] 1.2 Add `delete_connection_and_default(id) -> Result<bool, StoreError>`: one `rusqlite`
      transaction deleting the connection row, then the `app_settings` row where
      `key = 'default_connection_id' AND value = ?1`, committing once. Returns whether the connection
      existed. Leave `delete_connection` untouched.
- [x] 1.3 Store tests for 1.2: deleting the default clears the setting; deleting a non-default leaves
      it; a delete of an unknown id returns `false` and clears nothing.

## 2. Settings

- [x] 2.1 Add `DEFAULT_CONNECTION_ID` to `src/settings.rs`, its arm in `is_known`, and its arm in
      `validate`: accept a JSON string, trim it, reject empty-after-trim with a message naming the key.
- [x] 2.2 Add `resolve_default_connection_id_from(Option<String>) -> Result<Option<String>, SettingError>`:
      `None` -> `None`; blank or whitespace-only stored text -> `SettingError::Corrupt`; anything else
      returned as stored.
- [x] 2.3 Unit tests for 2.1 and 2.2, including that a well-formed dangling id resolves rather than
      erroring, and that blank stored text is `Corrupt`.

## 3. API

- [x] 3.1 Report `default_connection_id` from `get_settings` (`src/api.rs:1131`), as `null` /
      `is_default: true` with no override.
- [x] 3.2 In `put_setting`, move the connection-existence check for this key *inside* `state.write_lock`,
      immediately before `set_setting`, and map a miss to `Reason::SettingValueInvalid`.
- [x] 3.3 Reflect the canonical (trimmed) string back for this key instead of falling through to
      `body.value` (`src/api.rs:1204`). Do not change the reflection for any other key.
- [x] 3.4 Point `delete_connection_h` at `delete_connection_and_default`, keeping its `404` behavior.
- [x] 3.5 HTTP tests in `src/lib.rs`'s `mod tests`: PUT with `{ "value": "  id  " }` stores and reflects
      the trimmed id; PUT with an unknown id, `""`, `"   "`, `null`, a number and an object each give
      `400` / `setting_value_invalid`; PUT accepts a disabled connection's id; DELETE resets to
      `null` / `is_default: true`; GET reports a dangling stored id without erroring; deleting the
      default connection clears the setting and deleting a different one does not.

## 4. Connect page

- [x] 4.1 Replace the `useState("")` seed in `ui/src/pages/Connect.tsx:30` with `string | null` plus a
      latched resolution: resolve once, when `useConnections()` and `useSettings()` have both answered,
      via a render-phase `setState`; effective id is the operator's choice when non-null, else the latch.
- [x] 4.2 Implement the resolution order: stored default when it names an enabled connection, else the
      first enabled connection in list order, else nothing. A settings *error* resolves as no default
      stored; a connections error resolves nothing.
- [x] 4.3 Verify that a change of effective connection resets the row selection, the browse table and
      the composer together, and that the only path to a change is the picker's `onChange`.
- [x] 4.4 Tests in `Connect.test.tsx`: stored default selected on open; fallback when nothing stored;
      fallback when the stored default is disabled; fallback when it names no connection; nothing
      selected when no connection is enabled; fallback when the settings query errors; equal-name
      connections resolve in id order; a refetch that changes the stored default does not move the
      selection or drop the row selection; a manual pick clears the selection and writes no setting.

## 5. Settings > Connections control

- [x] 5.1 Add the default-connection control to `ui/src/pages/settings/ConnectionsSection.tsx`: every
      connection by name plus its id, disabled ones marked, a "no default" choice, and a line saying
      the default applies to everyone.
- [x] 5.2 Render an explicit unavailable state naming the stored id when it matches no connection, still
      clearable.
- [x] 5.3 Add `["settings"]` to the invalidations in `useDeleteConnection` (`ui/src/api/connectors.ts:74`).
- [x] 5.4 Tests in `ConnectionsSection.test.tsx`: picking sends `PUT` with the id; "no default" sends
      `DELETE`; no stored default shows the "no default" choice; a disabled connection is marked;
      two identically named connections are distinguishable; a dangling stored id shows the unavailable
      state and can still be cleared; deleting the default connection leaves the control showing no
      default without a reload.

## 6. Documentation

- [x] 6.1 Write `docs/adr/0069-connect-opens-on-a-default-connection.md` (Nygard, Status: Accepted),
      recording the instance-wide choice against the per-user alternatives, the latch, the atomic
      cascade, and the accepted upstream request on page open.
- [x] 6.2 Add the ADR-0069 row to `docs/adr/README.md`.

## 7. Verification

- [x] 7.1 Before each new test's implementation lands, run it against the unchanged tree and confirm it
      fails for the stated reason. A test that passes red is not a test.
- [x] 7.2 Run `cargo fmt`, `cargo clippy --all-targets --all-features`, and `cargo test`; fix root
      causes, never `#[allow]`.
- [x] 7.3 Run `npm --prefix ui run lint` and `npm --prefix ui run test`.
- [x] 7.4 Exercise it by hand against a running server (`LABELER_CONFIG_DIR=./config-dev cargo run`,
      `LABELER_NO_AUTH=true`): open Connect with no default and confirm the fallback loads rows without
      a click, set a default in Settings, reopen Connect on it, then delete that connection and confirm
      the control and the page both fall back.
