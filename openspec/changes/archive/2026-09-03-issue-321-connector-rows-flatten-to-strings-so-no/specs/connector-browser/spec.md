## MODIFIED Requirements

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
