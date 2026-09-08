## Context

See `proposal.md` — Why. The state this design starts from:

- `ui/src/pages/connect/ConnectionsSection.tsx` (≈700 lines) holds five components: the list section
  with the **Default connection** control, `ConnectionRow` (which owns the confirm-then-delete flow),
  `ConnectionForm` (a two-line switch), `CreateConnectionForm` and `EditConnectionForm` (which owns
  the rule editor and the preview).
- `ui/src/pages/Connect.tsx:63-88` listens for two `window` `CustomEvent`s, `labeler:connection-saved`
  and `labeler:connection-deleted`, dispatched from the mutations in `ui/src/api/connectors.ts:81,95`.
  They exist for one reason: the form and the Connect page shared a mount and had no common parent
  holding the state a save or a delete had to reach.
- The Connect page's resolution latch is `useState` (`Connect.tsx:34`), so it is already per mount.
  #166's spec described it as latched for a session only because the page never unmounted while an
  operator managed connections.
- Browse rows are fetched imperatively in a `useEffect` keyed on connection, resource, filters, parent
  and `refreshToken` (`ConnectorBrowser.tsx:218-243`); they are in no query cache, so a fresh mount
  always browses fresh. The connections list, the connector schema and the settings *are* in the
  react-query cache, with the client's default `staleTime` of 0. That default means "refetch on
  mount", not "do not serve the held answer": TanStack returns the cached data immediately and
  refetches behind it, so an invalidated query still answers with the pre-write copy until the
  replacement lands.
- **Cancel** stays enabled while a save is in flight (`ConnectionsSection.tsx:527`), and the primary
  navigation is never disabled, so an operator can leave the form before a write completes.
  Unmounting the form does not cancel the mutation or stop its success handler running.
- `GET /api/connections` returns `transforms` in full, so the list alone carries everything the editor
  needs to pre-fill.

## Goals / Non-Goals

**Goals:**

- Three routes and two page components, with the list and the form never mounted together.
- Delete every mechanism that existed only to bridge the two views, rather than repointing it, without
  losing the guarantee it carried: what Connect presents must agree with every write this browser has
  completed, including one that completes after the operator has left the form.
- Keep the field-transform editor and its preview byte-for-byte in behavior; only its container moves.

**Non-Goals:**

- Any Rust change. No endpoint, request body, response body or stored schema is touched.
- Preserving the Connect page's browse context across the trip to `/connections`. The issue chose the
  round trip as the cheaper problem; pretending otherwise would mean reintroducing the coupling this
  change removes.
- A test-connection action.

## Decisions

**One file becomes two pages under `ui/src/pages/connections/`.** `ConnectionsList.tsx` takes the
table plus the **Default connection** control; `ConnectionForm.tsx` takes the create and edit forms,
the rule editor and the delete action. Alternative considered: keep `ConnectionsSection.tsx` where it
is and render it from two routes. Rejected — the component's whole shape is "list with an editor
folded into it", which is the thing being removed; splitting is what makes "the list is never rendered
beside the form" structural rather than a rule someone must remember.

**Both window events are deleted, not repointed.** A `CustomEvent` on `window` is same-document only,
so it never crossed tabs; with the views on separate routes it has no consumer left in this one.
Alternative: keep them for a future cross-view need. Rejected — no such need exists, and an event with
no listener is a mechanism nobody can test. What replaces them is not "nothing": it is the two rules
below, which are properties of the cache and of the Connect page rather than of one form remembering
to shout.

