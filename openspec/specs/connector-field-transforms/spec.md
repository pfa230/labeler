# connector-field-transforms Specification

## Purpose

Defines the connection-scoped rules that derive new label fields from the values a connector returns:
what a rule is, when the service accepts one, where in the read path it is applied, and what a row
carries when a rule does not match. It is the seam between what the upstream returns and what a
template binds to, for every connector.

## Requirements

### Requirement: A connection carries an ordered list of field transforms

A connection SHALL carry a `transforms` list. Each entry is `{ resource, source, pattern }`:

- `resource` is the id of a resource offered by that connection's connector;
- `source` is the key of a text-valued field of that resource;
- `pattern` is a regular expression whose **named capture groups** name the fields the rule derives.

The list is ordered and the order is preserved as stored. A connection with an empty list SHALL behave
exactly as a connection with no transforms concept at all: nothing about its schema, browse rows, or
materialized rows changes.

`POST /api/connections` and `PUT /api/connections/{id}` SHALL accept `transforms`. Omitting it on
`POST` SHALL store an empty list. Omitting it on `PUT` SHALL keep the stored list, matching how an
omitted `credential` keeps the stored credential. Sending an empty list on `PUT` SHALL clear the list.

`GET /api/connections` and `GET /api/connections/{id}` SHALL return the stored `transforms`. Unlike
`credential`, a transform is not a secret and is returned in full.

This requirement supersedes the `docs/SPEC.md` §12 sentence "A connection is
`{ id, connector, name, base_url, credential, enabled }` stored in SQLite" and the §12 connection
endpoint table's account of the `POST`/`PUT` bodies, to the extent of adding `transforms`. Everything
else those sections state about connections, including that the credential is never returned, is
unchanged.

#### Scenario: A connection round-trips its transforms

- **WHEN** a connection is created with two transforms
- **THEN** the response and every later read of that connection return both, in the order supplied

#### Scenario: An update that omits transforms keeps them

- **WHEN** a connection holding transforms is updated with a body that has no `transforms` key
- **THEN** the stored transforms are unchanged

#### Scenario: An update with an empty list clears them

- **WHEN** a connection holding transforms is updated with `transforms: []`
- **THEN** the connection holds no transforms
- **AND** its schema, browse rows, and materialized rows carry no derived fields

#### Scenario: An existing connection predating the feature has none

- **WHEN** a connection stored before this capability existed is read
- **THEN** its `transforms` is an empty list
- **AND** it browses and materializes exactly as before

### Requirement: A transform is validated when the connection is saved

The service SHALL validate every transform at save time and SHALL NOT defer any of these faults to
browse, materialize, or render. A `POST` or `PUT` carrying a rejected transform SHALL fail with
`400 InvalidRequest` and `details.reason` `connection_transform_invalid`, SHALL name the offending
rule by its zero-based index and state the cause in `message`, and SHALL leave the stored connection
untouched.

A transform SHALL be rejected when any of the following holds:

- `pattern` is not a valid regular expression, or its compiled form exceeds the compiled-size budget
  of the bounds requirement below;
- `pattern` declares no named capture group;
- `resource` is not a resource id offered by that connection's connector;
- `source` is not a text-valued field of that resource;
- `source` names a field the connector's schema marks **multi-valued**, whatever its display type. The
  message SHALL name the source. A pattern is written against one value, and this service has no rule
  for applying one to a list: applying it per element is a separate contract, tracked as #350, and
  passing the field through untouched is refused rather than accepted, because a rule that quietly
  derived nothing is exactly the silent fallback this repo forbids;
- a capture-group name equals a field key the connector already declares for that resource;
- a capture-group name is repeated within the same rule, or is produced by another rule on the same
  resource;
