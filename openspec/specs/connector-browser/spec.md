# connector-browser Specification

## Purpose
Defines what the Connect page's browse table presents and how a user narrows it: the controls that
reorder, filter and hide columns of the rows already fetched from a connection, the scope those
controls act on, and what they must leave untouched. It is the boundary between reading the rows a
connector returned and choosing which of them become labels.

## Requirements


### Requirement: A column header orders the loaded rows

Every column the browse table displays SHALL be sortable from its header. Activating a column's sort
control SHALL cycle that column through ascending, then descending, then unsorted, where unsorted
restores the order the connector returned. At most one column SHALL be sorted at a time: sorting a
second column SHALL release the first.

The comparison SHALL follow the column's declared field type, not the shape of the value that happens
to be present:

- `text` and `badge` compare as text, case-insensitively;
- `number` and `money` compare numerically;
- `date` compares chronologically.

A row whose cell for the sorted column is absent or empty SHALL order after every row that has a
value, in **both** directions, so a blank never displaces a real value at the top of the list. Rows
that compare equal SHALL retain the relative order the connector returned.

A column's declared type is a claim about the column, not a guarantee about each cell: a connector may
place a text value in a `number`, `money` or `date` column. A cell that cannot be interpreted as the
column's declared type SHALL be treated exactly as an absent cell: it orders after every interpretable
value in both directions, and ties among such cells keep the connector's order. `number` and `money`
cells are interpretable when they yield a finite number; `date` cells are interpretable when they
parse as an ISO-8601 date or date-time. This SHALL NOT fall back to comparing those cells as text,
because a column that silently switches ordering rule based on its contents cannot be reasoned about
by the person reading it.

A **multi-valued** cell, which the `connector-multi-valued-fields` capability defines as a list of
strings, SHALL be compared by its **display text** — its elements joined with `", "` in order — on
exactly the terms above, with no rule of its own. So a multi-valued cell in a `text` or `badge` column
orders by that text case-insensitively, an empty list orders with the blanks because its display text
is empty, and a multi-valued cell in a `number`, `money` or `date` column is uninterpretable as that
type and orders with the blanks. Comparing the display text is not the text fallback the paragraph
above forbids: it is the same interpretation rule applied to the one text the browse table shows for
that cell.

Interpretation affects ordering only. Filtering SHALL continue to match against the cell as displayed,
so a value too malformed to sort is still findable.

The sorted order SHALL apply to the whole loaded set, including rows appended by a later page, and
the table SHALL indicate which column is sorted and in which direction, both visually and to assistive
technology.

#### Scenario: Sorting cycles through three states

- **WHEN** a user activates the same column header three times
- **THEN** the rows are ordered ascending, then descending, then returned to the connector's order

#### Scenario: A numeric column sorts numerically, not as text

- **WHEN** a `number` or `money` column holding 2, 10 and 99.95 is sorted ascending
- **THEN** the order is 2, 10, 99.95

#### Scenario: Rows missing the sorted value sort last in both directions

- **WHEN** a column is sorted ascending and then descending, and some rows have no value for it
- **THEN** those rows appear after every row that has a value, in both orders

#### Scenario: A newly loaded page joins the current order

- **WHEN** a sort is active and the user loads another page
- **THEN** the appended rows are placed within the existing order rather than appended to the end

#### Scenario: Sorting a second column releases the first

- **WHEN** a user sorts one column and then sorts another
- **THEN** only the second column is sorted, and the first shows no sort state

#### Scenario: A value that cannot be read as the column's type sorts with the blanks

- **WHEN** a `number` or `money` column holding 2, "n/a" and 10 is sorted ascending and then descending
- **THEN** the order is 2, 10, "n/a" ascending and 10, 2, "n/a" descending

#### Scenario: An unparsable date sorts with the blanks rather than as text

- **WHEN** a `date` column holding "2026-01-05", "" and "sometime in June" is sorted in either direction
- **THEN** only "2026-01-05" is ordered by date, and the other two follow it in the connector's order

#### Scenario: A multi-valued text column sorts by its displayed text

- **WHEN** a multi-valued `text` column holding `["KIDS", "CONSUMABLE"]`, `["ATTIC"]` and `[]` is
  sorted ascending
- **THEN** the order is `ATTIC`, `KIDS, CONSUMABLE`, then the empty list
- **AND** sorting descending gives `KIDS, CONSUMABLE`, `ATTIC`, then the empty list

#### Scenario: Sorting a multi-valued column does not fail

- **WHEN** any column carrying a multi-valued cell is sorted in either direction
- **THEN** the table reorders and stays usable, rather than erroring

#### Scenario: An unsortable value is still findable by filtering

- **WHEN** a `number` column contains "n/a" and the user filters that column for "n/a"
- **THEN** that row is shown

