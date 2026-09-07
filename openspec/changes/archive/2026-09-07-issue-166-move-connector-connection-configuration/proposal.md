## Why

Implements GitHub issue **#166**.

Connection configuration lives on `/settings` while browsing and printing from a connector lives on
`/connect`. Adding, editing, disabling or deleting a connection therefore interrupts the Connect
workflow with a trip to another page and back, and an installation with no connections at all lands
the operator on a Connect page that is a dead end with no way out of it in place.

## What Changes

- **BREAKING**: `/settings` renders no connections UI. The connections table, the add/edit form and
  the default-connection selector are gone from that page, with no link stub and no redirect. An
  operator who bookmarked Settings for connection work goes to `/connect` instead.
- Connection management moves inline onto `/connect`, as a collapsible **Manage connections** block
  below the connection and template pickers. Everything the section held travels with it: the
  connections table, add/edit/delete, the enabled checkbox, the write-only API-key field, the public
  URL field and the field-transform rule editor.
- The block starts **collapsed**, and starts **expanded** when the connections list has loaded and is
  empty, which is exactly the state in which the page is otherwise a dead end. The state is component
  state and is not persisted across reloads.
- The instance-wide **Default connection** selector moves with the list, because its options *are*
  the connection list.
- Creating or editing a connection updates the Connect page's connection picker without a reload,
  according to what the connection now is: the picker offers the enabled connections, so one saved as
  enabled appears there and one saved as disabled does not. Neither moves the operator's current
  selection. Deleting or disabling the currently-selected connection clears that selection, and the
  rows selected against it, instead of leaving either dangling on a connection the picker can no
  longer offer.

Out of scope, per the issue: a "test connection" action (no such endpoint exists; adding one is a
backend change and its own issue), and any change to the connections API, the stored schema or the
field-transform semantics. This is a UI relocation.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `connections`: the "Connections settings UI" requirement is retired and replaced by one that states
  the same form and table contract at its new home on the Connect page.
- `default-connection`: the "Settings names the default connection" requirement is retired and
  replaced by one stating the same control at its new home; "Connect opens on a resolved connection"
  is modified, because the page now shows the management block when nothing resolves, the default is
  now set from Connect rather than from Settings, and the selection now clears when the connection it
  names stops being an option.

## Impact

- `ui/src/pages/settings/ConnectionsSection.tsx` and its test move to `ui/src/pages/connect/`.
- `ui/src/pages/Settings.tsx` drops the section; `ui/src/pages/Connect.tsx` renders it and gains the
  disclosure state and the selection-clearing rule.
- `ui/src/pages/Settings.test.tsx` keeps a fetch spy and gains two checks: that no connections UI
  renders, and that once the page's queries have settled no request was made to `/api/connections`.
- `ui/src/pages/Connect.test.tsx` gains the block's coverage, including that rows selected against a
  connection do not come back after that connection is cleared and another is picked.
- No Rust change: no endpoint, request body, response body or stored schema is touched.
