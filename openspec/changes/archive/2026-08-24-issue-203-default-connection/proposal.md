## Why

Implements [#203](https://github.com/pfa230/labeler/issues/203).

The Connect page opens on nothing. `connectionId` starts as `""` (`ui/src/pages/Connect.tsx:30`) and
every section below the picker is gated on it (`:52`, `:56`, `:60`, `:70`), so the first thing an
operator sees is a lone "choose a connection" dropdown and empty space. Most installations have one
enabled connection, so that first click carries no information: it has exactly one possible answer,
and the operator pays it on every visit.

## What Changes

- A new known application setting, **`default_connection_id`**, holds the connection the Connect page
  opens on. It is instance-wide, like every other key in `app_settings`, and its in-code default is
  "none".
- The Connect page resolves an initial connection instead of starting empty: the stored setting when
  it names an enabled connection, otherwise the first enabled connection in the list the picker
  already builds. The listed order becomes total (see below), so that fallback is deterministic
  across visits rather than a reshuffle. The resolution is latched: it happens once per page load, and
  a later refetch never moves an operator mid-task.
- Opening Connect with a resolved connection therefore loads its schema and its first page of rows,
  because mounting `ConnectorBrowser` already browses on mount (`ConnectorBrowser.tsx:209`). The page
  hits the upstream connector on open. This is the accepted cost of landing ready to work; no gate is
  added.
- Settings > Connections gains a control that sets and clears the default, listing the connections it
  can name rather than asking an admin to type an id.
- `DELETE /api/connections/{id}` clears the setting when it named the deleted connection, in one
  atomic operation with the delete, following the cascade `remove_favorites_for_template` already
  performs for template deletes (`src/store.rs:~370`). A merely *disabled* connection keeps the
  setting and falls through to the fallback while it stays disabled.
- `GET /api/connections` orders by `name`, ties broken by `id`. Nothing makes a connection name
  unique (`src/store.rs:143-151`), so ordering by name alone leaves the "first enabled connection"
  fallback free to reshuffle between visits, which #203 named as worse than no fallback.
- Zero enabled connections keeps today's behaviour exactly: the picker, and nothing below it.
- Changing the picker by hand keeps working as it does now, including the selection reset at `:46`.
  Selecting by hand does not write the setting.

Not in this change: a default for the **template** picker directly below the connection one. Its two
ready-made sources (`/api/recent-templates`, `/api/favorites`) are per user, which is a different
preference model from the instance-wide one chosen here, and a wrong template default also mounts the
composer and builds a field mapping. It is filed as
[#208](https://github.com/pfa230/labeler/issues/208) and is not in this change's scope.

No behaviour is removed, and no existing request or response shape changes, so nothing here is
breaking.

Nothing in the product covers this today. The Connect page keeps no memory of the last connection
used; `localStorage` holds only per-connection browse-column choices (`connectorColumns.ts`), and the
`recent-templates` and `favorites` mechanisms that could supply a default are keyed to templates, not
connections. `app_settings` is the only place instance-wide configuration already lives, so this adds
a key to it rather than a store.

## Capabilities

### New Capabilities

- `default-connection`: the `default_connection_id` setting, how the Connect page resolves the
  connection it opens on, the fallback and its ordering guarantee, what happens when the stored id
  dangles, and the Settings control that sets and clears it. First touch on the Connect page's
  selection behaviour, which today is described only in the frozen `docs/SPEC.md` §12 ("Using a
  connection (UI)"), so the requirement states the complete post-change contract.

### Modified Capabilities

- `connections`: **Deleting a connection** additionally clears `default_connection_id`, atomically,
  when it named the deleted connection. **Connection record** gains a tie-break on `id` so the listed
  order is total.

## Impact

- `src/settings.rs`: a `DEFAULT_CONNECTION_ID` key, its arm in `is_known` and `validate`, and a
  resolver returning `Option<String>` that treats blank stored text as corrupt and a dangling id as
  valid.
- `src/api.rs`: `get_settings` reports the new key; `put_setting` rejects an id that names no
  connection and reflects the canonical (trimmed) value back; `delete_connection_h` uses the
  transactional delete.
- `src/store.rs`: a transactional delete covering the connection row and the matching setting row, and
  a total order on `list_connections`.
- `src/openapi.rs`: unchanged. It registers the generic `SettingValue` and `ResolvedSetting` schemas
  and enumerates no setting keys, so a new key needs no registration.
- `ui/src/pages/Connect.tsx`: the latched connection resolution, replacing the `useState("")` seed.
- `ui/src/pages/settings/ConnectionsSection.tsx`: the default-connection control.
- `ui/src/api/connectors.ts`: the delete-connection mutation also invalidates the settings query, now
  that a delete can clear the setting server-side.
- `ui/src/api/queries.ts`: the settings query is now read by Connect, not only by Settings.
- `docs/adr/0066-*.md` and its row in `docs/adr/README.md`.
- Upstream connectors now receive a browse request when an operator opens Connect, where previously
  they received one only after a click.