- a capture-group name is not a legal bare interpolation token name under the `interpolation-tokens`
  capability, which is to say it does not match `^[a-zA-Z0-9_-]+$`. A derived field is reachable from
  a template only as a bare `{name}` token, so a name carrying a dot or a colon could be advertised by
  the schema and mapped in the UI and still never reach a label. This replaces the earlier rejection
  of `datetime` and of names beginning `datetime.` or `vars.`: no word is reserved any more, so
  `datetime` is an ordinary derived name, while `datetime.short_date` and `vars.site` are refused for
  carrying a separator rather than for the word they start with.

A connector MAY declare that a resource carries text fields under a key prefix whose names it
discovers from the upstream at runtime; Homebox's per-item custom fields, keyed `custom:<name>`, are
the case that exists. A `source` under such a prefix SHALL be accepted without proving that the
upstream carries that field, because validation does not contact the upstream. Today that prefix
declares single-valued text. The cost is bounded and deliberate: a rule sourcing a custom field that
does not exist is not an error, it simply never matches, and falls under the non-match requirement
below.

Because a derived name may never equal a field the connector declares, no rule can read another rule's
output: transforms are a single flat pass over what the connector returned, and chaining is
unreachable rather than merely discouraged. Every field a rule derives is single-valued text, so no
rule can produce a multi-valued field either.

Validation SHALL NOT contact the upstream system. A connection whose upstream is unreachable, or whose
credential is wrong, SHALL still be able to save and correct its transforms.

This requirement supersedes the `docs/SPEC.md` §12 sentence "`POST` rejects an unknown `connector`, a
missing `credential`, or an invalid `base_url` with `400`" by adding transform validation to it; the
three faults it names are unchanged.

#### Scenario: A pattern that does not compile is refused

- **WHEN** a connection is saved with a transform whose `pattern` is not a valid regular expression
- **THEN** the response is `400` with `details.reason` `connection_transform_invalid`
- **AND** the message names the rule's index
- **AND** nothing is stored

#### Scenario: A pattern with no named group is refused

- **WHEN** a transform's pattern matches but declares only unnamed groups
- **THEN** the save is refused, because the rule can name no field

#### Scenario: A derived name that shadows an upstream field is refused

- **WHEN** a transform on a resource that already declares a `name` field derives a group called `name`
- **THEN** the save is refused

#### Scenario: Two rules on one resource cannot derive the same name

- **WHEN** two transforms on the same resource both derive `location_id`
- **THEN** the save is refused

#### Scenario: The same derived name on two different resources is allowed

- **WHEN** one transform on `entities` and one on `locations` both derive `location_id`
- **THEN** the save succeeds, because each rule applies only to the resource it names

#### Scenario: A derived name in a reserved namespace is refused

- **WHEN** a transform derives a group named `datetime.short_date`, `vars.site`, or `printed_on:long_date`
- **THEN** the save is refused, because the name carries a token separator, so no bare token can name
  it and it could never reach a label

#### Scenario: A derived name that was once a reserved word is accepted

- **WHEN** a transform derives a group named `datetime`
- **THEN** the save succeeds, because the name is a legal bare token name and no word is reserved

#### Scenario: A source under a connector's dynamic prefix is accepted unproven

- **WHEN** a transform sources `custom:Internal SKU` on a connector that declares `custom:` as a
  dynamic text prefix, and the upstream is never contacted
- **THEN** the save succeeds
- **AND** if no such custom field exists upstream, the rule never matches and no error is raised

#### Scenario: An unknown resource or source is refused

- **WHEN** a transform names a resource the connector does not offer, or a field that resource does not declare as text
- **THEN** the save is refused

#### Scenario: A multi-valued source is refused

- **WHEN** a transform on `entities` sources `tags`, which the schema marks multi-valued
- **THEN** the response is `400` with `details.reason` `connection_transform_invalid`
- **AND** the message names `tags`
- **AND** nothing is stored

#### Scenario: An unreachable upstream does not block saving

- **WHEN** a connection whose `base_url` cannot be reached is saved with valid transforms
- **THEN** the save succeeds

### Requirement: Derived fields are advertised by the connector schema

