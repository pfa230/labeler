## 1. The save mutation owns the schema re-read

- [x] 1.1 Add a failing test in `ui/src/api/connectors.test.ts`: saving through `useSaveConnection`
  (rendered against a `QueryClient`) cancels and then invalidates `["connector-schema", <id>]` for the
  saved connection as well as invalidating `["connections"]`, for an update and for a create. Covers
  the `connections` scenario "A save that changes no rule still re-reads the schema". Run it and record
  that it fails on the unmodified tree, where only one caller invalidates.
- [x] 1.2 Add a failing test in the same file for the `connections` scenario "A save while the first
  schema read is still in flight": with the first schema request unresolved, a save starts a fresh
  read, and resolving the abandoned request afterwards does not replace the new schema. Record the
  failure (measured today: one request, and the pre-save answer becomes the schema).
- [x] 1.3 Implement in `useSaveConnection`'s hook-level `onSuccess`: `cancelQueries` on
  `["connector-schema", <id>]` with `exact: true`, then `invalidateQueries` on that key, then
  `invalidateQueries` on `["connections"]`, taking the id from the mutation variables for an update and
  from the created connection for a create. Neither call is awaited; the handler stays synchronous.
  1.1 and 1.2 pass.
- [x] 1.4 Delete the duplicated `invalidateQueries({ queryKey: ["connector-schema", initial.id] })` from
  the edit form's per-call `onSuccess` in `ui/src/pages/connect/ConnectionsSection.tsx`, leaving its
  toast and `onClose`. The whole suite still passes.

## 2. The delete mutation and the page's reaction

- [x] 2.1 Add a failing test in `ui/src/api/connectors.test.ts`: deleting through `useDeleteConnection`
  dispatches `labeler:connection-deleted` carrying the deleted id, then removes
  `["connector-schema", <id>]` (`exact: true`), then invalidates `["connections"]` and `["settings"]`.
  Record the failure.
- [x] 2.2 Add a failing test in the same file: `useSaveConnection` dispatches `labeler:connection-saved`
  carrying the saved connection's id. Record the failure.
- [x] 2.3 Implement both dispatches in the two hook-level `onSuccess` handlers, the delete's event going
  out before `removeQueries`. 2.1 and 2.2 pass.
- [x] 2.4 Add a failing test in `ui/src/pages/Connect.test.tsx` for the `default-connection` scenarios
  "A delete clears the selection before the connections list reloads" and "The Manage connections block
  is collapsed before the delete completes", and the `connections` scenario "A deleted connection's
  schema is not kept": delete the selected connection with the connections refetch delayed or failing,
  re-render, and assert the picker is back to "choose a connection" with no browse table, row selection
  or composer, no cached schema for the deleted id, and no request for it. Record the failure (measured
  today: the schema entry is re-created and requested, 1 → 2 requests).
- [x] 2.5 Implement the listeners in `ui/src/pages/Connect.tsx`: a `labeler:connection-deleted` listener
  that clears the selected connection and the row selection when the deleted id is the one being
  browsed, and a `labeler:connection-saved` listener that bumps a refresh token when the saved id is
  the one being browsed. Keep the existing "no longer offered" check as the backstop. 2.4 passes.

## 3. The section retires an editor on the delete event

- [x] 3.1 Add failing tests in `ui/src/pages/connect/ConnectionsSection.test.tsx` for the `connections`
  scenarios "An editor opened after the delete starts still closes" and "The block is collapsed and
  reopened before the delete completes": with the connections list refetch delayed or failed, an editor
  opened on the deleted connection after the delete started closes, an editor open on another
  connection stays open, and re-rendering neither re-creates nor requests the deleted connection's
  schema. Record the failures.
- [x] 3.2 Implement the `labeler:connection-deleted` listener in `ConnectionsSection`, closing the
  editor through a functional `setEditing` update that compares the **current** editor's connection id
  with the event's id.
- [x] 3.3 Remove the per-call `onDeleted` path it replaces: the `ConnectionRow` prop, the call in the
  delete button's `onSuccess`, and the section's `onDeleted` handler, keeping the delete toast. 3.1
  passes and the existing section tests still pass.

## 4. The browse table refreshes on a save

