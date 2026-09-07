## Context

See `proposal.md` for motivation and for the three measured corrections to the issue's diagnosis.

Six client-side facts shape everything below. Each was checked on this tree at `ad57145`, against the
installed `@tanstack/react-query` 5.101.4.

- `Connect` holds the connector schema in one query, `["connector-schema", <id>]`, and hands it to the
  composer (whose field-mapping options are every resource's columns) and to the browse table (whose
  column picker and visible columns are the browsed resource's). Refreshing that query refreshes all
  of them.
- The browse table's rows are **not** a query. They are component state filled by an effect
  (`ConnectorBrowser.tsx:212-236`) firing on `[connectionId, resource, applied, parent]`, and by
  `loadMore`, both guarded by a shared monotonic `reqToken` so a slower in-flight request is dropped.
  Nothing invalidates rows, so a refresh has to arrive as a change to that dependency set.
- Mounting `Connect` against a stubbed API and invalidating the schema by hand: a refetch that adds a
  derived column re-browses (1 request → 2), and a refetch of a byte-identical schema does not (1 → 1).
  The first is react-query structural sharing changing the `ResourceSpec` identity; the second is the
  hole.
- `removeQueries` on a key whose observer is still enabled loses the data and then re-creates the entry
  and refetches on the next render (1 request → 2). Retiring the observer in the same synchronous block
  as the removal leaves no entry and no request, **in either order**. The page keeps
  `useConnectorSchema(connectionId)` enabled until the refetched connections list reports the absence,
  and when that refetch fails the check is skipped entirely (`Connect.tsx:61`), so it may never report
  it at all.
- **Mutation callbacks do not all survive the component unmounting.** Probed by starting a mutation,
  unmounting its component, then resolving the request: the per-call callback passed to
  `mutate(vars, { onSuccess })` did **not** run, while the hook-level `useMutation({ onSuccess })` did,
  as did the code after the `await` inside `mutationFn`. That split matters here because collapsing
  the Manage connections block unmounts every control inside it (`Connect.tsx:118`), and closing a form
  unmounts one.
- `ConnectorBrowser` holds an in-memory `columnOverrides` map and returns an entry from it **before**
  consulting storage (`ConnectorBrowser.tsx:85-90`), so any rule that lives only in the storage reader
  is bypassed for the rest of the session once the operator touches the column picker.

## Goals / Non-Goals

**Goals**

- Make the schema re-read a property of saving a connection, not of one caller.
- Refresh the rows on screen for **any** save of the browsed connection, including the ones no
  response can reveal, and independently of which controls are still on screen.
- Deliver the issue's accepted outcome: the derived column appears and carries its values.
- Keep `reqToken` the single arbiter of which browse response wins.

**Non-Goals**

- Moving browse rows into react-query. The cursor, the append path and the shared guard would all be
  rewritten for a refresh the issue asks to implement inside the existing effect.
- Changing the opening column set for anything but transform-derived columns.
- Any Rust or API change.

## Decisions

### The save mutation owns the schema invalidation; the caller's copy goes

`useSaveConnection`'s hook-level `onSuccess` invalidates `["connector-schema", <id>]` alongside
`["connections"]`, taking the id from the mutation variables for an update and from the created
connection for a create (a no-op there: a connection that did not exist has no held schema). The
duplicate at `ConnectionsSection.tsx:241` is deleted, because two spellings of one rule is how this bug
came to be filed against a tree that already had one of them.

Hook-level rather than per-call is what makes it hold when the operator collapses the block or closes
the form while the request is in flight: measured above, the hook-level callback runs after the
component is gone and the per-call one does not.

**It cancels that connection's schema query before invalidating it.** Invalidating alone does not
disturb a request already in flight, and the response that request is about to deliver was computed
before the save. Measured: with the first schema read still pending, invalidating produced no second
request and the pre-save response became the page's schema; cancelling the exact key first produced a
fresh request, and resolving the abandoned one afterwards did not replace its answer. Cancelling is
scoped to that one connection (`exact: true`) and costs nothing when nothing is in flight: with the
schema already cached, cancel-then-invalidate issued exactly the one refetch invalidation issues on
its own. Neither call is awaited, so the handler stays synchronous.

### The page hears about a save or a delete on a channel that outlives the controls

`useSaveConnection` and `useDeleteConnection` dispatch a window event from that same hook-level
`onSuccess`: `labeler:connection-saved` and `labeler:connection-deleted`, each carrying `{ id }`.
`Connect` listens for both, bumping a refresh token when the saved id is the one being browsed, and
retiring the selection when the deleted id is.

The channel has to outlive the controls. Routing this through props into the forms and rows, as an
earlier draft did, puts the signal on exactly the per-call callbacks that a collapse or a Cancel click
drops: the reviewer's case, and also the plainer one of closing the edit form while its save is in
flight.

The window `CustomEvent` is the codebase's existing shape for a signal that crosses from an `api/`
module to a page: `client.ts:16` dispatches `labeler:unauthenticated`, `main.tsx:14` and
`RequireAuth.tsx:10` listen, one of them from inside a component.

`ConnectionsSection` listens for `labeler:connection-deleted` too, and closing the editor for a deleted
connection is that listener's job rather than the delete button's callback. The listener retires the
editor with a functional update that inspects the **current** editor
(`setEditing(cur => cur && cur !== "new" && cur.id === id ? null : cur)`), because the value is what
must decide, and only the update function sees it.

The callback it replaces cannot do this. `ConnectionRow` passes `onDeleted` when the delete starts, so
the handler that eventually runs closes over the `editing` of that render: start a delete with no
editor open, then open one for the same connection, and the handler sees `null` and closes nothing.
Collapsing and reopening the block loses the callback altogether, the section having remounted.

The trigger is the save event rather than anything derived from the data, because a save can change
what the connection answers with while changing nothing the page can compare: a rotated credential
leaves `has_credential` true and both responses potentially identical, and a new `base_url` points at
an upstream whose fields may or may not differ. This is what an earlier draft got wrong by keying the
refresh on the connection's serialized transforms.

The token is id-scoped, so saving some other connection costs no upstream request.

*Alternative: a `MutationCache`-level `onSuccess` on the `QueryClient`.* It survives unmounting too,
and is rejected because it lives in `main.tsx`: every page test builds its own `QueryClient`, so the
rule would be absent from every test that matters and present only in the shipped app.

*Alternative: doing the work after the `await` inside `mutationFn`.* Also survives, and unnecessary
now that the hook-level callback is measured to survive; side effects in the request function would be
the surprising place to look for them.

*Alternative: `Connect` subscribing to the mutation cache.* Public API, but it reads mutation keys,
variables and event shapes to reconstruct what a one-line event already says.

### The existing schema-shaped trigger stays, and costs one superseded request

`resource` stays in the browse effect's dependency list. It is what refreshes the rows when the schema
changes without a save, which is a real case (a window-focus refetch discovering that the upstream
gained a field) and is not this issue's to remove.

Keeping both signals costs one extra browse on a save that changes the browsed resource's column set:
the token bump and the schema's arrival land in separate commits, so the effect runs twice and
`reqToken` discards the first response. One page of one resource, at the moment the operator clicked
Save.

Removing that cost requires the browse to wait for the schema query to settle after a save, which means
passing the query's `isFetching` into the table and returning early while it is true. That buys one
HTTP request and pays with a cross-component coupling, a timing assumption about when `isFetching`
flips relative to the token bump, and a new failure mode where an operator's filter is silently
deferred behind an unrelated background refetch. The request is the cheaper of the two.

### Deleting: the event goes out first, then the entry is dropped

`useDeleteConnection`'s hook-level `onSuccess` dispatches `labeler:connection-deleted`, then calls
`removeQueries({ queryKey: ["connector-schema", id], exact: true })`, then invalidates `["connections"]`
and `["settings"]` as it does today. `Connect`'s listener runs synchronously inside that dispatch and
retires the selection, so the observer is disabled by the time React commits.

Both orders measured clean, so this one is chosen for what it says rather than for what it fixes: stop
naming the connection, then forget it. Removal rather than invalidation, because invalidating schedules
a refetch for any live observer, and for a deleted id that request can only 404.

The editor's own schema observer is retired on the same event, by the section's listener above. The
per-call `onDeleted` callback that does it today is removed rather than kept alongside: two handlers
for one retirement, one of which reads a stale `editing`, is how an editor for a deleted connection
stays mounted and re-creates the entry the mutation has just dropped.

### One rule resolves the visible columns, wherever the operator's choice is held

The operator's column choice for a resource becomes a record of what is visible **and** which
transform-derived columns they hid, and one function resolves the visible set from it:

- no choice: the `cheap` columns plus every transform-derived column, or all columns when a resource
  has neither;
- a choice: the columns it lists that the resource still offers, plus every transform-derived column it
  did not hide.

`columnOverrides` holds that same record rather than a bare `Set`, so the in-session path and the
storage path resolve through the one function. This is finding 1 of the third review: an override
returned ahead of storage is why a customized resource would otherwise keep hiding a column the
operator had just created.

A transform-derived column is one the schema marks `tier: derived` and does not offer as a transform
source. That predicate is the only discriminator the wire format has, and it is exact for every
connector shipped today: `transform_source` is `!multi_valued && ty == Text` for a connector's own
columns (`src/connector/mod.rs:340`) and `false` for a transform's (`mod.rs:565`), and homebox's two
derived columns, `item_url` and `location_url`, are single-valued text, so both remain sources and stay
hidden (`src/connector/homebox.rs:86-92,116-122`). A future connector-derived column that is numeric or
multi-valued would fall in the gap and open visible: a visible extra column, not a wrong value, and the
alternative is a new schema field, which means Rust the issue says not to touch.

*Alternative: record the columns the stored choice was made over, and let any newly appearing column
follow the default.* More general, and rejected for that: it would also start showing a new `cheap`
column on a customized resource, which no issue asks for.

*Alternative: show every `derived`-tier column by default.* Rejected: it puts a "Homebox URL" column
into every operator's grid to fix a problem about columns they wrote rules for.

A record written before this rule cannot distinguish a hidden transform-derived column from one that
did not exist when the choice was made, so it is read as hiding none. That shows such a column once,
which is the side the guarantee falls on, and the next hide is recorded properly. Because
`connector-browser` already requires column visibility to survive a reload unconditionally, that
exception is written into it as a MODIFIED delta rather than left to contradict it.

### A column a save removes needs no new code, and does need the spec

`activeFilters` already prunes column filters to the visible columns on every render
(`ConnectorBrowser.tsx:300-308`), and `comparedRows` already returns the rows unsorted when the sorted
key names no column of the current resource (`ConnectorBrowser.tsx:317-322`). A column withdrawn by a
save therefore stops ordering and stops narrowing, and resumes if the rule comes back, without a line
of new code. The delta states it because the alternative to stating it is a refactor quietly turning an
invisible filter back on.

### The refresh returns to the first page

The effect clears rows and cursor, as it already does for a filter change. Re-fetching every loaded
page multiplies upstream requests; keeping them while refreshing the first leaves stale derived cells
below the fold, which is the defect itself.

## Risks / Trade-offs

- **One superseded browse per save that changes the browsed resource's columns.** → Accepted above,
  with the coalescing alternative priced.
- **A window event is global**: another page mounted at the same time would hear it. → Only `Connect`
  listens, and it is the only page that holds connection-scoped view state; the precedent
  (`labeler:unauthenticated`) is heard by two listeners for the same reason.
- **A refresh in flight when the operator switches resource, drills or filters.** → `reqToken` is
  bumped by whichever request starts last and both paths already drop a superseded response; the delta
  pins it as a scenario so a refactor cannot lose it.
- **A transform-derived column the operator had hidden reappears once.** → Declared BREAKING (UI) in
  the proposal. Bounded to that class of column, and self-correcting at the next hide.
- **The transform-derived predicate is inexact for a connector column that is derived and not a valid
  transform source.** → No connector ships one; the failure is an extra visible column.
- **The hook-level callback surviving an unmount is a library behavior, not a contract this repo
  owns.** → It is measured here against 5.101.4 and pinned by the collapsed-block tests below, which
  fail if a future upgrade changes it.

## Evidence the tests must earn

`proposal.md` records that the issue's first acceptance bullet cannot be met as worded: a test that
saves a rule adding a derived name and asserts the schema was re-read passes on the current tree,
because the edit form already invalidates. The tasks stage must label each test for what it is.

Red on the current tree, with the reason each goes red:

- Saving through `useSaveConnection` re-reads the connection's schema: the mutation does not, only one
  of its callers does.
- A rule edit that changes a `pattern` and no capture name leaves the rows showing the old derived
  values: measured, 1 → 1 browse requests.
- Saving the browsed connection with a different `base_url` leaves the previous upstream's rows on
  screen under the new schema.
- Collapsing the Manage connections block before a save completes loses the refresh entirely: the
  invalidation the tree has today is a per-call callback.
- A resource opens without the column its rule derived, and the row cells for it are absent.
- A resource whose columns were customized **in the same session** keeps hiding a name a rule has just
  derived, the in-memory override being consulted first.
- Deleting the browsed connection with the connections refetch delayed or failing, then re-rendering,
  leaves the schema held and issues a request for the deleted id: measured, 1 → 2 requests. The same
  test with the block collapsed before the delete completes covers the unmounted-control path.
- Saving the connection while its first schema read is still in flight leaves the pre-save response as
  the page's schema: measured, one request and the old answer. The test must start a fresh request and
  then resolve the abandoned one, which must not replace the new schema.
- Starting a delete with no editor open and opening the editor for that connection before the request
  completes leaves that editor mounted after the delete succeeds, and re-rendering re-creates the
  deleted connection's schema entry and requests it.
- Collapsing the block, reopening it and opening an editor before the delete completes does the same,
  the callback having been lost with the unmounted section. An editor open for a different connection
  must survive both.

Guards, green before and after, to be described as guards rather than as proof of the fix:

- A rule that adds a derived name reaches the column picker without a remount.
- A refresh superseded by a resource switch does not append the older rows.
- Sort, column filters and selection survive a refresh that removes no column they name.
- A sort or filter on a column a save removes stops acting without being cleared.
- A connector's own derived column (`item_url`, `location_url`) stays hidden by default.
