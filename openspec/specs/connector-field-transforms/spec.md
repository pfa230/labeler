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

### Requirement: A resource declares its shape, its dynamic source prefix and its field-list completeness

`GET /api/connections/{id}/schema` SHALL describe each resource as
`{ id, label, view, columns, filters, dynamic_source_prefix, fields_incomplete }`:

- `id` and `label` are strings; `view` is `table` or `tree`;
- `columns` is the list of that resource's `FieldSpec`s, whose shape the `connector-multi-valued-fields`
  capability states, and which includes one entry per field a transform derives on that resource per
  the derived-column requirement above;
- `filters` is the list of that resource's `FilterSpec`s, typed `search`, `location_id` or `label_id`;
- `dynamic_source_prefix` (string or `null`, always present) is the key prefix under which that
  resource's connector accepts a transform `source` it does not enumerate, or `null` when it accepts
  none. Homebox reports `"custom:"` for `entities` and `null` for `locations`;
- `fields_incomplete` (boolean, always present) is `true` when the connector could not complete the
  runtime discovery of that resource's field list.

A connector that discovers part of a resource's field list from the upstream at runtime SHALL report
`fields_incomplete` `true` when that discovery fails, rather than reporting a complete list that
happens to be short. Homebox's per-item custom fields are the case that exists: they are fetched from
the upstream while the schema is built, and a fetch that fails today yields a schema reporting no
custom field at all, which reads exactly like an upstream that declares none. A consumer choosing what
a rule may source cannot tell the two apart, and a picker that silently drops every `custom:` field the
moment the upstream is unreachable is the silent fallback this repo forbids.

Failure of that discovery SHALL NOT fail the schema request: the rest of the schema is still correct,
browse still works for the columns the connector declares statically, and a source under
`dynamic_source_prefix` is still authorable and still saves, which the unreachable-upstream guarantee
above requires. The response SHALL say the list is short; it SHALL NOT pretend it is whole.

This requirement supersedes the frozen `docs/SPEC.md` §12 sentence "A resource is
`{ id, label, view, columns, filters }`; `view` is `table` or `tree`", stating the complete resource
shape in its place. What a `FieldSpec` carries is stated by `connector-multi-valued-fields`, what a
derived column is by the requirement above, and every other part of that §12 bullet, including
`{ version, resources, relationships }` and the meaning of `tier` and `FilterSpec`, is unchanged.

#### Scenario: The dynamic prefix is reported per resource

- **WHEN** the schema is read for a Homebox connection
- **THEN** `entities` carries `dynamic_source_prefix` `"custom:"` and `locations` carries `null`
- **AND** a save sourcing `custom:Internal SKU` on `entities` succeeds whether or not any column
  reports that key

#### Scenario: Discovery failure is reported, not swallowed

- **WHEN** the schema is read for a Homebox connection whose upstream refuses the custom-field request
- **THEN** the request succeeds
- **AND** the `entities` resource carries the connector's declared columns and `fields_incomplete`
  `true`

#### Scenario: A complete discovery is not marked incomplete

- **WHEN** the schema is read and the upstream returns the custom-field list
- **THEN** the `entities` resource carries a column per custom field and `fields_incomplete` `false`

### Requirement: The schema names every source a transform may take

The schema SHALL report which sources a transform rule may take, precisely enough that a client can
offer exactly the set a save accepts and nothing else. Two of the fields above carry it:
`FieldSpec.transform_source` and `ResourceSpec.dynamic_source_prefix`.

`transform_source` SHALL be `true` when the connector itself declares that column single-valued and
text-valued, whatever its `tier`, and `false` when the column is multi-valued, is not text, or is a
column a transform derives. It SHALL be computed from the same rule the save-time validation
requirement applies, so the two cannot drift: a save SHALL accept every `transform_source` column of a
resource as a `source`, and SHALL refuse every column of it where the flag is `false`.

Together the two report the accepted set exactly: a `source` a save accepts is either a
`transform_source` column of that resource or a name beginning with that resource's
`dynamic_source_prefix`, and nothing else is accepted. The prefix is reported rather than enumerated
because the set of names under it is unbounded and validation proves nothing about them, which is the
shipped contract: a rule sourcing a custom field that does not exist is not an error, it simply never
matches.

`tier` SHALL NOT be read as `transform_source` by any consumer. `tier` says where a value comes from,
not whether a rule may read it, and the two do not coincide in either direction: Homebox declares
`item_url` and `location_url` as `derived`-tier single-valued text, and both are sources a save
accepts, while a column a transform derives is also `derived`-tier and is a source no save accepts.

