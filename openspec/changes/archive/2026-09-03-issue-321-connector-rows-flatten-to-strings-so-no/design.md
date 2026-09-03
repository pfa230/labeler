## Context

See proposal.md for motivation. What shapes the approach is code that already exists, and one of the
issue's own pointers is wrong.

- **`src/api.rs:2771` is not the connector seam.** The issue names it as "the seam that today wraps
  every connector cell in `serde_json::Value::String`". That line is in `import_csv`, folding a CSV
  `option.<name>` column into `data`, and the only other `Value::String` construction near it
  (`:2265`) is `parse_csv_rows`. The connector handlers (`connection_browse` `:2186`,
  `connection_materialize` `:2211`) serialize what the connector returned and construct nothing. **The
  server has no connector-cell-to-request seam at all**, because the conversion happens in the
  browser: `rowsFromMaterialized` (`ui/src/lib/connectorRows.ts:22-41`) copies materialized fields
  into a grid row, and `pruneDataForSubmit` (`ui/src/lib/labelInputs.ts:240-263`) builds the request
  from that row. The change is therefore typed end to end in Rust and shaped in TypeScript, with no
  serialization seam to widen in `api.rs`.
- **`pruneDataForSubmit` already passes an array through** for an input whose control is `list`
  (`labelInputs.ts:251-256`), and drops any non-array value for one. So once a grid row's cell holds
  `string[]`, the batch already carries a JSON array and `list-params` already accepts it. Nothing on
  the submit path needs changing.
- **`Connect.tsx` filters `list` inputs out of three places**: the mappable parameters
  (`:125`), the union of displayed grid columns (`:152`, `:158`), and per-row validation (`:177`).
  The first two must stop filtering for a list parameter to be mappable and its column visible; the
  third stays, because validating a list cell is not this change's and #213 left it to the server.
- **Three UI readers assume a cell is a string or a number.** `NameCell` renders `<>{value}</>`
  (`ConnectorBrowser.tsx:52,60`), which for an array concatenates elements with no separator;
  `displayedCell` does `String(value)` (`connectorFilter.ts:9-12`), which yields `KIDS,CONSUMABLE`;
  and `textKey` calls `.toLowerCase()` on anything that is not a number (`connectorSort.ts:9-13`),
  which **throws** on an array. The third is the one that fails loudly, and it fails on the first
  click of the `tags` header.
- **Transforms already discriminate on the cell variant.** `apply_to_cells` matches only
  `CellValue::Text` (`src/connector/mod.rs:143-146`), so a browse cell that is a list is already inert
  for transforms; `apply_to_map` (`:127-141`) reads a `String` map and must learn the same
  discrimination once that map holds a sum. The save-time refusal is what makes both unreachable in
  practice; the match arms are what make them safe anyway.
- **`validate_transforms` reads `c.ty == FieldType::Text`** (`:195-198`). With the flag spelling
  chosen below, `tags` is a `Text` column, so that check alone would accept it and the refusal has to
  be written explicitly.
- **`Connectors::schema` builds derived `FieldSpec`s inline** (`:486-491`), so a new `FieldSpec` field
  has two construction sites in Rust: that one and `homebox.rs`'s `field()` helper (`:454-461`).
- **The materialize path already strips and re-adds keys** around the transform pass
  (`:575-587`): it removes each derived name, applies the rules, then retains only the requested
  fields. That machinery is value-type agnostic and needs only its map type changed.

## Goals / Non-Goals

**Goals:**

