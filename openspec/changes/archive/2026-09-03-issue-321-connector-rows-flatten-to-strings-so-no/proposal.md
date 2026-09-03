## Why

Implements [#321](https://github.com/pfa230/labeler/issues/321).

A connector cannot supply a list-valued parameter, because a row value is a string everywhere it is
modelled. `LabelRow.data` is `BTreeMap<String, String>` (`src/connector/mod.rs:421-425`), `CellValue`
is `Text | Number` (`:333-338`), and the UI mirrors both as `Record<string, string>`
(`ui/src/lib/connectorRows.ts:29-31`, `ui/src/api/connectors.ts:43,57`). After #213 gave the template
model a `list` parameter, this is the gap that matters: the one upstream source this service has
cannot feed the one new parameter type it gained.

Homebox already returns the data and labeler discards it. `repo.EntitySummary.tags` and
`repo.EntityOut.tags` are both `array of repo.TagSummary`, on the list payload browse reads and the
detail payload materialize fetches. `EntitySummary` (`src/connector/homebox.rs:428-452`) does not
declare the field, so no browse row can carry it, and `extract_field` (`:541,559-563`) matches only
`Value::String` and `Value::Number`, so its `_ => String::new()` arm already turns any array-valued
key into an empty string. Because `tags` rides the list payload, the column is tier `cheap`: browse
gets it with no per-row fetch, which keeps `connector-field-transforms`' "browse does not fetch"
property intact.

## What Changes

**A row value becomes a sum of string or list.** `LabelRow.data` becomes a map to an untagged
`RowValue` of `String | Vec<String>`, `CellValue` gains a `List(Vec<String>)` variant, and the UI
mirrors both as `string | string[]` (and `string | number | string[]` for a cell). Every column that
exists today serializes byte-identically, so no current consumer's wire changes; a reader of a
possibly-multi-valued field branches on the shape. The rejected alternatives are in design.md.

**A `tags` column on Homebox `entities`.** Tier `cheap`, display type `text`, marked multi-valued. It
carries each `TagSummary.name` in the order Homebox returned. Ids, colours and icons are dropped:
nothing a label prints consumes them. An entity with no tags carries `[]`, never `""` and never an
absent key. No other Homebox array becomes a column: `attachments` and `children` are sub-resources,
not value sets, and `fields` is already mined for the scalar `custom:` columns and is unchanged.

**The schema says a column is multi-valued with a cardinality flag, not a field type.** `FieldSpec`
gains `multi_valued: bool`, always serialized, `false` on every column that exists today. `FieldType`
stays a display type. The alternative, a `FieldType::List` variant, is weighed in design.md.

**A multi-valued cell has one display text**: its elements joined with `", "` in order. The browse
table renders that text, per-column filtering matches it, and sorting orders by it. Today a
multi-valued cell would render `KIDSCONSUMABLE` (`ConnectorBrowser.tsx:52,60`), filter as
`String(array)` (`connectorFilter.ts:9-12`), and **throw** in `textKey`, which calls `.toLowerCase()`
on a value it assumed was a string (`connectorSort.ts:9-13`).

**The field mapping gains a cardinality rule, in both directions.** A `list` template parameter
becomes mappable, having been filtered out of the mapping entirely (`Connect.tsx:125`), and may take a
multi-valued column. A mismatch either way is refused with a message naming both the column and the
parameter: a multi-valued column onto a scalar parameter, and a scalar column onto a `list` parameter.
The second stays refused because splitting a scalar into elements is #348's question, unanswered here.
The mapping's same-key pre-fill matches on cardinality as well as key, so an incompatible pre-fill
never happens and the operator meets a refusal only for a pairing they chose.

**A mapped multi-valued cell reaches the batch as a JSON array**, which `list-params` already accepts,
so `{tags:join(', ')}` prints. The mapped-row grid renders such a cell read-only, showing its display
text; editing a list cell in a grid stays #271's.

**A field transform whose `source` names a multi-valued field is refused when the connection is
saved**, naming the source, alongside the existing unknown-resource-or-source refusals. Applying a
pattern per element is #350. Passing through untouched is refused outright: a rule that quietly did
nothing is what this repo's no-silent-fallbacks rule forbids.

**BREAKING** (additive, and under 1.0 no migration follows): `GET /connections/{id}/schema` gains
`multi_valued` on every `FieldSpec`, and `POST /connections/{id}/browse` and
`POST /connections/{id}/materialize` may now return an array where every value was a string or number
before. A client that assumed a scalar for every field sees an array only for a column the schema
marks multi-valued.

**Out of scope**: the batch grid, the CSV import grid and the print form are untouched, and #271, #318
and #320 stay as they are. Mapping a scalar column onto a list parameter stays refused and #348 stays
open.

## Capabilities

### New Capabilities

- `connector-multi-valued-fields`: what a connector row value is on browse and on materialize, how the
  schema marks a column multi-valued, the Homebox `tags` column, a multi-valued cell's display text,
  and the cardinality rule the Connect page's field mapping enforces.

### Modified Capabilities

- `connector-field-transforms`: "A transform is validated when the connection is saved" gains a
  refusal for a `source` naming a multi-valued field.
- `connector-browser`: "A column header orders the loaded rows" gains the rule for a multi-valued
  cell, which today's text comparison would crash on.

## Impact

- `src/connector/mod.rs`: `CellValue` gains `List`; a new `RowValue` for `LabelRow.data`; `FieldSpec`
  and `ColumnDef` gain `multi_valued`; `apply_to_map` reads and writes `RowValue`; the derived-column
  push in `Connectors::schema` and `validate_transforms` both learn the flag.
- `src/connector/homebox.rs`: `ENTITIES_COLUMNS` gains `tags`; `EntitySummary` gains `tags`;
  `summary_to_row` and `extract_field` emit a list for it.
- `src/openapi.rs`: `RowValue` registered; `FieldSpec` and `CellValue` re-emitted.
- `src/lib.rs`: HTTP tests for schema, browse, materialize and the transform refusal.
- `ui/src/api/connectors.ts`, `ui/src/lib/connectorRows.ts`, `ui/src/lib/connectorFilter.ts`,
  `ui/src/lib/connectorSort.ts`, `ui/src/pages/connect/ConnectorBrowser.tsx`,
  `ui/src/pages/Connect.tsx`, `ui/src/components/LabelGrid.tsx`.
- No store, migration or template-model change. `docs/SPEC.md` §12 is superseded in the three places
  the delta names, and is not edited.
