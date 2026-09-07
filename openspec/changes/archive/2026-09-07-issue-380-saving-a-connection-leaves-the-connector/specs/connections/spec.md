## ADDED Requirements

### Requirement: Saving or deleting a connection refreshes what the open Connect page holds

The Connect page holds two answers that a change to the connection can invalidate: the connector
schema it read for the selected connection, and the browse rows it fetched at some earlier moment.
Both SHALL follow the connection.

Saving a connection SHALL cause every consumer of that connection's schema to re-read it, so a saved
change is offered without a reload and without remounting the page. This SHALL hold for every save,
whichever control performed it and whatever the save changed: it is a property of saving, not of one
form remembering to ask. A read of that schema already in flight when the save succeeds SHALL
NOT become what the page holds: it was answered before the save, and the page SHALL end up with a
schema read after it.

Saving the connection currently being browsed SHALL re-browse the rows on screen, for the resource,
the connection filters and the drill-down parent in force at that moment. **The trigger is the save,
not what the save changed.** A save can point the connection at a different upstream, or replace the
credential it authenticates with, and the second of those is not visible in any response the page can
compare: `has_credential` says a key is set, never which one. Rows fetched from the previous upstream
SHALL NOT stay on screen under a schema read from the new one.

Neither refresh SHALL depend on the control that started it still being on screen. The operator may
collapse the Manage connections block, or close the form, while the request is still in flight; the
save completes either way, and what it re-reads and what it refreshes SHALL be what they would have
been had the operator waited.

A refresh SHALL be dropped rather than shown if a later request supersedes it: if the operator switches
resource, drills in, clears a parent or applies a filter while a refresh is in flight, the refresh's
rows SHALL never reach the table.

A refresh is not a change of browsing context. The active sort, the column filters, the visible column
set and the row selection SHALL survive it, except where the saved change removed the column they name,
which `connector-browser` governs. The refresh SHALL start from the first page; rows beyond it are
reloaded by the operator, their cells being as stale as the first page's were.

Deleting a connection SHALL leave no held schema for it and SHALL issue no request for it. That rests
on the page having retired the deleted connection's selection at the delete, which `default-connection`
requires, rather than on the connections list reporting the absence later.

An editor open on the deleted connection SHALL close at the delete on the same terms, whatever was
open when the delete started, and whether or not the Manage connections block was collapsed and
reopened while the request was in flight. An editor open on a different connection SHALL stay open: it
names a connection that still exists.

#### Scenario: A save that changes no rule still re-reads the schema

- **WHEN** the operator saves a connection having changed only its name
- **THEN** that connection's schema is requested again

#### Scenario: A save that points the connection at another upstream replaces the rows

- **WHEN** the operator saves the connection being browsed with a different base URL, changing no rule
- **THEN** the rows on screen are replaced by rows browsed from the new upstream, with no reload

#### Scenario: The Manage connections block is collapsed before the save completes

- **WHEN** the operator saves the connection being browsed and collapses the Manage connections block
  before the request completes
- **THEN** the connection's schema is re-read and the rows on screen are refreshed

#### Scenario: Saving another connection does not disturb the browse table

- **WHEN** the operator saves a connection other than the one being browsed
- **THEN** no browse request is made

#### Scenario: A refresh superseded by a resource switch is dropped

- **WHEN** the operator switches to another resource while a save's refresh is still in flight
- **THEN** the refresh's rows are discarded, and the table shows the newly selected resource's rows
  alone

#### Scenario: The browsing context survives the refresh

- **WHEN** a save refreshes the rows while a sort, a column filter and a row selection are in force,
  and the saved change removes no column they name
- **THEN** the sort, the column filter, the visible columns and the selection are all still in force
  afterwards

#### Scenario: A save while the first schema read is still in flight

- **WHEN** the operator saves the connection being browsed while the first read of its schema has not
  yet answered
- **THEN** a fresh schema read is made
- **AND** the earlier read answering afterwards does not become the page's schema

#### Scenario: A deleted connection's schema is not kept

- **WHEN** the operator deletes a connection whose schema the page had read
- **THEN** no schema for that connection is still held, and no schema request is made for the deleted
  id

#### Scenario: An editor opened after the delete starts still closes

- **WHEN** the operator confirms deleting a connection with no editor open, opens the editor for that
  same connection before the request completes, and the connections list refetch is delayed or fails
- **THEN** that editor closes when the delete succeeds
- **AND** re-rendering makes no request for the deleted connection's schema and leaves none held

#### Scenario: The block is collapsed and reopened before the delete completes

- **WHEN** the operator confirms deleting a connection, collapses the Manage connections block,
  reopens it and opens an editor before the request completes, with the connections list refetch
  delayed or failed
- **THEN** an editor open on the deleted connection closes, and one open on another connection stays
  open
- **AND** re-rendering makes no request for the deleted connection's schema and leaves none held