#### Scenario: A connector-derived text column is a source

- **WHEN** the schema is read for a Homebox connection
- **THEN** `item_url` on `entities` and `location_url` on `locations` carry `transform_source` `true`,
  both being single-valued text the connector declares
- **AND** a save naming either as a transform `source` succeeds

#### Scenario: A transform-derived column is not a source

- **WHEN** a connection derives `location_id` on `entities` and its schema is read
- **THEN** the `location_id` column carries `transform_source` `false`
- **AND** a save naming `location_id` as a transform `source` is refused

#### Scenario: A multi-valued or non-text column is not a source

- **WHEN** the schema is read for a resource carrying the multi-valued column `tags` and the numeric
  column `quantity`
- **THEN** both carry `transform_source` `false`

### Requirement: One candidate rule is previewed against live rows

The service SHALL offer `POST /api/connections/{id}/transforms/preview`, authenticated and
egress-bounded exactly as the other `/connections/{id}` routes are. It answers one question the save
cannot: whether a rule that compiles and collides with nothing actually matches anything.

The request body SHALL be:

```json
{
  "transforms": [{ "resource": "entities", "source": "location", "pattern": "..." }],
  "rule": 0,
  "page_size": 10
}
```

- `transforms` (array of the stored transform shape, required) is the **candidate** list, which is
  whatever the caller is editing and need not be what is stored;
- `rule` (integer, required) is the zero-based index in that list of the one rule to preview. The
  previewed resource is that rule's `resource`; the request carries no separate resource field, which
  could only ever disagree with it;
- `page_size` (integer, optional) is bounded by the requirement below.

No credential and no base URL travel in the body: both come from the stored connection, exactly as
browse and materialize take them. A preview against a connection that does not exist SHALL be `404`. A
`rule` that indexes no entry of `transforms` SHALL be `400 InvalidRequest` with `details.reason`
`request_body_invalid`, which is the mapping the `request-error-envelope` capability already publishes
for a body rejected for any reason it names no more specific one for.

The **whole** candidate list SHALL be validated exactly as `POST /api/connections` and
`PUT /api/connections/{id}` validate it, against every rule of the save-time validation requirement
above, and SHALL fail the same way: `400 InvalidRequest`, `details.reason`
`connection_transform_invalid`, naming the offending rule by its zero-based index in `transforms` and
stating the cause in `message`. What the preview accepts is what a save accepts, so a collision between
two rules, a rule naming a resource the connector does not offer, or a rule the caller has not finished
writing is reported here as it would be on save, whichever rule is being previewed.

Rows SHALL come from browsing one page of the previewed rule's resource upstream. The preview reads the
first page only: it accepts no cursor and returns none.

The previewed rule SHALL be evaluated by the same matching the stored rules get on the read path, so
every rule of the non-match requirement above holds here unchanged, and a preview and a save of the
same rule over the same rows SHALL agree. Evaluating the one rule rather than the whole filtered list
SHALL NOT change its result, and cannot: no rule may derive a name another rule could source, so a rule
never reads another rule's output and the pass is flat.

The response body SHALL be:

```json
{
  "rule": 0,
  "resource": "entities",
  "source": "location",
  "row_count": 10,
  "matched_count": 7,
  "rows": [
    {
      "id": { "resource": "entities", "key": "abc" },
      "source_value": "BOX.123 | Motorcycle parts",
      "matched": true,
      "value_truncated": false,
      "derived": { "location_id": "BOX.123", "location_name": "Motorcycle parts" }
    }
  ]
}
```

with these meanings, and no others:

- `rule` (integer) echoes the previewed rule's index, so a caller can attach the result to the rule it
  wrote; `resource` (string) and `source` (string) echo that rule's resource and source key;
- `row_count` (integer) is the number of rows the preview evaluated, and `matched_count` (integer) is
  how many of them the rule matched, because "compiles, collides with nothing, matches zero rows" is
  the failure this endpoint exists to expose;
- `rows` SHALL carry one entry for **every** evaluated row, in the order browsed. No row is omitted for
  any reason: a row's entry is the evidence behind the count, and a response that dropped rows would
  answer a question the caller did not ask;
- `id` is the row reference browse uses. `matched` (boolean) and `value_truncated` (boolean) are always
  present; `value_truncated` is defined by the requirement below;
