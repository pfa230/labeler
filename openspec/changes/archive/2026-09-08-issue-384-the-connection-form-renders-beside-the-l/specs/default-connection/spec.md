## ADDED Requirements

### Requirement: Connections names the default connection

The `/connections` page SHALL let an operator choose which connection is the default and clear that
choice, without typing an id. The control SHALL offer the existing connections by name, SHALL show
which one is currently stored, and SHALL offer a "no default" choice that clears the setting. The
control SHALL state that the default applies to everyone, because `default_connection_id` is
instance-wide.

Because connection names are not unique, an entry SHALL carry enough to tell two identically named
connections apart. A connection that is disabled SHALL be marked as such, so an admin choosing one is
not surprised when Connect falls through to the fallback.

When the stored id names no connection, the control SHALL show an explicit unavailable state naming the
stored id, rather than showing "no default" or an arbitrary connection, and that state SHALL still be
clearable. That state is only reachable once the connections list has loaded: while the list is still
loading or has failed, the control SHALL NOT report a stored default as unavailable, because neither
says the connection is gone.

After a connection is deleted, the control SHALL show the resulting state without a page reload:
deleting the connection that was the default SHALL leave the control showing no default, on the
`/connections` page the delete returns to.

The control SHALL sit with the connections list, whose entries are its options, and SHALL move with
it: it is on `/connections`, and on neither the Connect page nor the Settings page. It SHALL NOT
change the connection form or the connections table, whose contract stays the one `connections`
states. Writing the default SHALL write the setting and nothing else: it SHALL NOT alter what the
Connect page is working on, and it takes effect there on that page's next visit, under the resolution
order below.

This requirement supersedes nothing in `docs/SPEC.md`, which does not describe a default connection.

#### Scenario: Choosing a default

- **WHEN** the operator picks a connection in the default-connection control on `/connections`
- **THEN** `PUT /api/settings/default_connection_id` is sent with that connection's id
- **AND** the control shows that connection as the stored default

#### Scenario: Clearing the default

- **WHEN** the operator picks the "no default" choice
- **THEN** `DELETE /api/settings/default_connection_id` is sent
- **AND** the control shows no stored default

#### Scenario: No default stored

- **WHEN** the operator opens `/connections` with no default stored
- **THEN** the control shows the "no default" choice as selected

#### Scenario: Two connections share a name

- **WHEN** two connections are both named `Homebox`
- **THEN** the control presents them distinguishably, so the operator can tell which one they picked

#### Scenario: A disabled connection in the control

- **WHEN** a connection whose `enabled` is `false` appears in the control
- **THEN** it is marked as disabled

#### Scenario: The stored default names no connection

- **WHEN** the operator opens `/connections`, the connections list loads, and
  `default_connection_id` holds an id no connection has
- **THEN** the control shows an unavailable state naming that id, and the operator can still clear it

#### Scenario: The connections list has not answered yet

- **WHEN** the operator opens `/connections` with a stored default and the request for the connections
  list has not answered or has failed
- **THEN** the control does not report the stored default as unavailable

#### Scenario: Deleting the default connection

- **WHEN** the operator deletes the connection that is the stored default, from its form
- **THEN** they land on `/connections` and the control shows no default, without a page reload

#### Scenario: Naming a default does not move the Connect page

- **WHEN** the operator names a connection as the default on `/connections` and returns to the Connect
  page
- **THEN** the Connect page resolves under the order below, from a settings read made after that
  write, and the naming wrote nothing but the setting

### Requirement: Connect opens on a connection it resolves per visit

The Connect page SHALL resolve a connection to open on, rather than starting with none selected. It
SHALL resolve, in order:

1. the connection named by `default_connection_id`, when that connection exists and is enabled;
2. otherwise the first enabled connection in the order `GET /api/connections` returns, which is a
   total order (see the `connections` capability), so the same installation resolves the same
   connection on every visit;
3. otherwise nothing, when no connection is enabled.

**The resolution is latched for the visit.** The page SHALL resolve once and SHALL NOT re-resolve
while the operator stays on it. A later refetch of the connection list or the settings, by a
window-focus refresh or by another operator changing the instance-wide setting, SHALL NOT change what
is selected underneath the operator.

