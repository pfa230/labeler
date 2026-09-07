## Context

See proposal.md for motivation.

Three facts about the current code shape this design.

`ConnectionsSection` (`ui/src/pages/settings/ConnectionsSection.tsx`) is self-contained: it owns its
form state, reads `useConnections`, `useSettings`, `useSaveConnection`, `useDeleteConnection` and
`useToast`, and takes no props. `Settings.tsx` renders it as one line among seven sections.

`Connect.tsx` already reads `useConnections` and `useSettings` on the same react-query keys, and
`useSaveConnection`/`useDeleteConnection` already invalidate `["connections"]` (`ui/src/api/connectors.ts:70,79`),
with delete also invalidating `["settings"]`. Both pages have therefore always shared one cache; what
cost the round trip was the navigation, not the data.

`Connect.tsx` resolves its connection once through a render-time latch
(`Connect.tsx:35-50`): `latchedConnectionId` starts `null`, is written during render the first time the
list and settings are both readable, and is never recomputed. `selectedConnectionId` overrides it, with
`null` meaning "no manual choice yet". The distinction matters below: clearing a selection means writing
`""` to `selectedConnectionId`, not `null`, which would fall back to the latch.

## Goals / Non-Goals

**Goals:**

- One page for browsing, printing and configuring connections.
- No new state shared between the block and the page: the connection list stays a single cache entry.
- The Connect page's existing contract (the latch, the connection-scoped reset) survives intact except
  where the specs say it changes.

**Non-Goals:**

- Restructuring `ConnectionsSection` itself. It moves; its internals are not rewritten.
- Any backend change. No endpoint, request body, response body or stored value is touched.
- Persisting the disclosure state, per-user or otherwise.

## Decisions

**The block sits directly below the pickers, above the composer and the browse table.** The connection
picker and the default-connection control are the same list read two ways, so the management block
belongs next to the picker it configures; and in the state that motivates the change, an installation
with no connections, nothing else renders, so the block lands directly under the picker where the
operator is already looking. The alternative, immediately above the browse table and below the
composer, puts a configuration block inside the print flow and separates it from the picker, and buys
nothing: the composer only exists once a connection is already chosen.

**The disclosure is a one-shot latch, not a derived value.** `open` starts `null`; the first render in
which the connections list has loaded resolves it to `connections.length === 0`, and thereafter only the
operator moves it. Deriving `open` from `connections.length === 0` would be shorter and wrong: the
operator could not collapse the block on an empty installation, and the block would slam shut the
instant they added their first connection, mid-form. The shape mirrors `latchedConnectionId` in the same
file, so the page has one idiom for "resolve once from data that arrives late" rather than two.

A list that is still loading or has failed leaves `open` at `null`, which renders collapsed. That is
deliberate: "expanded" is an answer to "this installation has no connections", and neither state
answers it. The operator can still open the block by hand, which resolves the latch and stops it firing
later.

**The selection clears when the connections list stops offering it as enabled, and not otherwise.** The
issue names deletion; disabling produces the identical dangling shape, because the picker lists only
enabled connections (`Connect.tsx:69`), so a disabled selection leaves a `<select>` whose value matches
no option while the browse table below it keeps working against that connection. Writing the rule
against deletion alone would need a second rule for disabling, or would leave the second case broken;
one predicate, "the loaded list contains this id and it is enabled", covers both and has no state a
reader must keep in their head. The check runs during render next to the existing latch, guarded by the
list having actually loaded, so an in-flight refetch or a failed one clears nothing.