- [x] 4.1 Add failing tests in `ui/src/pages/Connect.test.tsx` for the `connections` scenarios "A save
  that points the connection at another upstream replaces the rows", "The Manage connections block is
  collapsed before the save completes" and "Saving another connection does not disturb the browse
  table", and the `connector-field-transforms` scenario "A rule edit that changes values but no
  column": each saves without remounting the page and asserts on the rows on screen and on the browse
  requests made. Record the failures (measured today: a save whose schema is byte-identical issues no
  new browse).
- [x] 4.2 Add the refresh-token prop to `ui/src/pages/connect/ConnectorBrowser.tsx` and to the browse
  effect's dependency list, keeping `resource`, `applied`, `parent` and the shared `reqToken` guard as
  they are, and pass the token from `Connect`. The refresh clears rows and cursor and starts from the
  first page, as the effect already does. 4.1 passes.

## 5. A column a transform derived opens visible

- [x] 5.1 Add failing tests in `ui/src/pages/connect/connectorColumns.test.ts` for the opening set and
  the choice record: with no stored choice a resource shows its `cheap` columns plus every column of
  tier `derived` that is not a transform source, and shows all columns when it has neither; a stored
  choice hides what it hid and still shows a transform-derived column it never saw; a hidden
  transform-derived column is recorded and stays hidden on reload; and a legacy array-shaped record is
  read as hiding no derived column while keeping every other column it hides hidden. Covers the
  `connector-browser` scenarios "A resource opens showing the columns a rule derived", "Hiding a
  derived column sticks" and "A column choice recorded before derived columns opened visible". Record
  the failures.
- [x] 5.2 Implement in `ui/src/pages/connect/connectorColumns.ts`: the choice record of visible columns
  plus hidden transform-derived columns, the storage read that accepts the legacy array shape, the
  storage write that records both, and the single resolver that turns a choice (or its absence) into
  the visible set. 5.1 passes.
- [x] 5.3 Add a failing test in `ui/src/pages/connect/ConnectorBrowser.test.tsx` for the
  `connector-browser` scenario "A newly derived column appears for a resource customized in this
  session": change which columns a resource shows through the picker, then, on the same mount, take a
  schema carrying a newly derived name and assert the new column is shown while the columns just
  hidden stay hidden. Record the failure (the in-memory override is consulted before storage today).
- [x] 5.4 Change `columnOverrides` in `ConnectorBrowser` to hold the same choice record as storage and
  resolve the visible keys through the one resolver for both paths, writing the record in
  `setVisibleKeysForCurrent`. 5.3 passes.
- [x] 5.5 Update the existing column-visibility tests that assert a transform-derived column is hidden,
  which encode the behavior this change reverses. Tests asserting `item_url` and `location_url` stay
  hidden are correct as they stand and must keep passing.

## 6. What a saved rule shows, and the guards

- [x] 6.1 Add failing tests in `ui/src/pages/Connect.test.tsx` for the `connector-field-transforms`
  scenarios "A newly derived field reaches the rows on screen", "Removing a rule removes its column and
  its cells" and "The field mapping offers a newly derived field": saving a rule shows the derived
  column with its cells on the rows that matched and none on the rows that did not, removing the rule
  takes both away, and the composer's field mapping offers the derived name, all without a reload or a
  remount. Record the failures.
- [x] 6.2 Confirm 6.1 passes on the implemented tree, with no further production change; if any of it
  still fails, fix the code rather than the assertion.
- [x] 6.3 Add the guard tests, described as guards rather than as proof of the fix, for the `connections`
  scenarios "A refresh superseded by a resource switch is dropped" and "The browsing context survives
  the refresh": a refresh superseded by a resource switch never reaches the table, and the sort, the
  column filters, the visible columns and the row selection survive a refresh that removes no column
  they name.
- [x] 6.4 Add the guard tests for the `connector-browser` scenarios "Sorting by a column a save
  removes", "Filtering by a column a save removes" and "A column that comes back resumes": the column
  stops ordering and stops narrowing without either being cleared, and both resume when the rule is
  restored. These pass before and after; record them as guards.

## 7. Gates

- [x] 7.1 `cd ui && npm run lint` passes with no new warnings, and no lint is silenced.
- [x] 7.2 `cd ui && npm run test` passes, with every test added above present and passing.
- [x] 7.3 `cd ui && npm run build` passes.
- [x] 7.4 `cargo fmt --check`, `cargo clippy --all-targets --all-features` and `cargo test` pass. No
  Rust file is touched by this change, so these confirm the tree is unchanged there rather than
  exercising new code.
