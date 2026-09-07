## Why

Implements GitHub issue **#380**.

Saving a connection does not refresh what the open Connect page holds for it, so a field transform the
save just created is not visible until the browser reloads. `useSaveConnection` invalidates
`["connections"]` alone (`ui/src/api/connectors.ts:79`); the connection's schema is re-read only
because one call site remembers to ask; the browse grid re-reads nothing it can be relied on to
re-read; and a column the operator has just derived is not shown even after a reload.

**Three claims in the issue do not survive verification, and this plan is written against what was
measured.** All three were checked on this branch's tree at `ad57145`.

1. *"`["connector-schema", id]` is never invalidated."* The edit form already invalidates it in its own
   `onSuccess` (`ConnectionsSection.tsx:241`), added by `dd78fcc` (#195) 21 minutes before the issue
   was filed. The rule editor is the only place `transforms` are submitted, so a rule save does refresh
   the schema on this tree. What is missing is the guarantee: the rule lives in a caller rather than in
   the mutation every caller goes through, and the same handler closes the form, so the issue's "the
   form stays open" premise does not hold either.
2. *"A save leaves the old cells in place."* Only sometimes, and by accident. The browse effect depends
   on the `ResourceSpec` object, and react-query's structural sharing hands back a new one exactly when
   that resource's column list changed. Measured: invalidating the schema with a derived column added
   re-issued the browse (1 request before, 2 after); invalidating it with a byte-identical schema
   re-issued nothing (1 before, 1 after). So a rule edit that changes a `pattern` and no capture name,
   and every save that changes the upstream rather than the rules, leaves stale rows on screen.
3. *"A derived column is visible in the grid."* It is not, and a reload does not make it so. A derived
   column carries `tier: derived` (`src/connector/mod.rs:563`) and `defaultColumnKeys` keeps only
   `cheap` columns when a resource has any (`ui/src/pages/connect/connectorColumns.ts:3-9`). Measured:
   after the schema refreshed, the column picker's count went `Columns (2/2)` → `Columns (2/3)` while
   no derived column header ever rendered. The issue's accepted outcome is that the column appears and
   carries its values, so this change delivers that too rather than narrowing the issue.

## What Changes

- `useSaveConnection` cancels that connection's schema query and then invalidates it, alongside
  invalidating `["connections"]`, from its hook-level `onSuccess`, and the duplicate at
  `ConnectionsSection.tsx:241` is deleted. One rule, one owner, and one that survives the control that
  started the save being unmounted: measured against the installed react-query 5.101.4, a hook-level
  `onSuccess` runs after its component is gone and the per-call callback the tree uses today does not.
  The cancellation is what makes the rule hold over a schema read that is still in flight: measured,
  invalidating alone left the pre-save response to become the page's schema with no second request,
  while cancelling first started a fresh read and left the abandoned answer discarded.
- **The Connect page re-browses the rows on screen when the connection being browsed is saved**, for
  the resource, filters and parent in force, through the existing effect and its `reqToken` guard. The
  trigger is the save itself, announced on a window event from the same hook-level handler
  (`labeler:connection-saved`, matching the existing `labeler:unauthenticated` channel) so that
  collapsing the Manage connections block mid-request cannot cost the refresh. It has to be the event
  and not the data: a save can change the upstream (`base_url`) or rotate the credential, and a
  rotation is invisible in every response the page could compare. The schema-shaped trigger that exists
  today is kept, so a schema change arriving without a save still refreshes the rows.
- **A column this connection's transforms derived opens visible**, alongside the `cheap` columns, and
  stays hidden once the operator hides it. One resolver applies that rule to the operator's column
  choice wherever it is held, in this session's overrides as well as in storage, so customizing a
  resource's columns does not go on hiding a field the operator has just created. A connector's own
  derived columns (`item_url`, `location_url`) are unaffected: the schema offers them as transform
  sources, which is what tells them apart.
- **Deleting the connection being browsed clears the selection at the delete**, announced on
  `labeler:connection-deleted` before `useDeleteConnection` drops `["connector-schema", <id>]`, rather
  than when the connections list next reports the absence. The Manage connections block listens for
  the same event and closes an editor open on the deleted connection through a functional update that
  reads the current editor, replacing the delete button's own callback: that callback closes over the
  editor of the render it started in, so opening an editor after the delete starts, or collapsing and
  reopening the block, leaves an editor mounted on a connection that is gone. Until it is cleared the page still holds and
  still asks for the deleted connection's schema, and a list refetch that fails never clears it at all.
  Verified with the installed react-query: removing that entry while the observer is still enabled
  re-creates it and issues another request on the next render (1 → 2 requests); removing it with the
  selection retired in the same block leaves no entry and no request, in either order.
- **BREAKING (UI)**: a resource whose columns the operator customized before this change shows any
  transform-derived column they had hidden, once. The old record cannot distinguish "hidden" from
  "never offered", and the guarantee this issue asks for needs that ambiguity resolved toward showing.
  Hiding it again is recorded and sticks.
- No Rust and no API change: the schema and browse responses are already correct.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `connections`: ADDED requirement that saving a connection re-reads its schema and re-browses the
  rows on screen, and that deleting one leaves no held schema and issues no request for it.
- `connector-field-transforms`: ADDED requirement that a saved rule shows on the open page without a
  reload: offered by the field picker, visible in the browse table, and carrying its cells.
- `connector-browser`: ADDED requirements that a column a transform derived opens visible unless
  hidden, and that a column a save removes stops ordering and stops narrowing without being cleared;
  MODIFIED "Ordering and filtering are transient, and reset with the browsing context", whose
  unconditional column-visibility persistence has to name the one legacy record that cannot honour it.
- `default-connection`: MODIFIED "Connect opens on a resolved connection", adding that a delete
  performed in the Manage connections block clears the selection at the delete rather than when the
  connections list reports it.

## Impact

- `ui/src/api/connectors.ts`: `useSaveConnection`, `useDeleteConnection`.
- `ui/src/pages/connect/ConnectionsSection.tsx`: the duplicated invalidation goes; the section listens
  for `labeler:connection-deleted` and retires a matching editor, and the per-call `onDeleted` path is
  removed.
- `ui/src/pages/Connect.tsx`: listens for the two events, holds the refresh token, retires the
  selection on a delete.
- `ui/src/pages/connect/ConnectorBrowser.tsx`: one added dependency on the browse effect, and the
  in-memory column overrides carry the same record as storage.
- `ui/src/pages/connect/connectorColumns.ts`: the opening column set, the choice's shape, and the one
  resolver both paths go through.
- Tests under `ui/src/pages/` and `ui/src/api/`. Existing column-visibility tests that assert a
  transform-derived column is hidden encode the behavior this change reverses and are expected to
  fail; ones about `item_url` and `location_url` are not.
- Gates: `ui: npm run lint && npm run test && npm run build`. No Rust file is touched.
