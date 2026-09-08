## ADDED Requirements

### Requirement: A saved rule is in force the next time Connect is opened

A derived column is computed from the connection's rules at the moment the schema is read, and a
derived cell from the same rules at the moment the rows were browsed. The rule editor is on
`/connections/{id}` and the browse table is on `/connect`, so the two never render together and a save
SHALL NOT be required to reach a Connect page beside it. A saved rule SHALL instead be in force the
next time Connect is opened, which reads the schema and browses the rows afresh. A save that settles
after the operator has already reached Connect SHALL be in force there too, on the terms `connections`
sets in "A connection write settles before Connect acts on it": the page acts on nothing until the
write settles, and then on answers read after it.

On that visit the derived names SHALL be offered wherever the schema is read: the composer's field
mapping and the browse table's column picker. A column derived by this connection's rules SHALL be
shown in the browse table without the operator choosing it, on the terms `connector-browser` sets, so
that a field the operator has just created is visible rather than merely offered.

The rows SHALL carry that column's cells for the rows whose source matched, and SHALL carry none for
the rows that did not, exactly as a freshly browsed page would. This SHALL hold for a rule change that
alters no column: editing a rule's `pattern` or `source` while keeping its capture-group names changes
every derived cell and no column name. Removing a rule SHALL take its column and its cells off the
page on the same terms.

A schema read before the save SHALL NOT be what that visit presents, which `connections` requires of
every write; without it a rule saved a moment ago could be absent from the schema the page opens with.

How the page comes to read the schema and browse the rows is stated by `connections`, and which columns
a resource opens with by `connector-browser`. This requirement restates neither and overrides neither.

A return to Connect resolves afresh and restores neither the connection the operator was browsing nor
the resource they were on (`default-connection`, `connector-browser`). A saved rule therefore shows
where the rule applies: on the connection it belongs to, once that connection is selected, and on the
resource it derives on, once that resource is being browsed. The scenarios below name both, because a
rule saved against a connection nobody has selected changes nothing on screen.

#### Scenario: A newly derived field reaches the rows on the next visit

- **WHEN** the operator saves a rule deriving `location_id` on a resource, and returns to Connect with
  that connection selected and that resource being browsed
- **THEN** the browse table shows a `location_id` column without the operator choosing it
- **AND** each row whose source matched carries its captured value, and each row that did not carries
  no `location_id` cell

#### Scenario: A rule edit that changes values but no column

- **WHEN** a rule deriving `location_id` is edited so its pattern captures a different part of the
  source, its derived name unchanged, the connection is saved, and the operator returns to Connect
  with that connection selected and the rule's resource being browsed
- **THEN** the browse table shows the new `location_id` values

#### Scenario: Removing a rule removes its column and its cells

- **WHEN** the operator deletes the rule deriving `location_id`, saves, and returns to Connect with
  that connection selected and the rule's resource being browsed
- **THEN** the browse table shows no `location_id` column, and the rows carry no such cell

#### Scenario: The field mapping offers a newly derived field

- **WHEN** the operator saves a rule deriving `location_id`, and returns to Connect with that
  connection selected and a template chosen
- **THEN** the composer's field mapping offers `location_id` as a connector field

## REMOVED Requirements

### Requirement: A saved rule shows on the open Connect page without a reload

**Reason**: It requires a saved rule to reach the Connect page "without a reload and without
remounting it", which #384 makes false by construction: the rule editor moved to `/connections/{id}`,
so the Connect page is not mounted when a rule is saved from it. Every guarantee about what the derived column
and its cells look like survives, and is restated by "A saved rule is in force the next time Connect is
opened"; only the promise that no remount happens is withdrawn, because a remount is now how the page
gets the new schema.

**Migration**: None. Field-transform semantics, the preview call and what the rule editor offers are
unchanged; only where the editor renders, and therefore when its result reaches the browse table.
