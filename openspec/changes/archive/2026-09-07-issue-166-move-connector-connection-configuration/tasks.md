## 1. Move the section to the Connect page's folder

- [x] 1.1 `git mv ui/src/pages/settings/ConnectionsSection.tsx ui/src/pages/connect/ConnectionsSection.tsx`
      and `git mv ui/src/pages/settings/ConnectionsSection.test.tsx ui/src/pages/connect/ConnectionsSection.test.tsx`.
      `connect/` is the same depth as `settings/`, so every `../../` import resolves unchanged; change
      no import path unless the build says otherwise.
- [x] 1.2 Run `cd ui && npx vitest run src/pages/connect/ConnectionsSection.test.tsx` and confirm every
      test passes at the new path, unedited. These are the tests that keep proving the form, table,
      public-URL and default-control scenarios of both delta specs.
- [x] 1.3 Delete the `ConnectionsSection` import and its render line from `ui/src/pages/Settings.tsx`,
      leaving no link, stub or redirect in its place.

## 2. Render the block on Connect

- [x] 2.1 In `ui/src/pages/Connect.tsx`, render `ConnectionsSection` inside a collapsible block labelled
      **Manage connections**, placed below the connection and template pickers and above the composer
      and the browse table. Use the disclosure shape the codebase already has
      (`ui/src/pages/settings/PrintersSection.tsx:190-199`): a `<button type="button">` carrying
      `aria-expanded`, with the contents rendered only when open.
- [x] 2.2 Resolve the open state with a one-shot latch beside the existing `latchedConnectionId` block:
      `open` starts `null` and renders collapsed; the first render in which the connections list has
      loaded sets it to `connections.length === 0`; a list that is still loading or that failed leaves
      it `null`; after that only the operator's clicks move it. Do not persist it.

## 3. Clear a selection the connection list no longer offers

- [x] 3.1 In `ui/src/pages/Connect.tsx`, during render next to the latch and guarded by the list having
      loaded successfully, clear the selection when `connectionId` is non-empty and the loaded list
      holds no enabled connection with that id. Clearing is two writes: `setSelectedConnectionId("")`
      (not `null`, which falls back to the latch) and `setSelected([])`, because `selected` is owned by
      `Connect` and survives the keyed remount of the composer and the browse table.

## 4. Repoint the existing Connect tests

- [x] 4.1 In `ui/src/pages/Connect.test.tsx`, change all thirteen picker lookups from
      `findByLabelText(/connection/i)` to `findByLabelText(/^connection$/i)`, so none of them can match
      the block's `default connection` control.
- [x] 4.2 Extend the existing "selects nothing when no connection is enabled" test to also assert the
      Manage connections block is shown.

## 5. Prove Settings has no connections UI

- [x] 5.1 In `ui/src/pages/Settings.test.tsx`, drop the `/api/connections` branch from the fetch stub,
      keeping the stub a `vi.fn` spy.
- [x] 5.2 Add a test that renders Settings, waits for a section it does render to settle, and asserts
      both that no connections table, connection form or default-connection control is in the document
      and that no call in the spy's `mock.calls` targets `/api/connections`.

## 6. Cover the block's new behavior on Connect

- [x] 6.1 Test the disclosure's three starting states: collapsed when the list loads with at least one
      connection and opening on a click; expanded when the list loads empty; collapsed when the request
      for the list fails. Scope the block's own queries with `within` or exact labels.
- [x] 6.2 Test that adding an enabled connection in the block puts it in the connection picker without a
      reload and leaves the selected connection, the browse table and the row selection unchanged.
- [x] 6.3 Test that adding a connection with **enabled** cleared lists it in the block's table, does not
      offer it in the picker, and leaves the selection unchanged.
- [x] 6.4 Test that renaming the selected connection, left enabled, offers it under the new name without
      a reload and leaves it selected.
- [x] 6.5 Test that deleting the connection currently selected returns the picker to "choose a
      connection" and leaves no browse table, row selection or composer behind.
- [x] 6.6 Test that disabling the connection currently selected does the same.
- [x] 6.7 Test that rows selected against a connection do not come back: select rows on `c1`, clear `c1`
      by deleting or disabling it, pick `c2`, and assert nothing is selected and adding rows adds only
      `c2`'s.
- [x] 6.8 Test that a later connections request that fails leaves the selected connection, the browse
      table and the row selection unchanged.
- [x] 6.9 Test that naming a different connection as the default while working on one leaves the
      selected connection, the browse table and the row selection unchanged.

## 7. Gates

- [x] 7.1 `cd ui && npm run lint`
- [x] 7.2 `cd ui && npm run test`
- [x] 7.3 `cd ui && npm run build`
- [x] 7.4 `cargo fmt --check`
- [x] 7.5 `cargo clippy --all-targets --all-features`
- [x] 7.6 `cargo test`