- One place decides whether a value is single or multi-valued (the schema's `multi_valued`), and one
  place decides what a multi-valued cell looks like as text.
- Byte-identical output for every field that exists today, on both endpoints, so no client is
  disturbed by a column it does not read.
- A cardinality mismatch that is impossible to reach silently: refused at save for a transform,
  refused at mapping for a parameter, and shaped by the type system in between.
- No new upstream request. `tags` rides the payload browse already fetches and the detail materialize
  already fetches.

**Non-Goals:**

- Applying a transform pattern per element (#350), splitting a scalar column into elements (#348), a
  list editor in any grid (#271), the CSV spelling of a list (#320), or the batch grid and print form
  (#318). Each stays exactly as it is.
- Any second multi-valued column. `attachments`, `children` and the tag ids, colours and icons are all
  reachable and all deliberately unread.
- Validating a `list` cell in the Connect grid. The server decides a missing required list, as it does
  today.

## Decisions

### The schema marks cardinality with a flag on `FieldSpec`, not a `FieldType` variant

This is the decision the issue left to the plan. `FieldSpec` gains `multi_valued: bool`, always
serialized; `ColumnDef` gains the same so a static column declares it once.

**Why over `FieldType::List`.** `FieldType` is a *display* type: every existing variant (`text`,
`number`, `money`, `date`, `badge`) says how to present and compare a value, and `connector-browser`
turns each into a comparison rule. A `List` variant would make one variant mean cardinality instead,
and the first thing it would cost is the answer to "how does this column sort?", which the other five
answer and it could not. It would also erase the element's display type: `tags` is text, and a future
multi-valued date or money column would have nowhere to say so. The two axes are independent, so two
fields express them.

**What the flag costs**, honestly: a new key on every `FieldSpec` in a published response, and one
more field to set at each of the two construction sites. It is always emitted rather than
`skip_serializing_if`, because a reader that has to infer `false` from an absent key is the inference
the flag exists to remove.

**A third option, not taken:** inferring cardinality from a cell's shape at the client. That is what
the code does today by accident, it is undecidable before the first row arrives, and it makes the
mapping screen depend on having browsed.

### Two value types, not one

`CellValue` gains `List(Vec<String>)`, becoming `Text | Number | List`. `LabelRow.data` becomes
`BTreeMap<String, RowValue>` where `RowValue` is a new untagged `Text(String) | List(Vec<String>)`.

**Why not reuse `CellValue` for both.** `CellValue` carries `Number`, and materialize has never
emitted one: `extract_field` stringifies an upstream number (`homebox.rs:559-563`). Sharing the type
would put a JSON number into `data` the first time anyone reused the wrong constructor, which is
exactly the byte-identity the acceptance criteria pin. Two types make that a compile error.

**Why untagged.** Both are serialize-only, and untagged is what makes an existing string serialize as
a bare string. A tagged or adjacent representation would rewrite every field on the wire, which is the
"make every value a list" alternative the issue already rejected, in a different costume.

### A multi-valued field with no values is `[]`, decided at the connector

`summary_to_row` and `extract_field` emit `List(vec![])` when the upstream carries no tags, rather
than omitting the key. Absence already means something on this path: `connector-field-transforms`
requires a non-matching rule to leave its key **absent**, and the grid renders a blank editable cell
for it. An untagged item is not a non-match, it is an item with zero tags, and `list-params` already
distinguishes `[]` (present and empty) from omission (resolve the default, else `422 MissingField`).
Emitting `[]` is what makes an untagged item print an empty tag strip instead of failing the label.

### One `displayCellText` helper, used by three readers

The join lives in one function that `NameCell`, `displayedCell` and `textKey` all call. The
alternative is three inline joins, and the failure mode of three is that a filter stops matching what
is on screen, which is the very disagreement `connectorFilter.ts`'s own comment says it exists to
prevent.

Ordering follows from that text with no new rule: a multi-valued cell is interpreted as the column's
declared display type *applied to its display text*, so a `text` column orders by it, a `number`
column finds `Number("1, 2")` is `NaN` and orders it with the blanks, and an empty list has empty
display text and orders with the blanks. That is why the `connector-browser` delta adds a paragraph
rather than a branch.

### The mapping refuses both directions, and pre-fills neither

The Connect page validates each `(parameter, column)` pair against the schema's `multi_valued` and the
template's declared type, and reports a mismatch naming both, blocking "Add rows".

**Why a refusal rather than filtering the `<select>` options.** Filtering alone cannot cover the
pre-fill: `defaultMapping` (`connectorRows.ts:8-13`) maps a parameter to a column of the *same key*,
so a template with a `string` parameter named `tags` would be pre-filled onto the new multi-valued
`tags` column with no operator action at all. So a validation pass is needed regardless, and once it
exists a second mechanism that hides options is a second code path saying the same thing. The pre-fill
itself learns cardinality, so it never creates a mismatch to report; a reported mismatch is always one
the operator chose, which is what makes the message worth showing.

**Why the scalar-onto-list direction is refused rather than left unmappable.** Today it is refused by a
list parameter not existing in the mapping at all. That spelling dies here, because the parameter must
become mappable. Refusing it with the same named message is the nearest thing to unchanged: the
outcome is identical, no split rule is invented, and #348 stays the open question of what a split
would mean.

### The row grid shows a multi-valued cell, read-only

`LabelGrid`'s list branch renders a muted em-dash and refuses to edit
(`LabelGrid.tsx:151`, `:155-157`, `:196`).
Editing stays refused. The rendering changes to show the display text **when the cell holds an
array**, and to keep the em-dash otherwise.

**Why this leaves the CSV and batch grids untouched in behaviour**, despite editing a shared
component: no row those screens build can hold an array for a list field. CSV rows come from
`parse_csv_rows`, which produces strings only, and manual rows are typed into scalar editors. The only
producer of an array-valued cell is `rowsFromMaterialized` on the Connect page. So the new branch is
unreachable from `Import.tsx`, and the em-dash it renders today is what it keeps rendering there.

**Why not leave the em-dash everywhere.** The cell would then hide a value the row actually holds and
the label will actually print, which is a display that lies about the data. The connector grid is the
one place a list cell has something to show.

### Assumptions recorded rather than asked

Three readings the issue leaves implicit, decided here:

1. **"Renders a multi-valued cell read-only" means rendering its value**, not keeping the em-dash. See
   above.
2. **"Mapping a scalar column onto a list parameter remains refused, unchanged" means the outcome is
   unchanged**, not the mechanism, which cannot survive a list parameter becoming mappable.
3. **`tags` carries display type `text`, not `badge`.** `badge` compares identically and would be a
   presentation claim this change has no basis for; `text` is what the elements are.

## Risks / Trade-offs

- **A client reading the schema strictly could break on the new `multi_valued` key.** → The only
  client is this repo's UI, versioned with the server, and the field is additive. Under 1.0 no
  migration follows, per the repo's breaking-change rule.
- **A client reading `data` values as strings meets an array for `tags`.** → Only for a column the
  schema marks multi-valued, and only when the caller asked for it by name. Every other field is
  byte-identical, which the acceptance tests pin directly.
- **The `tags` key could collide with a Homebox custom field named `tags`.** → It cannot: custom
  fields are keyed `custom:<name>`, so the namespaces do not meet. A derived transform field named
  `tags` is refused by the existing collision rule, now that `tags` is a declared column.
- **`Connect.tsx` will show a grid column for every `list` parameter, mapped or not.** → Accepted, and
  it is one rule rather than a mapped-only carve-out: an unmapped list column reads blank, which is
  what the row holds. The alternative, showing the column only when mapped, makes the grid's column
  set depend on the mapping, which nothing else in that grid does.
- **A required `list` parameter left unmapped still fails at submit rather than in the grid.** →
  Unchanged from today and out of scope; the server's `422 MissingField` names the parameter.
- **Sorting a `tags` column is a text sort over a joined string**, so `["B"]` sorts before
  `["A", "Z"]` is false and `A, Z` before `B` is true: the first element dominates, which is what
  joining implies. → Accepted and specified. Sorting by list length or by any single element would be
  a rule with no reader asking for it.
