# connector-multi-valued-fields Specification

## Purpose

Defines what a connector row value is when a field holds more than one value: how the schema marks a
column multi-valued, what browse and materialize carry for it, how such a cell is displayed, ordered
and filtered, and the cardinality rule the Connect page's field mapping enforces between a connector
column and a template parameter.

## Requirements

### Requirement: A connector row value is a string, a number on browse only, or a list of strings

A field a connector returns SHALL carry either one value or an ordered list of values, and the wire
SHALL say which by its JSON shape rather than by a wrapper. Three shapes exist across the two
endpoints, and which are legal differs by endpoint: **materialize** carries a JSON string or a JSON
array of strings and never a JSON number, which it has never emitted and does not begin to here;
**browse** carries a JSON string, a JSON number or a JSON array of strings, the number being the shape
it already emits for a `number` or `money` cell. Each subsection below states its own set, and neither
is widened to match the other.

**Materialize.** `POST /api/connections/{id}/materialize` takes
`{ rows: [{ resource, key }], fields, expansion }` and SHALL return label rows `[{ source, data }]`,
where `data` maps each requested field name to either a **JSON string** or a **JSON array of
strings**, and to nothing else: a value in `data` SHALL NOT be a JSON number. A field the schema marks
multi-valued SHALL be an array; every other field SHALL be a string, byte-identical to what that field
returns today, the stringified form of an upstream number included. `data` SHALL carry the fields the
caller asked for and nothing else, and the 200-row cap is unchanged.

**Browse.** `POST /api/connections/{id}/browse` takes `{ resource, filters?, parent?, cursor?,
page_size? }` and SHALL return `{ rows, next_cursor, has_more, count? }`, each row being
`{ id: { resource, key }, cells, url? }`. A cell SHALL be a **JSON string**, a **JSON number**, or a
**JSON array of strings**, the number being the shape browse already emits for a `number` or `money`
cell and the array being the one shape added here. A cell for a column the schema marks multi-valued
SHALL be an array; every other cell SHALL carry the string or number it carries today. Cursor opacity,
the cursor's binding, and `url` are unchanged.

**A multi-valued field with no values is an empty array**, on both endpoints: `[]`, never `""`, never
a JSON `null`, and never an absent key. Presence and value are distinguished everywhere else in this
service, and a field whose upstream simply holds nothing is present and empty.

Nothing else about either endpoint changes. In particular, an array-valued upstream key that no column
declares SHALL keep the answer it gives today, which for materialize is the empty string its unmatched
arm produces, and for browse is no cell at all.

This requirement supersedes two bullets of the frozen `docs/SPEC.md` §12 "Browse model": the
`POST /connections/{id}/browse` bullet to the extent of what a cell may be, and the
`POST /connections/{id}/materialize` bullet in full, namely the sentence that `data` is "a string map
ready to bind to a template". The request shapes, the response shapes, the cursor contract and the
200-row cap those bullets state are unchanged and remain authoritative, as does every other part of
§12.

#### Scenario: An existing field is byte-identical

- **WHEN** a row is materialized for `fields: ["name", "quantity"]`
- **THEN** `data["name"]` and `data["quantity"]` are JSON strings holding exactly what they hold today

#### Scenario: A multi-valued field materializes as an array

- **WHEN** a row is materialized for a field the schema marks multi-valued and the upstream holds two
  values
- **THEN** `data` for that field is a JSON array of two strings, in the upstream's order

#### Scenario: A multi-valued field with no values is an empty array

- **WHEN** a row whose upstream holds no value for a multi-valued field is materialized for it
- **THEN** `data` for that field is `[]`
- **AND** it is not `""`, not `null`, and not an absent key

#### Scenario: A browsed row carries the array in its cells

- **WHEN** a resource declaring a multi-valued column is browsed
- **THEN** each row's cell for that column is a JSON array of strings
- **AND** every other cell is the string or number it is today

### Requirement: The connector schema marks each column's cardinality

`GET /api/connections/{id}/schema` SHALL return, for every column of every resource, a `FieldSpec` of
`{ key, label, ty, tier, multi_valued }`. `multi_valued` is a boolean and SHALL be present on every
`FieldSpec`, `false` included: a reader SHALL never have to infer a column's cardinality from its
absence.

`multi_valued` SHALL be `true` exactly when a value of that column is a list of strings on browse and
on materialize, and `false` otherwise. Every column the service offers today SHALL carry `false`,
which includes every derived column a field transform contributes.

