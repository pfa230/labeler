## Why

Implements GitHub issue **#384**.

The connection add/edit form renders inside the collapsible **Manage connections** block on `/connect`
(`ui/src/pages/Connect.tsx:135-142`), a `flex flex-wrap` row of five `flex-1` inputs plus a checkbox
followed by an unbounded field-transform rule editor whose per-rule preview renders its own scrolling
result list (`ui/src/pages/connect/ConnectionsSection.tsx:60-88`, `466-500`). Six fields plus a
repeating editor is past every published threshold for an inline or overlaid form: Carbon puts
anything complex, lengthy or multistep on a dedicated page; Polaris moves large forms out of
constrained containers onto a new page categorically; NN/g prefers a separate view over inline
accordion editing beside a table. Every peer product that manages a credential-bearing connection
(Grafana, Metabase, Airbyte, n8n, Zapier) separates the list from the editor, and none keeps the two
as live panes.

This supersedes the placement #166 chose one day earlier, on the reasoning that "a separate route
relocates the round trip rather than removing it". The round trip is the cheaper problem: #166 traded
it for a form that no layout rescues. Everything else #166 decided stands, in particular that
`/settings` renders no connections UI, with no link stub and no redirect.

## What Changes

- **BREAKING**: `/connect` renders no connections table, no connection form and no default-connection
  control. The **Manage connections** block is removed. An operator managing connections goes to
  `/connections`.
- New routes: `/connections` holds the connections table and the **Default connection** selector;
  `/connections/new` and `/connections/:id` hold the form, which replaces the list rather than
  rendering beside it. A **Connections** nav item follows **Connect**.
- The form is rebuilt as a single-column page form with titled sections: **Details** (connector, name,
  base url, public url, api key, enabled) and **Field transforms**, the latter still absent on create
  with the existing "rules are added after saving" note. Field-transform semantics, the preview call
  and what the editor offers are untouched.
- `/connections/:id` deep-links to the editor on a cold load. An unknown id renders an explicit
  not-found state naming the id with a link back to `/connections`, never an empty form and never a
  silent redirect.
- Deleting a connection moves from the table row to the editor, which is where the issue's acceptance
  criteria put creating, editing, deleting and enabling/disabling. A successful delete goes to
  `/connections`.
- A link into the form carries router state naming its origin, and `/connections` relays the origin it
  was reached with onto its own **Add connection** and **Edit** links. A save or a cancel returns to
  that origin when present and to `/connections` otherwise, so a form reached from Connect lands back
  on Connect by either path: the empty-list shortcut straight to `/connections/new`, or the ordinary
  Connect → list → editor route.
- `/connect`'s empty-connections state becomes a call to action linking to `/connections/new` instead
  of an auto-expanded block.
- **BREAKING**: the `labeler:connection-saved` and `labeler:connection-deleted` window events and
  their listeners are removed. They existed only because the form and the Connect page shared a
  mount. Two rules replace them, because separate routes stop a write from starting beside Connect but
  not from finishing there, an operator being free to leave a pending save by **Cancel** or by the
  nav: a successful connection-management write evicts the answers it affects rather than marking them
  stale, durably and whether or not the form is still mounted, and the Connect page resolves nothing,
  reads no schema and browses no rows while any such write is in flight. Once none is, each answer the
  page acts on comes from a request started after the latest successful write affecting it; an answer
  no completed write affects, and one held across a failed write, is reused as held. A completion
  navigates only when the form that started it is still the active view.
- The latched resolution becomes per visit, and gains a precondition: it fires only once no management
  write is pending, on inputs no successful write has since invalidated. Returning from `/connections`
  resolves afresh and does not restore the connection the operator had picked by hand, nor its rows or
  row selection.

Out of scope, per the issue: any change to the connections API, the stored schema or field-transform
semantics, and a test-connection action, which has no endpoint and stays deferred.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `connections`: "Connection management on the Connect page" and "Saving or deleting a connection
  refreshes what the open Connect page holds" are both retired. They are replaced by requirements
  stating connection management at its own route, the page form and its not-found state, where a form
  returns to, and how a connection write settles before Connect acts on it.
- `default-connection`: "Connect names the default connection" is retired and replaced by one stating
  the same control on `/connections`. "Connect opens on a resolved connection" is retired for
  "Connect opens on a connection it resolves per visit": the resolution order is unchanged, but it
  latches per visit rather than for a session, and every clause and scenario turning on a form that
  shared the page's mount goes with the block.
- `connector-field-transforms`: "A saved rule shows on the open Connect page without a reload" is
  retired and replaced. Its contract is that saving a rule updates the Connect page **without a
  reload and without remounting it**, which this change makes false by construction: the rule editor
  is no longer on that page. The issue named only the first two deltas; this third one is the direct
  consequence of its own decision, and leaving it unwritten would leave a published spec asserting
  behavior the change removes. No field-transform semantics change.

## Impact

- `ui/src/app/App.tsx` gains three routes; `ui/src/app/Shell.tsx` gains a **Connections** nav item.
- `ui/src/pages/connect/ConnectionsSection.tsx` is split into the list page and the form page under
  `ui/src/pages/connections/`, and its test moves with it. The form's route component keys its
  draft-owning subtree by history entry and connection id, so moving between editors is a fresh
  editor rather than the previous one holding another connection's draft and its pending save.
- `ui/src/pages/Connect.tsx` drops the block, the disclosure state and both window-event listeners,
  and gains the link and the empty-state call to action.
- `ui/src/api/connectors.ts` stops dispatching both window events, drops the connections list, the
  written connection's schema and the settings from the cache rather than marking them stale, and
  gains keyed mutations for the default-connection write and clear so that all four
  connection-management writes evict durably and are visible to Connect while in flight. The eviction
  is hook-level and survives the form unmounting; only the navigation stays at the form's call site,
  so a completion moves nobody who has already left. `ConnectorBrowser`'s `refreshToken` prop loses
  its only caller and goes. The shared `useUpdateSetting`/`useResetSetting` are untouched, so the
  Settings page is unaffected.
- `ui/src/pages/Connect.test.tsx` and `ui/src/app/Shell.test.tsx` change; a new test file covers the
  connections routes.
- No Rust change: no endpoint, request body, response body or stored schema is touched.