`GET /api/connections/{id}/schema` SHALL include, in the `columns` of each resource, one `FieldSpec`
for every capture-group name derived by that connection's transforms on that resource. A derived
column SHALL carry `ty` `text` and `tier` `derived`, and SHALL be keyed and labelled by the
capture-group name.

Derived columns SHALL appear only on the resource their rule names, and SHALL be additional to the
connector's own columns, never a replacement for one.

A stored transform naming a resource the connector no longer offers SHALL be inert: it contributes no
column and SHALL NOT make the schema request fail.

This requirement supersedes the `docs/SPEC.md` §12 bullet describing
`GET /connections/{id}/schema` to the extent of what `columns` contains. The response shape, the
meaning of `view`, `tier`, and `FilterSpec`, and every other part of that bullet are unchanged.

#### Scenario: A derived field is offered to the field picker

- **WHEN** a connection defines a transform on `entities` deriving `location_id` and `location_name`
- **THEN** the `entities` resource lists `location_id` and `location_name` among its columns
- **AND** both carry tier `derived`
- **AND** the `locations` resource lists neither

#### Scenario: A rule for a resource that no longer exists is inert

- **WHEN** a stored transform names a resource the connector does not offer
- **THEN** the schema request succeeds and carries no column for that rule

### Requirement: Browse rows carry the derived cells

`POST /api/connections/{id}/browse` SHALL apply the transforms whose `resource` matches the browsed
resource to every returned row, inserting each capture group's value as a text cell keyed by the group
name.

A transform SHALL read only the cells the connector produced for that row. It SHALL NOT trigger an
extra upstream fetch. A browsed row therefore carries a derived cell only when that row already
carries the rule's source; browse and materialize do not draw on the same set of fields, since
materialize fetches a per-row detail and browse does not.

The guarantee is one-sided, and stated as such: a derived cell shown while browsing SHALL equal what
materializing that row would produce for the same field, and browse SHALL NOT show a different value
than the label will get. It MAY show nothing where the label will get a value, when the source is a
field browse does not return.

This requirement supersedes the `docs/SPEC.md` §12 bullet describing `POST /connections/{id}/browse`
to the extent of what `cells` contains. The request shape, the response shape, and the cursor
contract are unchanged.

#### Scenario: A browsed row shows its derived cells

- **WHEN** a row's `location` cell is `BOX.123 | Motorcycle parts` and a transform splits it
- **THEN** that row's cells carry `location_id` = `BOX.123` and `location_name` = `Motorcycle parts`

#### Scenario: Browse does not fetch to satisfy a transform

- **WHEN** a transform's source is a field browse does not return for a resource
- **THEN** the browse call makes no additional upstream request
- **AND** the rows carry no derived cells for that rule
- **AND** materializing those same rows still produces the derived fields

### Requirement: Materialize emits the derived fields

`POST /api/connections/{id}/materialize` SHALL accept a derived field name in `fields` and SHALL
return it in each row's `data`.

To satisfy a requested derived field the service SHALL ensure the rule's `source` is fetched, whether
or not the caller listed it. A source fetched only to satisfy a rule SHALL NOT appear in the returned
`data`: the response carries the fields the caller asked for and nothing else.

A transform SHALL run only when its resource matches the row's resource, and SHALL read only the
fields the connector produced for that row.

This requirement supersedes the `docs/SPEC.md` §12 bullet describing
`POST /connections/{id}/materialize` to the extent of what `fields` may name and what `data` carries.
The request shape, the response shape, and the 200-row cap are unchanged.

#### Scenario: A derived field is materialized without its source

- **WHEN** materialize is called for `fields: ["location_id"]` and a rule derives it from `location`
- **THEN** each row's `data` carries `location_id`
- **AND** `data` does not carry `location`

#### Scenario: The source is returned when the caller asks for it

- **WHEN** materialize is called for `fields: ["location", "location_id"]`
- **THEN** each row's `data` carries both

#### Scenario: A rule does not cross resources