`ty` remains the column's **display type** and SHALL NOT encode cardinality: `text`, `number`,
`money`, `date` and `badge` keep their existing meanings, and a multi-valued column carries the
display type of its elements. The two axes are independent, so a multi-valued column of any display
type is expressible.

`view`, `tier`, `FilterSpec` and the rest of the response are unchanged, and `tier` keeps its existing
meaning for a multi-valued column: `cheap` when the list call supplies it, `hydrated` when a per-row
fetch is needed, `derived` when it is computed.

This requirement supersedes the frozen `docs/SPEC.md` §12 `GET /connections/{id}/schema` bullet to the
extent of what a `FieldSpec` carries. Everything else that bullet states is unchanged.

#### Scenario: Every column declares its cardinality

- **WHEN** a client reads the schema of any connection
- **THEN** every `FieldSpec` of every resource carries a `multi_valued` key
- **AND** every column that is not multi-valued carries `false` rather than omitting the key

#### Scenario: A derived column is not multi-valued

- **WHEN** a connection defines a field transform deriving `location_id`
- **THEN** that column's `FieldSpec` carries `tier` `derived` and `multi_valued` `false`

#### Scenario: Display type and cardinality are separate

- **WHEN** a multi-valued column of text elements is read from the schema
- **THEN** its `ty` is `text` and its `multi_valued` is `true`

### Requirement: Homebox offers the item tags as a multi-valued column

The Homebox connector's `entities` resource SHALL offer a column keyed `tags`, labelled `Tags`, with
`ty` `text`, `tier` `cheap` and `multi_valued` `true`.

Its value SHALL be the **name** of each tag the upstream returns for that item, in the order the
upstream returned them, with no sorting, deduplication or trimming applied by this service. A tag's
id, colour, icon and description SHALL be dropped: nothing a label prints consumes them.

The column SHALL be `cheap`, so browsing SHALL carry it with no additional upstream request per row,
and materializing it SHALL take the names from the per-row detail the call already fetches. An item
carrying no tags SHALL carry `[]`, under the empty-array rule above.

The column SHALL be offered on `entities` only. No other array the upstream returns SHALL become a
column: attachments and children are sub-resources rather than value sets, and the per-item custom
fields keep the scalar `custom:<name>` columns they have today, unchanged.

The outbound `tag` browse filter is a different thing and is unchanged: it narrows the upstream's
result set and is not read back into any row.

#### Scenario: The schema offers the column

- **WHEN** a client reads the schema of a Homebox connection
- **THEN** the `entities` resource lists a `tags` column with `ty` `text`, `tier` `cheap` and
  `multi_valued` `true`
- **AND** the `locations` resource lists no such column

#### Scenario: A browsed row carries the tag names

- **WHEN** `entities` is browsed and an item upstream carries the tags `KIDS` then `CONSUMABLE`
- **THEN** that row's `tags` cell is `["KIDS", "CONSUMABLE"]`

#### Scenario: Browsing the column costs no extra request

- **WHEN** a page of `entities` is browsed
- **THEN** the number of upstream requests is the same as before the column existed

#### Scenario: Materializing the column returns the names

- **WHEN** a row is materialized for `fields: ["name", "tags"]`
- **THEN** `data["tags"]` is a JSON array of the tag names and `data["name"]` is the string it is today

#### Scenario: An untagged item carries an empty array

- **WHEN** an item with no tags is browsed and materialized
- **THEN** its `tags` cell and its `data["tags"]` are both `[]`

### Requirement: A multi-valued cell has one display text

Every place that presents a multi-valued cell as text SHALL present the same text: its elements
joined with `", "` in the order the connector returned them. An empty list displays as the empty
string, exactly as an absent cell does.

The browse table SHALL render that text for a multi-valued cell, and per-column filtering SHALL match
against it, which is what `connector-browser` already requires of every cell: filtering matches the
cell as displayed. Ordering by a multi-valued column is `connector-browser`'s and is stated there.

There SHALL be one definition of that text rather than one per presenting screen, so a filter can
never disagree with what is on screen.

#### Scenario: The browse table renders the joined elements

- **WHEN** a row whose `tags` cell is `["KIDS", "CONSUMABLE"]` is shown in the browse table
- **THEN** the cell reads `KIDS, CONSUMABLE`

#### Scenario: An empty list displays as blank

- **WHEN** a row whose multi-valued cell is `[]` is shown
- **THEN** the cell is blank, exactly as an absent cell is

#### Scenario: A column filter matches the displayed text

- **WHEN** the user types `kids, cons` into the `tags` column filter
- **THEN** the row whose cell reads `KIDS, CONSUMABLE` is shown, matched case-insensitively against
  the displayed text