- `source_value` (string) SHALL be present when the row carries a text value for the rule's `source`
  and SHALL be absent otherwise. Its absence is why the rule could not be evaluated; a present value
  with `matched` `false` is what the pattern did;
- `derived` (object of string to string) SHALL be present when `matched` is `true`, carrying every
  field the rule derives keyed by its capture-group name, and SHALL be absent when `matched` is
  `false`. A capture group that participated and captured the empty string therefore appears as a
  present key with an empty value, which the absent object never resembles.

Nothing SHALL be written. The candidate rules SHALL NOT be persisted, and the stored connection SHALL
be unchanged whether the preview succeeds or fails. An upstream failure SHALL be reported exactly as
browse reports it.

This requirement adds an endpoint and supersedes no part of the frozen `docs/SPEC.md` §12: the endpoint
table there stays authoritative for the endpoints it lists.

#### Scenario: A matching rule reports its captures and its count

- **WHEN** a rule on `entities` sourcing `location` with a pattern splitting
  `BOX.123 | Motorcycle parts` is previewed over a page of 10 rows, 7 of which carry a location in that
  shape
- **THEN** the response is `200` with `row_count` 10 and `matched_count` 7
- **AND** `rows` carries 10 entries
- **AND** a matched row carries `source_value` `BOX.123 | Motorcycle parts` and `derived`
  `{"location_id": "BOX.123", "location_name": "Motorcycle parts"}`

#### Scenario: A rule that matches no row says so

- **WHEN** a rule whose pattern compiles and collides with nothing matches none of the evaluated rows
- **THEN** the response is `200` and `matched_count` is `0`
- **AND** every row entry carries `matched` `false` and no `derived`

#### Scenario: A non-match and an empty capture are distinguishable

- **WHEN** one row's source value matches a pattern whose named group captures the empty string, and
  another row's source value does not match at all
- **THEN** the first row carries `derived` with that key set to `""`
- **AND** the second row carries no `derived`

#### Scenario: A row that does not carry the source is distinguishable from a non-match

- **WHEN** an evaluated row carries no text value for the rule's `source`
- **THEN** that row carries no `source_value` and carries `matched` `false`
- **AND** a row that carries a source value the pattern did not match carries its `source_value`

#### Scenario: An invalid candidate rule is refused exactly as a save is

- **WHEN** a preview of rule 0 carries a candidate rule at index 1 whose `pattern` declares no named
  capture group
- **THEN** the response is `400` with `error.code` `InvalidRequest` and `details.reason`
  `connection_transform_invalid`
- **AND** the message names rule 1
- **AND** no upstream row is fetched and nothing is stored

#### Scenario: A collision anywhere in the list is refused

- **WHEN** a preview of rule 0 carries two candidate rules on `entities` that both derive
  `location_id`
- **THEN** the response is `400` with `details.reason` `connection_transform_invalid`

#### Scenario: A rule index that names nothing is refused

- **WHEN** a preview carries two candidate rules and `rule` 5
- **THEN** the response is `400` with `details.reason` `request_body_invalid`

#### Scenario: An unknown connection is not found

- **WHEN** a preview is requested for a connection id that does not exist
- **THEN** the response is `404`

#### Scenario: A preview never touches the stored rules

- **WHEN** a connection storing one transform is previewed with a candidate list holding a different
  rule, and again with a candidate list that is refused
- **THEN** both times the stored connection's `transforms` are exactly the one rule it started with

### Requirement: A preview evaluates and reports a bounded page

The preview SHALL bound what it evaluates, and SHALL bound each value it reports, without ever omitting
a row it evaluated:

- `page_size` SHALL default to 10 and SHALL be bounded as browse bounds it, which is a clamp into
  `1..=200`: `0` becomes `1` and a value above `200` becomes `200`. Preview inherits browse's bounds
  rather than declaring its own, because it is one browse call.
- The preview SHALL evaluate at most that effective page size in rows, taking them in browse order,
  and `row_count` SHALL be the number it evaluated. A connector that returns more rows than were asked
  for SHALL NOT enlarge the preview: the excess is dropped before the rule is evaluated, so no upstream
  can make the response larger than the request allowed.
- Every value the response reports, whether a `source_value` or a value in `derived`, SHALL be
  truncated to at most **512 bytes**, cut on a character boundary, and a row entry carrying at least
  one truncated value SHALL carry `value_truncated` `true`. Truncation applies to the report only:
  matching SHALL run against the whole source value, so `matched`, the set of keys in `derived`,
  `row_count` and `matched_count` are exactly what a save would produce.

