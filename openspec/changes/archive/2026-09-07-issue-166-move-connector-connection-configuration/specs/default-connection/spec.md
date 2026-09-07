## ADDED Requirements

### Requirement: Connect names the default connection

The Manage connections block SHALL let an operator choose which connection is the default and clear
that choice, without typing an id. The control SHALL offer the existing connections by name, SHALL show
which one is currently stored, and SHALL offer a "no default" choice that clears the setting. The
control SHALL state that the default applies to everyone, because `default_connection_id` is
instance-wide.

Because connection names are not unique, an entry SHALL carry enough to tell two identically named
connections apart. A connection that is disabled SHALL be marked as such, so an admin choosing one is
not surprised when Connect falls through to the fallback.

When the stored id names no connection, the control SHALL show an explicit unavailable state naming the
stored id, rather than showing "no default" or an arbitrary connection, and that state SHALL still be
clearable.

After a connection is deleted, the control SHALL show the resulting state without a page reload:
deleting the connection that was the default SHALL leave the control showing no default.

The control SHALL sit with the connections list, whose entries are its options, and SHALL move with it:
it is on the Connect page and not on the Settings page. It SHALL NOT change the connection form or the
connections table, whose contract stays the one `connections` states. Writing the default SHALL NOT
change which connection the page is working on: the two controls are now on one page, and the picker's
selection answers to the operator alone.

This requirement supersedes nothing in `docs/SPEC.md`, which does not describe a default connection.

#### Scenario: Choosing a default

- **WHEN** the operator picks a connection in the default-connection control
- **THEN** `PUT /api/settings/default_connection_id` is sent with that connection's id
- **AND** the control shows that connection as the stored default

#### Scenario: Clearing the default

- **WHEN** the operator picks the "no default" choice
- **THEN** `DELETE /api/settings/default_connection_id` is sent
- **AND** the control shows no stored default

#### Scenario: No default stored

- **WHEN** the operator opens the Manage connections block with no default stored
- **THEN** the control shows the "no default" choice as selected

#### Scenario: Two connections share a name

- **WHEN** two connections are both named `Homebox`
- **THEN** the control presents them distinguishably, so the operator can tell which one they picked

#### Scenario: A disabled connection in the control

- **WHEN** a connection whose `enabled` is `false` appears in the control
- **THEN** it is marked as disabled

#### Scenario: The stored default names no connection

- **WHEN** the operator opens the Manage connections block and `default_connection_id` holds an id no
  connection has
- **THEN** the control shows an unavailable state naming that id, and the operator can still clear it

#### Scenario: Deleting the default connection

- **WHEN** the operator deletes the connection that is the stored default
- **THEN** the control shows no default without the operator reloading the page

#### Scenario: Naming a default while working on another connection

- **WHEN** the operator is working on a selected connection and names a different one as the default
- **THEN** the selected connection, the browse table and the row selection are unchanged

## MODIFIED Requirements

### Requirement: Connect opens on a resolved connection

The Connect page SHALL resolve a connection to open on, rather than starting with none selected. It
SHALL resolve, in order:

1. the connection named by `default_connection_id`, when that connection exists and is enabled;
2. otherwise the first enabled connection in the order `GET /api/connections` returns, which is a
   total order (see the `connections` capability), so the same installation resolves the same
   connection on every visit;
3. otherwise nothing, when no connection is enabled.

**The resolution is latched.** The page SHALL resolve once, the first time both the connection list
and the settings are available, and SHALL NOT re-resolve afterwards. A later refetch of either, by a
window-focus refresh, by another operator changing the instance-wide setting, or by this operator
adding or editing a connection in the Manage connections block, SHALL NOT change what is selected
underneath the operator. A create or an edit that succeeds SHALL update the picker's options without a
reload, according to what the connection now is: the picker offers the enabled connections, so a
connection saved as enabled SHALL be offered, one saved as disabled SHALL NOT, and one whose name
changed SHALL be offered under the new name. A connection the operator has just created SHALL NOT be
selected on their behalf, whether or not it is enabled. After the initial resolution, the selection
SHALL change only when the operator changes the picker, or when the connection it names stops being one
the picker can offer.

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