**A successful write evicts, it does not invalidate.** `useSaveConnection` currently invalidates
`["connections"]` and `["connector-schema", id]`; `useDeleteConnection` invalidates `["connections"]`
and `["settings"]`. Invalidation marks a held answer stale and refetches it, but TanStack keeps
serving the stale copy until the replacement lands, and Connect resolves the moment
`connections !== undefined` (`Connect.tsx:43`) — which a stale copy satisfies. Because the resolution
latches once per visit, resolving from a pre-write copy strands the operator on it for the whole
visit: create the first connection from Connect's empty-state link and you can return to the cached
empty list, latch "nothing resolved", and stay unselected after the refetch lands. So every successful
connection-management write `removeQueries` exactly the answers the spec says it affects, leaving
nothing to serve: a create removes `["connections"]`; an update removes `["connections"]` and
`["connector-schema", id]`; a delete removes those two and `["settings"]`, the server having possibly
cleared `default_connection_id`; a default write or clear removes `["settings"]` alone. A write that
fails removes nothing. The mapping is per write rather than "evict everything" so that the freshness
rule stays provable: the spec permits reusing an answer no completed write affects, and a blanket
eviction would make that permission untestable while reloading the connections list after every
default change. Alternatives: `await`ing the invalidation before navigating, rejected because a failed
refetch still leaves the pre-write copy to be served; and writing the mutation's response into the
cached list with `setQueryData`, rejected because inserting correctly means duplicating the published
total order (`name`, ties by `id`) in the client, where it would drift.

**The default-connection write and clear become their own keyed mutations.** They are
connection-management writes by the spec's definition, so they must both evict `["settings"]` and
count towards the gate below; the shared `useUpdateSetting` / `useResetSetting` do neither, having no
`mutationKey` and only invalidating. `useSetDefaultConnection` and `useClearDefaultConnection` sit
beside the connection mutations, hit the same `/settings/default_connection_id` endpoints, and carry
the same `mutationKey` and the same hook-level eviction. Alternative: add the key and the eviction to
the shared hooks. Rejected — it would gate the Connect page on every unrelated settings write and
would make the Settings page reload its settings after each one, which is a behavior change to a page
outside this change.

**Cache maintenance is durable; navigation is the form's.** They cannot share a handler, because they
need opposite lifetimes. A `mutate()` call-site `onSuccess` does not run when its component has
unmounted, which loses the eviction in exactly the case the eviction exists for; a hook-level
`onSuccess` always runs, which would drag an operator who had already navigated to Connect off to
`/connections` when their delete landed — contradicting the pending-delete scenario that has them
proceed on Connect. So eviction and the `mutationKey` live in the hook's `onSuccess` in
`connectors.ts`, and only `navigate(...)` stays in the form's `mutate()` callback. TanStack's
skip-after-unmount rule for call-site callbacks is then not an obstacle but the mechanism: it is what
makes "only a form that is still the active view moves the operator" true without the form tracking
its own mountedness. The spec states that as a contract rather than leaning on the library silently.

**The editor subtree is keyed, because navigation alone does not unmount it.** The paragraph above is
only true if leaving an entry really unmounts its form, and React preserves state for a component
rendered at the same position with the same type: navigating `/connections/{b}` → `/connections/{a}`
re-renders one `ConnectionForm` with a new prop, it does not replace it. The existing edit form
initializes every draft from `initial` once (`ConnectionsSection.tsx:106`) and today depends on the
caller's explicit `key` (`ConnectionsSection.tsx:630`) for exactly this reason; on the route that
caller is gone. So the route component is a shell that renders the draft-owning subtree under
`key={`${useLocation().key}:${id ?? "new"}`}`, and everything that must not travel between entries —
the field drafts, the rule editor and its revision counters, the preview results, the validation
messages and the `useMutation` observer — lives inside it. `location.key` is what makes back and
forward between two entries for the *same* id a fresh editor too, which the id alone would not.
Alternative: reset the drafts from an effect on the id. Rejected — it reproduces the derived-state bug
class the key exists to avoid, and it would not discard the in-flight mutation observer, which is the
half that navigates.

Regression to plan for it: navigate `/connections/{a}` → `/connections/{b}`, edit and save `b` against
a deferred response, navigate back to `a`, then resolve the save. Assert `a`'s stored values with none
of `b`'s draft, that the location did not change when the save landed, and that the affected queries
were evicted.

When the form is still mounted, the hook's eviction and the form's navigation run in the same mutation
resolution and batch, so the form's `useConnections()` never renders its own loading or not-found
branch on the way out.

