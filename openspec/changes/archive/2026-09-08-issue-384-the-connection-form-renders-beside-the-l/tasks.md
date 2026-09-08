## 1. Query layer

- [x] 1.1 In `ui/src/api/connectors.ts`, delete the `labeler:connection-saved` and
      `labeler:connection-deleted` `dispatchEvent` calls from `useSaveConnection` and
      `useDeleteConnection`.
- [x] 1.2 Give `useSaveConnection` and `useDeleteConnection` `mutationKey: ["connection"]`.
- [x] 1.3 Replace their invalidation with the per-write eviction the delta fixes: a create removes
      `["connections"]`; an update removes `["connections"]` and `["connector-schema", id]`; a delete
      removes those two and `["settings"]`. Keep the eviction in each hook's own `onSuccess` so it
      runs after the form unmounts, and evict nothing when the request fails.
- [x] 1.4 Add `useSetDefaultConnection` and `useClearDefaultConnection` beside them, hitting
      `PUT`/`DELETE /settings/default_connection_id`, carrying `mutationKey: ["connection"]` and
      removing `["settings"]` in their own `onSuccess`. Leave `useUpdateSetting` and `useResetSetting`
      untouched.

## 2. Routing and navigation

- [x] 2.1 Add `connections`, `connections/new` and `connections/:id` routes to `ui/src/app/App.tsx`,
      inside the authenticated `Shell` layout beside `connect`.
- [x] 2.2 Add a **Connections** entry to `NAV_ITEMS` in `ui/src/app/Shell.tsx`, directly after
      **Connect**, pointing at `/connections`.

## 3. The connections list page

- [x] 3.1 Create `ui/src/pages/connections/ConnectionsList.tsx` rendering the connections table with
      name, connector, base URL, public URL (`-` when absent), whether an API key is set and whether
      the connection is enabled, in the order `GET /api/connections` returns.
- [x] 3.2 Give each row an **Edit** link to `/connections/{id}` and no delete, and give the page an
      **Add connection** link to `/connections/new`.
- [x] 3.3 Render the still-loading, failed and loaded-empty states distinguishably.
- [x] 3.4 Move the **Default connection** control onto this page unchanged in behavior, switched to
      `useSetDefaultConnection` / `useClearDefaultConnection`, keeping the unavailable state that only
      appears once the connections list has loaded.
- [x] 3.5 Read `location.state.from` and relay it onto the **Add connection** and **Edit** links,
      relaying nothing when the page was reached without one.

## 4. The connection form page

- [x] 4.1 Create `ui/src/pages/connections/ConnectionForm.tsx` as a route shell that reads the `id`
      param, the connections list and `location.state.from`, and renders: nothing decisive while the
      list has not answered, the load failure when it failed, and a not-found state naming the id and
      linking to `/connections` when the loaded list holds no such connection.
- [x] 4.2 Have the shell render the draft-owning subtree under
      ``key={`${location.key}:${id ?? "new"}`}``, with every draft field, the rule editor and its
      revision counters, the preview results, the validation messages and the `useMutation` observer
      inside it.
- [x] 4.3 Move `CreateConnectionForm` and `EditConnectionForm` out of
      `ui/src/pages/connect/ConnectionsSection.tsx` into that subtree, relaid out as a single-column
      page form with a **Details** section (connector, name, base url, public url, api key, enabled)
      and a **Field transforms** section, keeping the connector control fixed and the rule editor,
      its preview call and its suspension rules unchanged.
- [x] 4.4 Keep create carrying no rule editor and its "rules can be added after saving" note.
- [x] 4.5 Move the confirm-then-delete action from `ConnectionRow` into the edit form, and offer no
      delete on `/connections/new`.
- [x] 4.6 Navigate from the `mutate()` call-site callback only: a successful save and a cancel go to
      the honoured origin or to `/connections`, a successful delete always goes to `/connections`, and
      an origin is honoured only when it is a string beginning with a single `/`.

## 5. The Connect page

- [x] 5.1 In `ui/src/pages/Connect.tsx`, remove the `ConnectionsSection` render, the `open` disclosure
      state, both window-event listeners and the `refreshToken` state.
- [x] 5.2 Add the link to `/connections` carrying `state: { from: "/connect" }`, and make the
      loaded-empty state a call to action linking to `/connections/new` with the same state.
