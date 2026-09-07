## MODIFIED Requirements

### Requirement: The connector schema marks each column's cardinality

`GET /api/connections/{id}/schema` SHALL return, for every column of every resource, a `FieldSpec` of
`{ key, label, ty, tier, multi_valued, transform_source }`. `multi_valued` is a boolean and SHALL be
present on every `FieldSpec`, `false` included: a reader SHALL never have to infer a column's
cardinality from its absence. `transform_source` is a boolean and SHALL likewise be present on every
`FieldSpec`; it says whether a field transform may take that column as its `source`, and what decides
it is stated by the `connector-field-transforms` capability, which this requirement neither restates
nor overrides.

`multi_valued` SHALL be `true` exactly when a value of that column is a list of strings on browse and
on materialize, and `false` otherwise. Every column the service offers today SHALL carry `false`,
which includes every derived column a field transform contributes.

`ty` remains the column's **display type** and SHALL NOT encode cardinality: `text`, `number`,
`money`, `date` and `badge` keep their existing meanings, and a multi-valued column carries the
display type of its elements. The two axes are independent, so a multi-valued column of any display
type is expressible.

`view`, `tier` and `FilterSpec` are unchanged by this requirement, and `tier` keeps its existing
meaning for a multi-valued column: `cheap` when the list call supplies it, `hydrated` when a per-row
fetch is needed, `derived` when it is computed. What the enclosing resource object carries is stated by
`connector-field-transforms`, not here.

This requirement supersedes the frozen `docs/SPEC.md` §12 `GET /connections/{id}/schema` bullet to the
extent of what a `FieldSpec` carries. Everything else that bullet states is unchanged.

#### Scenario: Every column declares its cardinality

- **WHEN** a client reads the schema of any connection
- **THEN** every `FieldSpec` of every resource carries a `multi_valued` key
- **AND** every column that is not multi-valued carries `false` rather than omitting the key

#### Scenario: Every column declares whether a transform may source it

- **WHEN** a client reads the schema of any connection
- **THEN** every `FieldSpec` of every resource carries a `transform_source` key, `false` included

#### Scenario: A derived column is not multi-valued

- **WHEN** a connection defines a field transform deriving `location_id`
- **THEN** that column's `FieldSpec` carries `tier` `derived` and `multi_valued` `false`

#### Scenario: Display type and cardinality are separate

- **WHEN** a multi-valued column of text elements is read from the schema
- **THEN** its `ty` is `text` and its `multi_valued` is `true`
