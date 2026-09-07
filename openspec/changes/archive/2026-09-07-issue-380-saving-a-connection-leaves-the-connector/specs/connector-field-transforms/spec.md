## ADDED Requirements

### Requirement: A saved rule shows on the open Connect page without a reload

A derived column is computed from the connection's rules at the moment the schema is read, and a
derived cell from the same rules at the moment the rows were browsed. Saving a connection's transforms
SHALL therefore bring both up to date on the open page, without a reload and without remounting it.

After a save, the derived names SHALL be offered wherever the schema is read: the composer's field
mapping and the browse table's column picker. A column derived by this connection's rules SHALL be
shown in the browse table without the operator choosing it, on the terms `connector-browser` sets, so
that a field the operator has just created is visible rather than merely offered.

The rows on screen SHALL carry that column's cells for the rows whose source matched, and SHALL carry
none for the rows that did not, exactly as a freshly browsed page would. This SHALL hold for a rule
change that alters no column: editing a rule's `pattern` or `source` while keeping its capture-group
names changes every derived cell and no column name. Removing a rule SHALL take its column and its
cells off the page on the same terms.

How the page comes to re-read the schema and re-browse the rows is stated by `connections` ("Saving or
deleting a connection refreshes what the open Connect page holds"), and which columns a resource opens
with by `connector-browser`. This requirement restates neither and overrides neither.

#### Scenario: A newly derived field reaches the rows on screen

- **WHEN** the operator saves a rule deriving `location_id` on the resource being browsed
- **THEN** the browse table shows a `location_id` column without the operator choosing it
- **AND** each row whose source matched carries its captured value, and each row that did not carries
  no `location_id` cell
- **AND** neither a reload nor a remount was needed

#### Scenario: A rule edit that changes values but no column

- **WHEN** a rule deriving `location_id` is edited so its pattern captures a different part of the
  source, its derived name unchanged, and the connection is saved
- **THEN** the rows on screen show the new `location_id` values

#### Scenario: Removing a rule removes its column and its cells

- **WHEN** the operator deletes the rule deriving `location_id` and saves
- **THEN** the browse table no longer shows a `location_id` column, and the rows carry no such cell

#### Scenario: The field mapping offers a newly derived field

- **WHEN** the operator saves a rule deriving `location_id` and a template is selected
- **THEN** the composer's field mapping offers `location_id` as a connector field, with no reload
