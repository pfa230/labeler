## ADDED Requirements

### Requirement: A column a transform derived opens visible

The browse table SHALL show, for a resource the operator has made no column choice over, its `cheap`
columns together with every column this connection's transforms derived, and SHALL show all of a
resource's columns when it has neither. A transform-derived column is one the schema marks `tier`
`derived` and does not offer as a transform source: `connector-field-transforms` gives
`transform_source` `false` to every column a transform produced. A connector's own derived columns,
which that schema does offer as sources, SHALL stay out of the opening set exactly as they do today.
The distinction is the operator: a column exists because they wrote a rule for it, and a field they
have just created is of no use to them offered and hidden.

Where the operator **has** made a column choice, that choice SHALL bind the columns it was made over,
and a transform-derived column it did not hide SHALL still be shown. This SHALL hold for a choice made
in the current session exactly as for one restored from storage: the operator who customises a
resource's columns and then saves a rule SHALL see the new column, without reopening the page and
without touching the column picker.

Hiding a transform-derived column SHALL therefore be recorded alongside which columns are visible, so
that the hiding survives a reload and a later save alike, and so that a derived name the choice was
never made over is shown rather than suppressed by a choice that could not have considered it.

A column choice recorded before this rule records no such hiding and SHALL be read as recording none:
a transform-derived column the operator had hidden becomes visible once, and stays hidden after they
hide it again. Every other column that choice hides SHALL stay hidden, that choice being unambiguous.

#### Scenario: A resource opens showing the columns a rule derived

- **WHEN** the operator opens a resource that has `cheap` columns, a connector-derived URL column and
  a column derived by one of this connection's rules, having made no column choice for it
- **THEN** the table shows the `cheap` columns and the rule's column
- **AND** the connector's own derived URL column is not shown

#### Scenario: A newly derived column appears for a resource customized in this session

- **WHEN** the operator changes which columns a resource shows, and then, without leaving the page,
  saves a rule deriving a new name on that resource
- **THEN** the new column is shown
- **AND** every column they had just hidden stays hidden

#### Scenario: Hiding a derived column sticks

- **WHEN** the operator hides a transform-derived column, then reloads the page and saves the
  connection again
- **THEN** that column stays hidden

### Requirement: A column the schema no longer offers stops acting

When a save removes a column the operator had sorted or filtered by, that column SHALL stop acting: it
SHALL NOT order the table, and its filter SHALL NOT narrow it. Neither SHALL be cleared, and a column
that comes back SHALL resume ordering or narrowing on the terms it had, the operator having never
revoked either.

This is neither of the two cases already specified. The operator hiding a column clears that column's
filter deliberately, and a change of browsing context clears the sort and every column filter; a
column withdrawn by a save is the operator's doing at one remove and revokes nothing.

#### Scenario: Sorting by a column a save removes

- **WHEN** the operator sorts by a derived column and then saves the connection with that rule removed
- **THEN** the table shows the loaded rows in the connector's order

#### Scenario: Filtering by a column a save removes

- **WHEN** the operator filters by a derived column and then saves the connection with that rule
  removed
- **THEN** every loaded row is shown again, and the table no longer reports a narrowed subset

#### Scenario: A column that comes back resumes

- **WHEN** the operator sorts by a derived column, saves the connection with that rule removed, then
  saves it again with the rule restored
- **THEN** the table is ordered by that column again

## MODIFIED Requirements

### Requirement: Ordering and filtering are transient, and reset with the browsing context

Sorting and column filters SHALL be scoped to the resource currently being browsed. Switching the
resource tab, drilling into a relationship, and clearing the drill-down parent SHALL each clear the
active sort and every column filter, so a view control set for one list never silently narrows
another.

Sorting and column filters SHALL NOT persist beyond the session: reloading the page SHALL present the
connector's own order with no filters applied. Column visibility, which is chosen deliberately and
shows rather than hides rows, SHALL continue to persist per connection and resource. That persistence
has one bounded exception, stated by "A column a transform derived opens visible": a column choice
recorded before that rule existed carries no record of a hidden transform-derived column, so restoring
such a choice SHALL show that column once. Every other column the choice hides SHALL stay hidden, and a
choice recorded since carries the hiding and SHALL restore it.

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

#### Scenario: A column choice recorded before derived columns opened visible

- **WHEN** a user's stored column choice predates the rule that keeps transform-derived columns
  visible, and hides both a `cheap` column and a transform-derived one
- **THEN** on reload the `cheap` column is still hidden and the transform-derived one is shown
- **AND** hiding the transform-derived column again and reloading leaves it hidden