- [x] 5.3 Report a failed connections list, and offer no call to action in that state.
- [x] 5.4 Gate the page on `useIsMutating({ mutationKey: ["connection"] })`: while it is non-zero,
      resolve no connection, read no connector schema, browse no rows and report that the page is
      waiting.
- [x] 5.5 Add the same condition to the resolution latch, so it fires only once no management write is
      in flight and both inputs are answers to requests started after the latest successful write
      affecting them.

## 6. Retire the inline block

- [x] 6.1 Delete `ui/src/pages/connect/ConnectionsSection.tsx` once nothing imports it.
- [x] 6.2 Remove the `refreshToken` prop from `ui/src/pages/connect/ConnectorBrowser.tsx` and its
      effect dependency list.

## 7. Tests for the connections routes

- [x] 7.1 Move `ui/src/pages/connect/ConnectionsSection.test.tsx` to
      `ui/src/pages/connections/`, split between the list and the form, keeping the form cases that
      still hold: public URL set, cleared, invalid and unset; API key kept and required; create
      carrying no rule editor.
- [x] 7.2 Cover the list page: the table's columns and `-`, **Edit** with no delete, **Add
      connection**, and the loading, failed and empty states.
- [x] 7.3 Cover the default-connection control on `/connections`, including choosing, clearing, no
      default stored, two connections sharing a name, a disabled connection marked, the unavailable
      state for a stored id no connection has, that state not being reported while the list has not
      answered, and no default shown after deleting the connection that was the default.
- [x] 7.4 Cover the form route: a cold deep link to `/connections/{id}`, an unknown id rendering the
      not-found state naming it, the list still pending, the list failed, and the confirming step
      before a delete request.
- [x] 7.5 Cover editor identity: navigating from `/connections/{b}` with an unsaved edit and a
      deferred save back to `/connections/{a}` shows `a`'s stored values with none of `b`'s draft or
      previews, and `b`'s save landing afterwards changes neither the location nor the form on screen
      while still evicting what it affects; and reopening the same connection shows stored values with
      the abandoned draft gone.
- [x] 7.6 Cover the return paths: save from Connect's call to action, save and cancel from Connect by
      way of the list, save from the list reached on its own, save from a cold load, an origin that is
      not an in-app path, a delete ignoring the origin, and a completion moving nobody who has already
      left while its cache maintenance still happens.
- [x] 7.7 Add the nav item to `ui/src/app/Shell.test.tsx`.

## 8. Tests for the Connect page

- [x] 8.1 Cover that Connect renders no connections table, form or default-connection control, offers
      the link to `/connections`, offers the call to action when the list loads empty, offers none
      while the list has not answered, and reports a failed list.
- [x] 8.2 Cover resolution per visit: returning from connection management resolving afresh without
      restoring a hand-picked connection or its rows, a connection created while away being able to
      resolve, creating the first connection from the call to action, and a connection saved disabled,
      renamed or deleted while away.
- [x] 8.3 Cover the pending-write gate: leaving the form by **Cancel** during a save and by primary
      navigation during a delete, both leaving Connect selecting nothing and reporting that it waits,
      then acting on answers read afterwards and not moving the operator; and naming a default then
      opening Connect before the write answers.
- [x] 8.4 Cover freshness: a save reaching Connect on the next visit with that connection selected, a
      credential-only save, no pre-write answer being presented, a failed write changing nothing, a
      default write not evicting the connections list, and a deleted connection leaving no held schema
      and drawing no request.
- [x] 8.5 Keep the surviving selection rules covered: a later refetch not moving the operator, another
      operator disabling the selected connection clearing it, rows not coming back after a clearing, a
      failed refetch clearing nothing, changing the picker by hand, and the only-disabled state showing
      the picker and the link and nothing below.
- [x] 8.6 Cover the field-transform scenarios at their new timing: with the connection selected and
      the rule's resource browsed on the return, a newly derived column and its cells, a rule edit
      that changes values but no column, a removed rule, and the composer's field mapping offering the
      derived name.

## 9. Gates

- [x] 9.1 Run `cd ui && npm run lint`, `npm run test` and `npm run build`, and fix what they report.
- [x] 9.2 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features` and `cargo test`, which
      this change leaves untouched, and confirm they still pass.