**What it latches on SHALL be current.** Because the resolution never runs twice, resolving from a
copy of an input taken before a successful write that changed it would strand the operator on that
copy for the whole visit, with no later refetch able to correct it. So the page SHALL resolve only
once the connection list and the settings have each either answered or failed **and** no
connection-management write is in flight at all, and each of the two SHALL be the answer to a request
started after the latest successful write affecting it. An input that no completed write affects, and
one held across a write that failed, MAY be the copy already held; `connections` ("A connection write
settles before Connect acts on it") fixes which writes affect which answers and states how both hold.
Until then the page SHALL report that it is waiting and SHALL select nothing. A connection the
operator created a moment ago is therefore in the list the page resolves from, whether they saved it
from this page's call to action or from `/connections`, and a default they just named is the one it
resolves to.

**A visit ends when the operator leaves the page.** Connection management is a separate route, so
going there and coming back is a new visit: the page resolves again, by the order above, and SHALL NOT
restore a connection the operator had picked by hand, nor its browse rows, its row selection or its
composer. A connection created, renamed, enabled, disabled or deleted while the operator was away is
therefore offered by the picker, or not, according to what it now is, and can itself be what resolves:
a connection has no property saying the operator made it a moment ago, and the resolution order is
applied whole on every visit. This is what a reload has always done, and #384 made the round trip
explicit rather than pretending a saved connection could reach a page that is not mounted.

**The selection SHALL NOT outlive what it names.** When a loaded connections list no longer offers the
selected connection as an enabled connection, whether because it was deleted or because it was
disabled, the page SHALL clear the selection and return the picker to its "choose a connection" state.
A connections list that is still loading or that failed to load SHALL clear nothing, because neither
says the connection is gone.

Whenever the selected connection changes, every piece of connection-scoped state SHALL reset together:
the row selection, the browse table's rows and browsing context, and the composer. No row selected
against one connection SHALL ever be presented or materialized against another. Clearing the selection
is such a change, so a cleared selection SHALL leave no browse table, no row selection and no composer
behind, and the rows selected before it was cleared SHALL NOT reappear when the operator picks another
connection.

A resolved connection SHALL be selected in the connection picker as though the operator had chosen it,
which SHALL load its schema and the browse table's first page of rows without any click. Opening the
Connect page therefore issues a request to the upstream system the resolved connection points at.

When nothing resolves, the page SHALL present the connection picker and its link to `/connections`,
and none of the sections below them. When the connections list has loaded holding no connection at
all, that link SHALL be the call to action `connections` requires, pointing at `/connections/new`.

When the settings read fails, the page SHALL resolve as though no default were stored, using the
fallback, rather than waiting indefinitely or leaving the operator on an empty page. A failure to load
the connection list resolves nothing, because there is then nothing to select.

Changing the picker by hand SHALL keep working unchanged, including clearing the row selection, and
SHALL NOT write `default_connection_id`: the default is written only by the default-connection control,
never as a side effect of choosing a connection to work with.

This requirement supersedes one clause of the frozen `docs/SPEC.md` §12 ("Using a connection (UI)"):
the statement that the Connect page's flow begins by picking a connection. The rest of that paragraph
is untouched here, and parts of it are already superseded elsewhere: the browse table by
`connector-browser`, and where connections are added and edited by the `connections` capability's
"Connection management has its own route" and "The connection form is a page of its own".

#### Scenario: A stored default resolves

- **WHEN** `default_connection_id` names an enabled connection and an operator opens Connect
- **THEN** that connection is selected in the picker
- **AND** its schema is loaded and the browse table has requested its first page of rows

#### Scenario: No default stored, several connections enabled

- **WHEN** no `default_connection_id` is stored and an operator opens Connect
- **THEN** the first enabled connection in the listed order is selected

#### Scenario: The stored default is disabled

- **WHEN** `default_connection_id` names a connection whose `enabled` is `false`, and another
  connection is enabled
- **THEN** the first enabled connection is selected instead

#### Scenario: The stored default names no connection

- **WHEN** `default_connection_id` names an id no connection has, and a connection is enabled
- **THEN** the first enabled connection is selected instead

#### Scenario: Only disabled connections exist

- **WHEN** no connection is enabled and an operator opens Connect
- **THEN** no connection is selected, the picker shows its "choose a connection" state, and no
  template picker, composer or browse table is shown
- **AND** the page's link to `/connections` is shown

#### Scenario: The settings read fails

- **WHEN** `GET /api/settings` fails and an enabled connection exists
- **THEN** the first enabled connection is selected, exactly as when no default is stored

#### Scenario: A later refetch does not move the operator

- **WHEN** an operator is working on a resolved connection and the connections list or the settings
  are refetched, with the stored default now naming a different connection
- **THEN** the selected connection, the browse table and the row selection are unchanged

#### Scenario: Returning from connection management resolves afresh

- **WHEN** an operator on Connect picks a connection by hand that is not the one that resolved,
  selects rows, follows the link to connection management and returns to Connect
- **THEN** the page resolves by the order above, the hand-picked connection is not restored, and no
  browse rows, row selection or composer from before the trip are left behind

#### Scenario: A connection created while away can be what resolves

- **WHEN** no default is stored, an operator creates an enabled connection that sorts first in the
  listed order, and returns to Connect
- **THEN** that connection is what resolves, being the first enabled connection in the listed order

#### Scenario: Creating the first connection from Connect's call to action

- **WHEN** an operator whose installation has no connection follows the Connect page's call to
  action, creates an enabled connection, and lands back on Connect
- **THEN** that connection is what resolves, and the page does not present itself as an installation
  with no connections

#### Scenario: Leaving the form while the save is still in flight

- **WHEN** an operator saves a new enabled connection and reaches the Connect page, by **Cancel** or
  by primary navigation, before the response arrives
- **THEN** the page selects nothing and reports that it is waiting until the save settles
- **AND** it then resolves from a connections list that holds the new connection

#### Scenario: Naming a default and going straight to Connect

- **WHEN** an operator names a connection as the default on `/connections` and opens the Connect page
  before the setting write has answered
- **THEN** the page waits for that write to settle and resolves on the newly named default, not on
  the one stored before it

#### Scenario: A connection saved as disabled while away is not offered

- **WHEN** an operator saves a connection with **enabled** cleared and returns to Connect
- **THEN** the connection picker does not offer it

#### Scenario: A connection renamed while away is offered under its new name

- **WHEN** an operator renames an enabled connection and returns to Connect
- **THEN** the picker offers it under the new name

#### Scenario: A connection deleted while away is not offered

- **WHEN** an operator deletes a connection and returns to Connect
- **THEN** the picker does not offer it, and the page makes no request for its schema

#### Scenario: Another operator disables the selected connection

- **WHEN** an operator is working on a selected connection, another operator disables it, and the
  connections list refetches
- **THEN** the picker returns to its "choose a connection" state, and no browse table, row selection
  or composer is left behind

#### Scenario: Rows selected against a cleared connection do not come back

- **WHEN** an operator selects rows on a connection, that connection stops being offered as enabled by
  a loaded connections list, and the operator then picks another connection
- **THEN** the new connection's browse table opens with nothing selected, and none of the earlier
  rows are listed in the selection summary or added to the grid

#### Scenario: Changing the picker by hand

- **WHEN** an operator opens Connect on a resolved connection, selects rows, and then picks a
  different connection
- **THEN** the new connection's schema and rows load, the row selection is empty, the composer is
  reset, and `default_connection_id` is unchanged

#### Scenario: A failed connections refetch does not clear the selection

- **WHEN** an operator is working on a resolved connection and a later request for the connections
  list fails
- **THEN** the selected connection, the browse table and the row selection are unchanged

## REMOVED Requirements

### Requirement: Connect opens on a resolved connection

**Reason**: Its resolution latched for a session and its scenarios turn on a form that shared the
Connect page's mount: adding, renaming, disabling or deleting a connection "in the Manage connections
block" while the page stayed put. #384 moved that form to its own route, so the page unmounts for
every one of those acts and resolves again on return. "Connect opens on a connection it resolves per
visit" carries the resolution order, the no-refetch-moves-you rule, the selection-does-not-outlive-what
-it-names rule and the connection-scoped-state reset forward unchanged, and replaces only the clauses
that assumed the two views shared a screen.

**Migration**: None. Pre-1.0, a change that alters behavior breaks what came before. An operator who
had picked a connection by hand and leaves Connect to manage connections returns to the resolved
connection rather than the hand-picked one; picking again is one control.

### Requirement: Connect names the default connection

**Reason**: The control it names sits with the connections list, and that list moved off the Connect
page to `/connections` (#384). Every clause of this requirement locates the control in the **Manage
connections** block, and that block no longer exists. "Connections names the default connection"
restates the whole control contract at its new home, including the unavailable state and the
instance-wide notice, and replaces the one clause that assumed the control and the Connect page's
picker shared a mount.

**Migration**: None. The setting, its endpoints and its stored value are unchanged; only the page
carrying the control moves.