These bound the response by construction. It carries at most `page_size` row entries; each entry
carries one row reference, exactly as a browse row does, and at most 512 bytes per reported value,
of which there is one source value and one per capture group the pattern names, itself limited by the
512-byte pattern bound. At the default page of 10 that is kilobytes; at the maximum page of 200 with a
pattern that names as many groups as it can hold, it is a few megabytes, which is the order of the
browse page the preview is drawn from. Reporting one rule per request is what keeps it there: a report
covering every candidate rule would multiply that by as many as 32, and no bound could hold it without
dropping the row detail the caller asked for.

#### Scenario: Page size is clamped, not rejected

- **WHEN** a preview requests `page_size` 500, and another requests `page_size` 0
- **THEN** the first evaluates at most 200 rows and the second evaluates 1

#### Scenario: An over-returning upstream does not enlarge the preview

- **WHEN** a preview asks for 10 rows and the upstream returns 50
- **THEN** `row_count` is 10, `rows` carries 10 entries, and `matched_count` counts those 10 rows only

#### Scenario: A long value is truncated in the report but not in the matching

- **WHEN** a row's source value is 4000 bytes and the rule matches it
- **THEN** the reported `source_value` is at most 512 bytes and that row carries `value_truncated`
  `true`
- **AND** `matched_count` counts that row

### Requirement: The rule editor is schema-driven and previews each rule

The field-transform rule editor SHALL belong to the **edit** form only. The create form SHALL carry no
rule editor, SHALL state that transform rules are added after the connection is saved, and SHALL send
no `transforms` key, so a new connection is created with an empty list. The connector schema and the
preview are both `{id}`-scoped, so a connection being created has neither, and a free-text `source`
kept alive for the create form would be a second spelling of a field the edit form picks.

The editor SHALL be driven by `GET /api/connections/{id}/schema`:

- **resource** SHALL be a select over the resource ids that schema reports. No resource list SHALL be
  hardcoded in the UI.
- **source** SHALL offer exactly the sources a save accepts for the chosen resource, which is that
  resource's `transform_source` columns and, when its `dynamic_source_prefix` is not `null`, one
  further choice naming a field under that prefix by name. Choosing it SHALL present an input for the
  name alone, and the rule's `source` SHALL be the reported prefix followed by that name. The operator
  SHALL NOT be able to type a `source` outside that construction, and the editor SHALL NOT apply a rule
  of its own about which columns qualify; in particular it SHALL NOT filter on `tier`.
- Changing a rule's **resource** SHALL reset its **source** to a source of the new resource, since the
  old one need not be one. When a resource offers no `transform_source` column and no
  `dynamic_source_prefix`, the editor SHALL say so rather than leave a control that silently means
  nothing.
- When a resource carries `fields_incomplete` `true`, the editor SHALL say so where it offers that
  resource's sources, because a column the upstream carries may be missing from the list. Naming a
  field under the prefix stays available, which is what keeps a rule authorable while discovery is
  down.

A stored rule whose `resource` or `source` the schema does not offer SHALL be preserved and shown as
its own option, marked unavailable. The editor SHALL NOT silently rewrite such a rule to the first
option a select happens to hold.

The editor SHALL be live only while the schema and the preview describe the connection the form is
showing. It SHALL fall back to showing the stored rules read-only, with the reason, and the form SHALL
then omit `transforms` from the save so the stored rules are kept, in either of these states:

- the schema request failed, so nothing is known about what the rules may source;
- the form's **base url** differs from the stored one, or an **api key** has been typed. Both decide
  which upstream answers and as whom, while the schema and every preview are taken from the stored
  connection, so an editor left live would offer columns and report matches from an upstream the form
  is no longer describing.

In the second state the editor SHALL say that the connection details must be saved first, SHALL
discard every displayed preview result, and SHALL offer no preview. Saving the form SHALL make the
editor live again against the saved details. An unreachable upstream SHALL NOT clear a rule: the API's
guarantee that a connection with an unreachable upstream can still save its transforms is unchanged.

Each rule SHALL carry a preview control. Previewing a rule SHALL request
`POST /api/connections/{id}/transforms/preview` with **every** candidate rule the editor holds, in
editor order, and that rule's index as `rule`, so ordering and collisions are the ones a save would
see. The panel SHALL show `matched_count` against `row_count`, and for each row that row's
`source_value` and either its `derived` fields or that the rule did not match. A row carrying no
`source_value` SHALL read differently from one whose `source_value` is empty, and a row carrying
`value_truncated` `true` SHALL say the value shown is shortened.