### Requirement: A per-column filter narrows the loaded rows

Every column the browse table displays SHALL offer a filter control in its header. A filter SHALL
match case-insensitively on the cell as displayed, and SHALL match anywhere within it rather than only
at the start. Filters on different columns SHALL combine with AND: a row is shown only if it satisfies
every non-empty filter. Filtering SHALL take effect as the user types, without a separate confirmation
step, and SHALL apply to rows appended by a later page.

Filtering SHALL be independent of the connection's server-side filters: entering a column filter SHALL
NOT re-query the connector, change the cursor, or discard loaded rows. Clearing a filter SHALL restore
the rows it hid.

Hiding a column through the column visibility control SHALL clear that column's filter, so a hidden
column can never narrow the table invisibly.

#### Scenario: A filter narrows to matching rows and restores on clear

- **WHEN** a user types into one column's filter and then clears it
- **THEN** only rows whose cell contains that text are shown, and clearing restores the full loaded set

#### Scenario: Filters on two columns combine with AND

- **WHEN** filters are set on two columns
- **THEN** only rows satisfying both are shown

#### Scenario: Filtering does not re-query the connector

- **WHEN** a user types into a column filter
- **THEN** no browse request is issued and the pagination cursor is unchanged

#### Scenario: Hiding a filtered column clears its filter

- **WHEN** a column carrying a non-empty filter is hidden through the column visibility control
- **THEN** that filter no longer restricts the table

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

### Requirement: Ordering and filtering are transient, and reset with the browsing context

Sorting and column filters SHALL be scoped to the resource currently being browsed. Switching the
resource tab, drilling into a relationship, and clearing the drill-down parent SHALL each clear the
active sort and every column filter, so a view control set for one list never silently narrows
another.

Sorting and column filters SHALL NOT persist beyond the session: reloading the page SHALL present the
connector's own order with no filters applied. Column visibility, which is chosen deliberately and
shows rather than hides rows, SHALL continue to persist per connection and resource.

#### Scenario: Switching resources clears the view controls

- **WHEN** a user sorts and filters one resource, then switches to another resource tab
- **THEN** the second resource is shown in the connector's order with no filters applied

#### Scenario: Drilling in clears the view controls

- **WHEN** a user sorts and filters a list, then drills into a row's relationship
- **THEN** the drilled-in list is shown in the connector's order with no filters applied

#### Scenario: A reload does not restore a filter

- **WHEN** a user sets a column filter and later reloads the page
- **THEN** no column filter is applied

#### Scenario: Column visibility survives a reload

- **WHEN** a user changes which columns are visible and later reloads the page
- **THEN** the same columns are visible

### Requirement: The browse table's existing behavior is preserved under the view controls

The browse table SHALL continue to present a connector's resources as schema-driven columns with the
connection's typed server-side filters, cursor pagination through an explicit load-more action, and
direct drill-down through relationships. Adding ordering and filtering SHALL leave the following
unchanged:

- **Selection is by row identity, not by position or visibility.** A selected row that a sort moves or
  a filter hides SHALL remain selected, SHALL remain listed in the selection summary, and SHALL remain
  removable from it.
- **The selection summary's "in this view" count means loaded rows, not displayed rows.** A filter
  SHALL NOT move rows between the visible and hidden halves of that split.
- **The materialize cap still binds.** At the cap, an unselected row's selection control SHALL be
  disabled whether or not a filter or sort is active.
- **Each row keeps its link to the source system** on the name cell, and its drill-down action where
  the resource has a relationship.
- **Column visibility is unchanged**: the user chooses which of the resource's columns are shown, at
  least one column always remains visible, and the choice persists per connection and resource.

This requirement supersedes the `docs/SPEC.md` §12 "Using a connection (UI)" description of the browse
table, namely the parenthetical "a generic schema-driven table with typed filters, cursor pagination,
and direct drill-down via relationships", to the extent of adding ordering, per-column filtering and
column visibility to that table. Everything else §12 states, including the connection endpoints, the
browse and materialize contracts, cursor opacity, the egress policy, and the Settings > Connections
form, is unchanged and remains authoritative.

#### Scenario: A filtered-out row stays selected

- **WHEN** a user selects a row and then applies a filter that excludes it
- **THEN** the row remains selected and remains listed in the selection summary

#### Scenario: Filtering does not change the visible/hidden split

- **WHEN** a filter hides some selected rows from the table
- **THEN** the summary still counts them as in this view, because they are loaded

#### Scenario: The cap still disables selection while sorted

- **WHEN** the selection is at the materialize cap and a sort is active
- **THEN** the selection control of every unselected row is disabled

#### Scenario: Drill-down and the source link survive sorting

- **WHEN** a sort is active
- **THEN** each row still links to its page in the source system and still offers its drill-down action