**Connect acts on nothing while a connection-management write is in flight.** Separate routes stop a
write from *starting* beside Connect; they do not stop one from *finishing* there. **Cancel** stays
enabled while a save is pending (`ConnectionsSection.tsx:527`) and the primary navigation is always
available, so an operator can reach Connect mid-write; unmounting the form does not cancel the
mutation or stop its hook-level success handler. Connect would then latch, read a schema and browse rows from
pre-write answers, and the eviction that arrives afterwards cannot undo the latch or retrigger the
browse (`ConnectorBrowser`'s effect keys on connection, resource, filters and parent — a new schema
object does not re-browse). The fix is a gate, not a signal: the mutations get a `mutationKey` of
`["connection"]` — the four of them, the connection save and delete and the default write and clear —
and Connect renders a waiting state and resolves nothing while
`useIsMutating({ mutationKey: ["connection"] })` is non-zero. Alternatives: blocking navigation while
a write is pending, rejected because `useBlocker` needs a data router and this app mounts
`<BrowserRouter>`, and because it would still not cover the operator who leaves by **Cancel**;
reinstating a refresh signal, rejected because it rebuilds the coupling this change removes and
because a gate is checkable from one place while a signal has to be remembered at every call site.

**Origin travels as router state, guarded, and the list page relays it.** Both Connect links carry
`state: { from: "/connect" }` — the one to `/connections` and the empty-state one straight to
`/connections/new`. `/connections` reads its own `location.state.from` and puts it on its **Add
connection** and **Edit** links, so the ordinary path (Connect → list → editor) returns to Connect;
reached from primary navigation it has no origin to relay and its forms return to `/connections`. Save
and cancel navigate to the origin when it is a string beginning with a single `/`, and to
`/connections` otherwise. Without the relay, only the empty-state shortcut would honour the issue's
"saving from a form reached via Connect lands back on `/connect`", since every operator with at least
one connection reaches the editor through the list. Alternatives: `navigate(-1)`, rejected because a
save that follows a cancel-then-reopen walks history backwards to somewhere arbitrary and because a
cold load has no entry to go back to; a `?from=` query parameter, rejected because it is user-editable
and would need the same guard while also being visible in the URL. The single-leading-slash guard is
what keeps a value that reached the state from outside the application (a crafted `//evil.example.com`
is a protocol-relative URL) from deciding where the operator lands.

**Delete moves from the row to the editor.** The issue's acceptance criteria put creating, editing,
deleting and enabling/disabling on `/connections/new` and `/connections/:id`, which leaves the list a
list. The confirm-then-delete two-step moves with it unchanged. A successful delete always goes to
`/connections`, ignoring the recorded origin, because the connection the origin was reached for is
gone. **Assumption**: the issue says "deleting … work[s] from `/connections/:id`" and separately
"Deleting a connection returns to `/connections`"; both read as delete living in the editor, and a
delete on the list page would have nowhere to "return" to.

**Not-found is decided from the connections list, not from a per-id request.** The route reads
`useConnections()` and looks the id up. `GET /api/connections/{id}` would answer `404` directly, but
the list is fetched anyway to pre-fill the form with `transforms`, so the per-id call is a second
request and a second loading state for an answer the first already carries. Deciding from the list
also inherits the discipline already written for the dangling default: "unknown" is only knowable once
the list has *loaded*, so pending renders neither state and a failed fetch reports the failure rather
than claiming the id does not exist.

**The resolution latch keeps its shape and gains a precondition.** `latchedConnectionId` is already
per mount, and the route split is what turns that into "per visit"; no new state and no reset logic is
added. What does change is when it may fire: the existing condition
(`connections !== undefined && (settings !== undefined || settingsFailed)`) is joined by "and no
connection-management write is in flight". Because the latch runs once, a wrong input is permanent for
the visit, which is why the two rules above are preconditions on it rather than corrections after it.
The consequence — a connection created while away can be what resolves on return, and a hand-picked
selection is not restored — is written into the spec rather than special-cased, because a connection
carries no property saying the operator made it a moment ago.

**`ConnectorBrowser`'s `refreshToken` prop goes.** It exists solely to let the saved-event handler
force a re-browse; with the event gone its only caller passes nothing, and a prop that is always
`undefined` is a dead branch in the effect's dependency list.