When nothing resolves, the page SHALL present the connection picker and the Manage connections block,
and none of the sections below them.

When the settings read fails, the page SHALL resolve as though no default were stored, using the
fallback, rather than waiting indefinitely or leaving the operator on an empty page. A failure to load
the connection list resolves nothing, because there is then nothing to select.

Changing the picker by hand SHALL keep working unchanged, including clearing the row selection, and
SHALL NOT write `default_connection_id`: the default is written only by the default-connection control,
never as a side effect of choosing a connection to work with.

This requirement supersedes one clause of the frozen `docs/SPEC.md` §12 ("Using a connection (UI)"):
the statement that the Connect page's flow begins by picking a connection. The rest of that paragraph
is untouched here, and parts of it are already superseded elsewhere: the browse table by
`connector-browser`, and the connection form by the `connections` capability's "Connection management
on the Connect page" requirement.

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
- **AND** the Manage connections block is shown

#### Scenario: The settings read fails

- **WHEN** `GET /api/settings` fails and an enabled connection exists
- **THEN** the first enabled connection is selected, exactly as when no default is stored

#### Scenario: A later refetch does not move the operator

- **WHEN** an operator is working on a resolved connection and the connections list or the settings
  are refetched, with the stored default now naming a different connection
- **THEN** the selected connection, the browse table and the row selection are unchanged

#### Scenario: Changing the picker by hand

- **WHEN** an operator opens Connect on a resolved connection, selects rows, and then picks a
  different connection
- **THEN** the new connection's schema and rows load, the row selection is empty, the composer is
  reset, and `default_connection_id` is unchanged

#### Scenario: Adding an enabled connection while working on another

- **WHEN** an operator working on a resolved connection adds an enabled connection in the Manage
  connections block
- **THEN** the new connection appears in the connection picker without a reload
- **AND** the selected connection, the browse table and the row selection are unchanged

#### Scenario: Adding a disabled connection

- **WHEN** an operator adds a connection with **enabled** cleared
- **THEN** the connection is listed in the Manage connections block and the connection picker does not
  offer it
- **AND** the selected connection is unchanged

#### Scenario: Renaming a connection while working on it

- **WHEN** an operator edits the name of the connection currently selected and saves, leaving it enabled
- **THEN** the picker offers it under the new name without a reload, and it stays selected

#### Scenario: Deleting the connection being worked on

- **WHEN** an operator deletes the connection currently selected in the picker
- **THEN** the picker returns to its "choose a connection" state, and no browse table, row selection
  or composer is left behind

#### Scenario: Disabling the connection being worked on

- **WHEN** an operator edits the connection currently selected in the picker and clears **enabled**
- **THEN** the picker returns to its "choose a connection" state, and no browse table, row selection
  or composer is left behind

#### Scenario: Rows selected against a cleared connection do not come back

- **WHEN** an operator selects rows on a connection, deletes or disables that connection, and then
  picks another connection
- **THEN** the new connection's browse table opens with nothing selected, and none of the earlier
  rows are listed in the selection summary or added to the grid

#### Scenario: A failed connections refetch does not clear the selection

- **WHEN** an operator is working on a resolved connection and a later request for the connections
  list fails
- **THEN** the selected connection, the browse table and the row selection are unchanged

## REMOVED Requirements

### Requirement: Settings names the default connection

**Reason**: The control it names moved to the Connect page with the connections list whose entries are
its options, so a requirement naming "Settings > Connections" describes a place that does not exist.
Its whole contract is restated at its new home by the ADDED requirement "Connect names the default
connection".

**Migration**: None. The setting, its endpoints and its stored value are unchanged; only the page
carrying the control moved.
