## ADDED Requirements

### Requirement: Each filter group names the scope it acts on

The browse table offers two kinds of filter control with different reach, and it SHALL NOT leave
which is which to be inferred from position.

Where the browsed resource declares server-side filters, the table SHALL present them as one group,
visually delineated from the grid's own controls, and that group SHALL state both that its filters
query the connection and that they restrict the connector's whole result rather than only the rows
already loaded. The group SHALL also make clear that its filters take effect on an explicit apply
action, not as the user types.

The per-column filter controls SHALL be identified, once for the set rather than per column, as
narrowing the rows already loaded and as taking effect as the user types.

Both statements SHALL be available to assistive technology, not carried by placement or proximity
alone.

A resource that declares no server-side filters SHALL present no such group and no statement implying
one, so the table never advertises a reach it does not have.

The two kinds compose: when a server-side filter and a column filter are both set, the table SHALL
show the rows satisfying both, the column filter narrowing within what the connection returned. A
field reachable by both kinds — such as a location, offered both as a connection filter and as a
column — is therefore two controls asking two different questions, and each SHALL be identifiable as
belonging to its own group.

#### Scenario: The connection's filters declare their reach

- **WHEN** a resource declaring server-side filters is browsed
- **THEN** those filters are presented as one delineated group stating that they query the connection,
  restrict the whole result, and take effect on apply

#### Scenario: The column filters declare their reach

- **WHEN** the browse table is shown
- **THEN** the per-column filter controls are identified as narrowing the rows already loaded, as the
  user types

#### Scenario: A resource with no server-side filters shows no such group

- **WHEN** a resource that declares no server-side filters is browsed
- **THEN** no connection-filter group and no statement about connection filters is presented

#### Scenario: The two filter kinds compose

- **WHEN** a server-side filter is applied and a column filter is then typed
- **THEN** the rows shown are those the connection returned for that filter which also match the
  column filter

#### Scenario: The scope of each group is exposed to assistive technology

- **WHEN** the browse table is read by assistive technology
- **THEN** each filter control is associated with the statement of what its group acts on

### Requirement: One control clears every filter the user can see

The browse table SHALL offer a single control that clears the connection's filters and every column
filter together, and that control SHALL say that it clears both rather than name only one of them.

That control SHALL be offered whenever a filter of either kind is set, including when only column
filters are set. Activating it SHALL leave no filter of either kind set, and the table SHALL then
present the resource exactly as it does when no filter has ever been applied.

Clearing SHALL NOT be split so that a control the user reads as "clear the filters" leaves the grid
still narrowed by filters it did not reach.

#### Scenario: One activation clears both kinds

- **WHEN** a connection filter is applied, a column filter is typed, and the clear control is activated
- **THEN** neither the connection filter nor the column filter remains set, and the table shows the
  resource unfiltered

#### Scenario: The clear control is offered for column filters alone

- **WHEN** only a column filter is set, with no connection filter applied
- **THEN** the clear control is offered and clears that column filter

#### Scenario: Clearing leaves nothing narrowing the grid

- **WHEN** the clear control is activated
- **THEN** no filter control retains a value and no row is hidden by a filter

## MODIFIED Requirements

### Requirement: Sorting and filtering act on loaded rows, and the table says so

Sorting and column filtering SHALL act only on the rows already fetched. Neither SHALL request a
differently ordered or differently filtered page from the connector.

Because a connection is paged, the table SHALL disclose that scope rather than let a sorted or
filtered view be mistaken for the whole of the source system. Whenever a column filter is active, the
table SHALL state how many of the loaded rows are shown. Whenever the connection reports that more
rows are available, the table SHALL state that the loaded rows are not the whole result. When a column
filter matches no loaded row while more rows are available, the table SHALL say that explicitly rather
than present an empty table as an answer.

This disclosure describes the sorting and column filtering controls only. It SHALL be placed with the
grid it describes rather than between the two filter groups, and SHALL name those controls, so that it
cannot be read as qualifying the reach of the connection's own filters.

The partial-load statement SHALL be governed by whether more rows remain, not by whether a connection
filter is applied: applying a connection filter narrows the result but still pages it, so the loaded
rows are still not the whole result.

#### Scenario: The table discloses the filtered subset

- **WHEN** a column filter is active over loaded rows
- **THEN** the table states how many of the loaded rows are currently shown

#### Scenario: A partial load is disclosed

- **WHEN** the connection reports that more rows are available
- **THEN** the table states that sorting and column filtering cover only the rows loaded so far

#### Scenario: No match with more rows available is distinguished from no results

- **WHEN** a column filter matches none of the loaded rows and more rows are available
- **THEN** the table says that nothing in the loaded rows matched and that more rows can be loaded

#### Scenario: The disclosure sits with the grid, not between the filter groups

- **WHEN** the disclosure is shown
- **THEN** it appears with the grid whose rows it describes, not between the connection filters and
  the column filters

#### Scenario: A connection filter does not retire the partial-load caveat

- **WHEN** a connection filter is applied and the connection still reports more rows available
- **THEN** the table still states that sorting and column filtering cover only the rows loaded so far