A preview refused with a rule index SHALL be reported against that rule, which is not necessarily the
rule being previewed: an unfinished rule elsewhere in the editor refuses the whole list, and the
editor SHALL name the rule that caused it rather than reporting a failure of the previewed one.

A displayed result SHALL be the answer to the request the operator last made for that rule, over the
rules now on screen. Two things follow, and the editor SHALL hold both:

- any edit to the candidate rules, meaning a change to any field of any rule, a rule added, a rule
  removed or the order changed, SHALL discard every displayed result, and a response computed for a
  candidate list that has since been edited SHALL NOT be displayed;
- when a rule is previewed again before an earlier request for it has resolved, only the later
  request's response SHALL be displayed, whatever order the two arrive in. Identical candidate lists do
  not make two responses interchangeable: the rows behind them are fetched live and can differ.

A panel showing anything else would be the misleading feedback this change exists to remove, so the
editor shows nothing rather than something stale.

#### Scenario: The resource select comes from the connector schema

- **WHEN** the operator edits a connection whose schema reports the resources `entities` and
  `locations`
- **THEN** each rule's resource select offers exactly those two ids

#### Scenario: The source control offers exactly the accepted sources

- **WHEN** the schema reports, for `entities`, `location` and `item_url` with `transform_source`
  `true`, `tags` and `location_id` with `transform_source` `false`, and `dynamic_source_prefix`
  `"custom:"`
- **THEN** the source control offers `location` and `item_url`, offers neither `tags` nor
  `location_id`, and offers naming a field under `custom:`

#### Scenario: A field under the prefix is authored by name

- **WHEN** the operator chooses to name a field under `custom:` and types `Internal SKU`
- **THEN** the rule's `source` is `custom:Internal SKU`
- **AND** the operator cannot type a source that does not begin with the reported prefix

#### Scenario: A resource with no prefix offers no by-name choice

- **WHEN** the operator selects `locations`, whose `dynamic_source_prefix` is `null`
- **THEN** the source control offers that resource's `transform_source` columns and no by-name choice

#### Scenario: An incomplete field list is disclosed and still authorable

- **WHEN** the schema reports `fields_incomplete` `true` for `entities`
- **THEN** the editor says the list may be missing fields the upstream carries
- **AND** a field under `custom:` can still be named and saved

#### Scenario: A stored rule the schema no longer offers is not rewritten

- **WHEN** a stored rule sources a field the schema no longer reports for its resource
- **THEN** the source control shows that field, marked unavailable, and the rule is unchanged until
  the operator changes it

#### Scenario: The create form has no rule editor

- **WHEN** the operator opens the form to add a connection
- **THEN** no rule editor renders, the form states that rules are added after saving, and the create
  request carries no `transforms`

#### Scenario: A failed schema request does not clear stored rules

- **WHEN** the operator edits a connection whose schema request failed, and saves the form
- **THEN** the stored rules are shown read-only with the reason
- **AND** the request carries no `transforms` key

#### Scenario: Editing the base url suspends the editor

- **WHEN** the operator changes the **base url**, or types an **api key**, while a preview result is
  displayed
- **THEN** the result is discarded, no preview can be requested, and the rules are shown read-only
  saying the connection details must be saved first
- **AND** saving the form sends no `transforms` key, so the stored rules survive

#### Scenario: The preview panel reports the count and the non-matching rows

- **WHEN** the operator previews a rule and the response reports `matched_count` 1 of `row_count` 2
- **THEN** the panel shows that count, shows the matched row's `source_value` beside its `derived`
  fields, and shows the other row's `source_value` marked as not matching

#### Scenario: A preview refused for another rule is shown on that rule

- **WHEN** the operator previews rule 0 while rule 1 is unfinished, and the service refuses the
  request naming rule 1
- **THEN** the error is shown against rule 1

#### Scenario: Editing a rule discards the displayed result

- **WHEN** a preview panel is showing a result and the operator edits any rule's pattern, source or
  resource, or adds, removes or reorders a rule
- **THEN** the panel stops showing that result

#### Scenario: A response for an edited list is never displayed

- **WHEN** a preview is requested, the operator edits a rule before the response arrives, and the
  response then arrives
- **THEN** no result from it is displayed

#### Scenario: An earlier response never overwrites a later one

- **WHEN** the operator previews the same rule twice with no edit between, and the first request's
  response arrives after the second's
- **THEN** the panel shows the second request's result, and the first's is discarded

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