- **WHEN** a rule on `locations` derives `location_id` and rows of `entities` are materialized
- **THEN** those rows carry no `location_id`

#### Scenario: A requested derived name is never filled in by the connector

- **WHEN** materialize is called for a derived field and the rule does not match a row
- **THEN** that row's `data` has no key for the field
- **AND** in particular the key is not present with an empty value

### Requirement: A row that does not match carries no derived fields

When a transform's pattern does not match a row's source value, the service SHALL omit every field
that rule derives from that row: the keys SHALL be absent, not present-and-empty, and SHALL NOT carry
the unsplit source value.

A rule SHALL contribute fields to a row only when its pattern matches **and** every named capture
group of the pattern participates in that match. A pattern that matches while one of its named groups
does not participate, which alternation and optional groups both permit, SHALL be treated as a
non-match for the whole rule rather than filling some of its fields. A group that participates and
captures the empty string is a match and yields an empty value; not participating is not the same as
capturing nothing.

The same SHALL hold when the source field is absent from the row, when its value is empty, and when
its value exceeds the input bound of the bounds requirement below.

A non-matching row SHALL NOT fail the browse or materialize call, and SHALL NOT affect any other row
or any other rule.

Consequences of the absent key, both intended: the label grid renders a blank, editable cell for the
row so the operator can correct it before printing, and a template that binds the field and prints it
without going through the grid fails that one label with `MissingField`, which a batch reports per
label rather than aborting.

#### Scenario: One non-matching row among many

- **WHEN** 200 rows are materialized and one row's source value does not match the pattern
- **THEN** the call succeeds
- **AND** that row's `data` has no key for any field the rule derives
- **AND** the other 199 rows carry theirs

#### Scenario: A missing source field is a non-match

- **WHEN** a row does not carry the rule's source field at all
- **THEN** the row carries no derived field for that rule, and no error is raised

#### Scenario: A rule whose groups do not all participate yields nothing

- **WHEN** a pattern whose named groups sit in different alternation branches matches a row, so that
  one group participates and another does not
- **THEN** the row carries no derived field for that rule
- **AND** the participating group's value is not inserted on its own

#### Scenario: An absent derived field is visible to the operator

- **WHEN** a materialized row is loaded into the label grid and its derived field did not match
- **THEN** the grid shows that field blank and editable for that row

### Requirement: Transforms are bounded against hostile input

The service SHALL bound transform evaluation so that a stored rule cannot make a browse or
materialize call unbounded in time or memory:

- a connection SHALL hold at most 32 transforms, and a longer list SHALL be refused at save time;
- a `pattern` SHALL be at most 512 bytes, and a longer one SHALL be refused at save time;
- a compiled pattern SHALL be refused at save time when its compiled form exceeds 65536 bytes;
- a source value longer than 8192 bytes SHALL be treated as a non-match, without evaluating the
  pattern against it.

These are the only bounds the contract carries; there is no per-row or per-call time budget, because a
pattern accepted at save time matches in time linear in the length of the source value.

Every one of these limits SHALL be enforced where it is stated: the save-time ones on `POST` and
`PUT`, returning `400 InvalidRequest` with `details.reason` `connection_transform_invalid`; the
input bound on every evaluation.

#### Scenario: An over-long rule list is refused

- **WHEN** a connection is saved with 33 transforms
- **THEN** the response is `400` with `details.reason` `connection_transform_invalid`

#### Scenario: An over-long pattern is refused

- **WHEN** a transform's pattern exceeds 512 bytes
- **THEN** the save is refused

#### Scenario: A pattern that compiles too large is refused

- **WHEN** a transform's pattern is within 512 bytes but compiles to more than 65536 bytes
- **THEN** the save is refused with `details.reason` `connection_transform_invalid`

#### Scenario: An over-long source value is a non-match

- **WHEN** a row's source value is longer than 8192 bytes
- **THEN** the row carries no derived field for that rule
- **AND** the call succeeds