**The form is a single column with `<section>` per titled group.** **Details** (connector, name, base
url, public url, api key, enabled) then **Field transforms**, constrained to a readable measure rather
than stretched to the viewport. This is the part the issue is actually about: the current
`flex flex-wrap gap-3` row of five `flex-1` inputs is what produces the ragged shape, and a page with
one column per row is what the cited guidance (Carbon, Polaris, NN/g) prescribes for a form this size.

**A third delta, on `connector-field-transforms`.** The issue names two spec deltas. Its own decision
makes a third requirement false: "A saved rule shows on the open Connect page without a reload"
promises a save reaches Connect "without a reload and without remounting it", which cannot hold once
the rule editor is on another route. Leaving it published would leave a contract nothing can satisfy,
so it is retired and restated in terms of the next visit. No field-transform semantics change.
**Assumption recorded**: this is a consequence of the issue's decision, not scope added to it.

**The only-disabled-connections case links to `/connections`.** The issue specifies the call to action
for a list that loads *empty*. For a list that holds only disabled connections the page has the same
dead end for a different reason, and the general link to `/connections` the page now carries covers
it; no second call to action is introduced.

## Risks / Trade-offs

- **An operator loses their browse context on every trip to manage a connection.** → The origin state
  puts them back on `/connect` in one hop, and the resolution reselects a connection without a click.
  The row selection and the rows are gone, which is the cost the issue accepted when it called the
  round trip the cheaper problem. It is written into the spec so it is a decision, not a surprise.
- **A newly created connection can become what resolves on return**, where the old spec forbade
  selecting a just-created connection on the operator's behalf. → The old rule protected a live
  selection from moving underneath someone; there is no live selection to protect across an unmount,
  and a reload has always behaved this way. Stated explicitly in the spec with a scenario.
- **Test churn.** `Connect.test.tsx` and `ConnectionsSection.test.tsx` both carry substantial coverage
  keyed to the block. → The list and form tests move with the components; the Connect tests lose the
  block's cases and gain the link, the empty-state call to action and the return-and-re-resolve case.
  `Shell.test.tsx` gains the nav item.
- **`removeQueries` on save touches queries the open editor is subscribed to**, putting them into a
  pending state. → The hook's eviction and the form's navigation resolve in the same commit, so
  nothing renders the editor's loading or not-found branch on the way out. Worth a test that a save
  from the editor issues no schema request for the connection after the navigation.
- **The key makes every entry a remount**, so an operator who opens the same connection twice loses an
  abandoned draft rather than finding it waiting. → That is the stated contract, and the alternative
  is worse: a draft that survives navigation belongs to a connection the operator may no longer be
  looking at. Nothing here persists a draft deliberately, so nothing is lost that was promised.
- **Splitting eviction from navigation puts the two halves of one completion in two files**, and a
  later reader may "tidy" the navigation back into the hook. → The spec states both lifetimes and why
  they differ, and the two scenarios that pin them (a completion moves nobody who has left; the
  eviction happens all the same) fail if they are recombined either way.
- **Two default-connection mutations now exist beside the shared settings hooks**, and a future
  default-writing control could reach for the wrong pair. → The `/connections` control is the only
  writer of `default_connection_id` the UI has, which `default-connection` already requires; the
  keyed hooks live next to the connection mutations rather than among the settings ones, where the
  reason for their existence is visible.
- **After a delete the `/connections` list shows a loading state**, where invalidation would have kept
  the old rows on screen during the refetch. → That is the intended trade: the old rows include the
  connection just deleted, and showing them back is the defect. The same applies to the
  default-connection control after a delete, which the published contract requires to show no default
  without a reload.
- **The Connect gate can hold the page on a slow write.** → It holds only for as long as the request
  is actually in flight, and it reports that it is waiting rather than showing a stale page. A failed
  write releases the gate and evicts nothing, so the page proceeds on answers the failure did not
  change.
- **`useIsMutating` keys on a string the mutations must both carry.** A `mutationKey` typo makes the
  gate silently never fire, and the page would look correct in every test that does not race a write.
  → The scenarios for leaving during a pending save and a pending delete are the ones that catch it;
  they must assert the waiting state, not merely the eventual outcome.

## Open Questions

None.
