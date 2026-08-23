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

### Requirement: Sorting and filtering act on loaded rows, and the table says so

Sorting and filtering SHALL act only on the rows already fetched. Neither SHALL request a differently
ordered or differently filtered page from the connector.

Because a connection is paged, the table SHALL disclose that scope rather than let a sorted or
filtered view be mistaken for the whole of the source system. Whenever a filter is active, the table
SHALL state how many of the loaded rows are shown. Whenever the connection reports that more rows are
available, the table SHALL state that the loaded rows are not the whole result. When a filter matches
no loaded row while more rows are available, the table SHALL say that explicitly rather than present
an empty table as an answer.

#### Scenario: The table discloses the filtered subset

- **WHEN** a filter is active over loaded rows
- **THEN** the table states how many of the loaded rows are currently shown

#### Scenario: A partial load is disclosed

- **WHEN** the connection reports that more rows are available
- **THEN** the table states that sorting and filtering cover only the rows loaded so far

#### Scenario: No match with more rows available is distinguished from no results

- **WHEN** a filter matches none of the loaded rows and more rows are available
- **THEN** the table says that nothing in the loaded rows matched and that more rows can be loaded

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