Clearing resets `selectedConnectionId` to `""` and defensively calls `setSelected([])`. The
`key={connectionId}` on the composer and the browse table unmounts the state those components own, while
`selected` lives in `Connect` (`Connect.tsx:37`) and is passed down (`Connect.tsx:125,138`). Although the
manual picker's `onChange` also clears `selected` (`Connect.tsx:83`), clearing `selected` alongside
`selectedConnectionId` ensures that whenever the page transitions to an unselected connection, `selected`
is kept empty locally at the site of the transition, upholding the invariant from
`specs/default-connection/spec.md` ("No row selected against one connection SHALL ever be
presented or materialized against another"). The test verifies the delta's "Rows selected against a
cleared connection do not come back" scenario: select two rows on `c1`, delete `c1` in the block, pick
`c2`, and assert the selection summary is empty and adding rows adds only `c2`'s.

**A newly created connection is not selected for the operator.** The picker's options follow the list,
because both read one cache entry, so an enabled connection shows up there and a disabled one does not,
and the latch keeps the selection where it was either way. "Appears in the picker" is therefore a claim
about enabled connections only: a connection saved with **enabled** cleared appears in the block's table
and nowhere else, which is what the picker has always meant by offering a connection
(`Connect.tsx:69`). Auto-selecting would be a second
way the selection moves without the operator touching the picker, in a requirement whose whole point is
that it moves exactly one way; and on an installation that already has connections it would yank a
working operator onto a connection they just configured for someone else. The cost is one click after
adding a first connection, on a page that until then had nothing to click at all.

**The moved requirements land as REMOVED plus ADDED, not RENAMED or MODIFIED.** Both change name and
content: they name a page that no longer holds them. `archive-merge-check.sh` resolves requirements by
name and checks each landed verbatim or is gone, so a rename plus an edit is two operations against one
name. The repo already took this route for the same reason (`openspec/changes/archive/2026-09-01-issue-291-one-colour-type/design.md:162`).

**`connector-browser`'s reference to "the Settings > Connections form" is left alone.** Its closing
paragraph (`openspec/specs/connector-browser/spec.md:324`) scopes what *that* requirement supersedes:
everything else in §12 is unchanged by it. It already coexists with `connections` superseding the same
paragraph's field list, so the sentence has never meant "and nothing else may supersede this". Editing
it would drag a third capability into the change to restate a scoping clause that stays true.

**The component moves by `git mv`, imports unchanged.** `ui/src/pages/connect/` is the same depth as
`ui/src/pages/settings/`, so every `../../` import in the component and its test still resolves. The
change to each file is its own header comment and nothing else; the two edited files are `Settings.tsx`
(one import, one line) and `Connect.tsx`.

## Risks / Trade-offs

**Existing Connect tests now mount the section, and their picker query is ambiguous.** → Thirteen call
sites look the picker up with `findByLabelText(/connection/i)` (`Connect.test.tsx:94` and twelve more),
and the moved control is labelled `default connection`
(`ConnectionsSection.tsx:319-324`), which that pattern matches. Every one of the thirteen changes to an
exact `/^connection$/i`. This is not conditional on the block being collapsed: whether a closed
disclosure's contents are in the DOM is an implementation detail of how it is hidden, the block starts
expanded on an empty list, and any new test that opens it would break a query written to depend on it
being shut. Queries for the block's own controls go the other way, scoped with `within` to the block or
named exactly (`/^default connection$/i`), so neither set can start matching the other's.

**The block adds a row of vertical space above the working area.** → It is collapsed in every state but
the empty one, so the cost is one disclosure line on the page that was already the operator's
destination.

**Disabling the selected connection discards the row selection.** → It is the same reset that already
happens on every selection change, and the alternative is a picker showing a connection it refuses to
offer. An operator who disables a connection to edit it and re-enables it re-picks it and re-selects
rows; the specs say what happens rather than leaving it to whichever component notices first.

**Settings loses its only reader of `GET /api/connections`, and a DOM-absence check does not prove it.**
→ Dropping the `/api/connections` branch from `Settings.test.tsx`'s stub is not enough: the request goes
through React Query (`ui/src/api/connectors.ts:59-60`), which turns a rejected `queryFn` into ordinary
error state that a section renders as text (`ConnectionsSection.tsx:243,290-293`), so a lingering query
would fail no assertion the current tests make (`Settings.test.tsx:42-63`). The test keeps the stub as a
`vi.fn` spy, waits for the page to settle on something it does render, and then asserts that no call in
`fetch.mock.calls` targets `/api/connections`. That assertion fails if the section is still mounted, and
is the only one here that can.

## Migration Plan

None. Pre-1.0, a change that alters behavior breaks what came before: the connections UI is on
`/connect` and nothing stands in for it on `/settings`. No stored data, endpoint or response shape
changes, so an upgrade needs nothing but the new build.