### Requirement: The field mapping pairs a column and a parameter of the same cardinality

The Connect page maps each of the chosen template's parameters to a column of the connection, and that
mapping SHALL be offered for a `list` parameter as it is for every other type: a `list` parameter is
no longer omitted from the mapping.

A mapping SHALL be accepted only when the column's cardinality matches the parameter's:

- a column with `multi_valued` `true` SHALL map only to a parameter declared `type: list`;
- a column with `multi_valued` `false` SHALL map only to a parameter of any other declared type.

Either mismatch SHALL be refused with a message naming **both** the column and the parameter, and the
refusal SHALL block adding rows for that mapping rather than adding rows the batch would reject. A
multi-valued column onto a scalar parameter is refused because the parameter has no shape for a list;
a scalar column onto a `list` parameter is refused because splitting one value into elements would
need a rule this service does not have, and inventing one silently is what the no-silent-fallback rule
forbids.

The mapping's pre-fill, which maps a parameter to a column of the same key when one exists, SHALL
match on cardinality as well as key, so an incompatible pairing is never pre-filled and the operator
meets a refusal only for a pairing they chose. A parameter whose same-named column is of the wrong
cardinality SHALL be left unmapped.

Leaving a parameter unmapped is unchanged and is not a mismatch, for a `list` parameter as for any
other.

This requirement supersedes the frozen `docs/SPEC.md` §12 "Using a connection (UI)" sentence
describing the Connect page's flow, to the extent of what the field mapping accepts. Everything else
that paragraph states about the page is unchanged.

#### Scenario: A list parameter maps to a multi-valued column

- **WHEN** the chosen template declares `tags: { type: list }` and the connection offers a
  multi-valued `tags` column
- **THEN** the mapping offers `tags` as a mappable parameter and accepts that pairing

#### Scenario: A multi-valued column onto a scalar parameter is refused

- **WHEN** the operator maps the multi-valued `tags` column to a parameter declared `type: string`
- **THEN** the mapping is refused with a message naming both `tags` and that parameter
- **AND** no rows are added

#### Scenario: A scalar column onto a list parameter is refused

- **WHEN** the operator maps the scalar `name` column to a parameter declared `type: list`
- **THEN** the mapping is refused with a message naming both `name` and that parameter
- **AND** no rows are added

#### Scenario: An incompatible same-named column is not pre-filled

- **WHEN** the chosen template declares a `tags` parameter of type `string` and the connection offers
  a multi-valued `tags` column
- **THEN** that parameter starts unmapped, with no refusal shown until the operator chooses that
  column

#### Scenario: An unmapped list parameter is not a mismatch

- **WHEN** a `list` parameter is left unmapped
- **THEN** no refusal is shown and rows are added, carrying no value for that parameter

### Requirement: A mapped multi-valued column reaches the label as a list

Rows added from a connection SHALL carry a multi-valued column's value as an ordered list, and the
batch those rows produce SHALL send it as the **JSON array** that `list-params` defines for a `list`
parameter, so a template reading `{name:join('<sep>')}` prints the joined elements.

The service SHALL NOT flatten, join or otherwise reshape the value on its way from the connector to
the request: the elements that reach the label are the elements the connector returned, in that
order.

A row produced by `POST /api/connections/{id}/materialize` that carries a multi-valued cell SHALL be
shown in the row grid **read-only**, rendering the display text defined above; rows produced by CSV
parsing or manual entry carry no array and keep the em-dash they show today. Only a materialized row
may hold a list, so only a materialized row reaches the display-text branch, and the grid's rule for
every other row is unchanged rather than re-decided. Editing a list cell in any grid is out of scope
and unchanged.

#### Scenario: A tag list prints on the label

- **WHEN** an item tagged `KIDS` then `CONSUMABLE` is added with `tags` mapped to a `list` parameter
  named `tags`, and the template renders `{tags:join(', ')}`
- **THEN** the request carries `data: {"tags": ["KIDS", "CONSUMABLE"]}` and the label prints
  `KIDS, CONSUMABLE`

#### Scenario: The grid shows the value and refuses to edit it

- **WHEN** a row carrying a multi-valued cell is shown in the row grid
- **THEN** the cell reads `KIDS, CONSUMABLE` and cannot be edited

#### Scenario: An empty tag list reaches the label as an empty list

- **WHEN** an untagged item is added with `tags` mapped to a `list` parameter and the template renders
  `{tags:join(', ')}`
- **THEN** the request carries `data: {"tags": []}` and the label renders that text empty, rather than
  the response being `422 MissingField`
