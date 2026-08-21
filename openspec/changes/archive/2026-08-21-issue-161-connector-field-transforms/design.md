## Context

See `proposal.md` for motivation and `specs/connector-field-transforms/spec.md` for the contract.

Three facts about the current code shape the design:

- `Connectors` (`src/connector/mod.rs`) is a static-dispatch enum wrapping one connector. Its three
  methods (`schema`, `browse`, `materialize`) are already the single funnel every connector read goes
  through, and `api.rs` calls nothing else.
- The row payloads are flat string maps: `LabelRow.data` is `BTreeMap<String, String>` and
  `DisplayRow.cells` is `BTreeMap<String, CellValue>`. Nothing downstream cares where a key came from.
- `HomeboxConnector::schema` builds its `columns` as an inline `vec![…]` and its browse cells in
  `summary_to_row`. Save-time validation needs the same column list without an upstream call, and
  `schema()` needs an upstream call only for Homebox's discovered custom fields.

**ADR.** This change adds `docs/adr/0059-connection-scoped-field-transforms.md` and its row in
`docs/adr/README.md`. Next free number checked against `main`, where `0058` is the highest.

## Goals / Non-Goals

**Goals:**

- One transform pass, connector-agnostic, that no future connector has to implement or remember.
- Save-time validation strong enough that the only runtime outcome is "matched" or "did not match".
- Schema, browse, and materialize cannot disagree about which derived fields exist.

**Non-Goals:**

- A transform language beyond a regex with named groups. No verbs, no chaining, no conditionals.
- Any preview surface that contacts the upstream. Filed as a follow-up issue instead.
- Transform-aware caching or any change to the cursor contract.

## Decisions

### The pass lives in the `Connectors` wrappers, not in a connector

The three `Connectors` methods call the connector, then apply the connection's transforms to what came
back. A connector never sees a transform.

*Alternative considered:* implement it per connector, so a connector could derive fields during its own
extraction and avoid a second map walk. Rejected: every new connector would have to re-implement the
same pass, and #161 is explicitly connector-agnostic. *Alternative considered:* apply it in `api.rs`
above the registry. Rejected: `materialize` has to inject the source field into the request before the
connector runs, which is below the API layer.

### A static `ResourceDescriptor` table is the single source of column truth

Each connector exposes a non-async `resources() -> &'static [ResourceDescriptor]`, where a descriptor
is `{ id, columns: &[ColumnDef { key, label, ty, tier }], dynamic_text_prefix: Option<&'static str> }`.
`HomeboxConnector::schema` builds its static `columns` **from** that table, then appends the custom
fields it fetches. Validation reads the same table.

This is the drift guard. `CLAUDE.md` already flags one place where the same rule is written twice
(`auto` sizing in `templates.rs` and `render/mod.rs`); making validation a second hand-maintained copy
of the column list would add another. `dynamic_text_prefix` (`Some("custom:")` for Homebox items) is
how a connector declares "keys under this prefix are text fields I discover at runtime", so a rule may
source `custom:Internal SKU` without validation needing an upstream call.

*Alternative considered:* validate `resource` and `source` against a live `schema()` call at save time.
Rejected: it makes saving a transform depend on the upstream being reachable and correctly
credentialed, which is exactly when a user is most likely to be editing the connection.

### Browse gives a one-sided guarantee, not equivalence

Browse and materialize do not read the same fields. Browse fills cells from the list response
(`summary_to_row`, `src/connector/homebox.rs:371-421`), which carries a field only when the list
happens to return it: `manufacturer`, `modelNumber` and `serialNumber` are inserted behind
`if let Some(...)` (`src/connector/homebox.rs:399-407`) and are declared `Tier::Hydrated`
(`src/connector/homebox.rs:127-134`), while materialize fetches `/api/v1/entities/{id}` per row
(`src/connector/homebox.rs:309-316`) and can always see them. Tier is not a usable proxy for browse
availability either: Homebox's `custom:` fields are `Hydrated` yet do appear in browse cells when the
list returns them (`src/connector/homebox.rs:408-419`).

So the promise is one-sided and the spec says so: a derived cell shown while browsing equals what
materialize will produce; browse may show nothing where the label will get a value. It is never
wrong, only sometimes silent.

*Alternative considered:* restrict transform sources to `Tier::Cheap` fields so the two always agree.
Rejected: it agrees by amputation — it would forbid a rule on a `custom:` field, which is one of the
likeliest places a user encodes something, for a guarantee tier does not actually deliver.
*Alternative considered:* have browse hydrate a row when a rule needs it. Rejected: one extra upstream
fetch per row destroys the browse cost model the `cheap`/`hydrated` split exists to protect.

### The source must be a declared text field, checked at save time

`CellValue` is `Text | Number` in browse but `data` is all-strings in materialize. If a rule could
source a number-valued field, it would match in materialize and not in browse (or match both but with
divergent formatting, since the two paths stringify through different code). Rejecting a non-text
source at save time removes the divergence instead of papering over it, and catches typo'd source
names in the same check.

*Alternative considered:* stringify `CellValue::Number` for the browse pass. Rejected: two
formatting paths for one value is a bug waiting to be filed, and splitting a number is not a real use
case. Documented in the spec as "a text-valued field of that resource".

### Materialize rewrites the field list down, then projects the result back

`MaterializeRequest.fields` is both what the connector fetches and what ends up in `data`, and the
connector inserts a key for **every** field it is asked for: `HomeboxConnector::materialize` loops
`req.fields` inserting `extract_field(...)` (`src/connector/homebox.rs:317-320`), and `extract_field`
falls through to `String::new()` for a key it does not know (`src/connector/homebox.rs:468-472`).
Passing a derived name down would therefore produce `location_id: ""` on every row and quietly defeat
the absent-on-no-match rule. The wrapper must do three things, in order:

1. **Rewrite down.** Build the connector's field list as: the requested fields minus every name
   derived by a rule on that resource, plus the `source` of each rule whose output was requested.
2. **Run the connector, then the transform pass** over the returned `data`.
3. **Project back.** Keep exactly the keys the caller listed. That drops a source fetched only to
   feed a rule, and it is also what leaves a non-matching derived key absent rather than empty.

*Alternative considered:* leave the derived name in the list and let the transform overwrite the
connector's empty string. Rejected: it only looks equivalent. The overwrite cannot distinguish "the
rule did not match" from "the rule matched empty", so every non-match would print blank instead of
surfacing, which is the behavior the spec exists to prevent. *Alternative considered:* return the
injected source alongside. Rejected: `rowsFromMaterialized` maps by key
(`ui/src/lib/connectorRows.ts:22-41`), so the extra key is silent noise, and `data`'s shape would
depend on the transform config rather than on the request.

### Storage is a JSON column on `connections`

`ALTER TABLE connections ADD COLUMN transforms TEXT;` holding a JSON array, read as an empty list when
`NULL`. This mirrors the existing `public_url` migration and the opaque-JSON `printers.config` column,
and `update_connection`'s existing branching on which optional fields are present extends to it.

*Alternative considered:* a `connection_transforms` table with a row per rule and an `ord` column.
Rejected for now: the list is small, bounded at 32, always read and written whole with its connection,
and never queried by any other axis. A table buys referential integrity nothing here needs and costs a
join plus ordering bookkeeping on every read.

### One `details.reason`, a precise message

A new `Reason::ConnectionTransformInvalid => "connection_transform_invalid"` covers every save-time
transform fault, with the cause and the zero-based rule index in `message`.

*Alternative considered:* a slug per fault (`transform_pattern_invalid`, `transform_output_collision`,
…). Rejected: `reason.rs` says each slug is API and renaming one is breaking, so six new permanent
strings need six reasons for a client to branch on them, and no client does — the UI shows the message
next to the offending rule, which the index already locates.

### Reserved derived names are refused at save time

A regex group name may contain `.` after its first character, so `vars.site` and `datetime.short_date`
are legal group names. Both namespaces are resolved by the renderer before request data is consulted
(`src/render/helpers.rs:73-83`) and are excluded from a template's data fields on both sides
(`src/render/mod.rs:2007-2015`, `ui/src/lib/templateFields.ts:81-82`). A derived field so named would
be advertised by the schema, offered in the mapping UI, and then never reach a label. Validation
rejects `datetime`, `datetime.*` and `vars.*` rather than shipping a field that cannot bind.

### `regex`, with a compiled-size budget

New direct dependency `regex`. The crate has no backreferences and no lookaround, so the pattern
language is bounded and matching is linear in input length: a stored rule cannot be made to backtrack.
`RegexBuilder::size_limit` caps the compiled program at 65536 bytes, well under the crate's 10 MiB
default and far above what a 512-byte pattern needs, and the 512-byte pattern and 8192-byte input
bounds from the spec are the belt to that suspenders. Compilation happens at save time; the browse and
materialize paths compile each rule once per call, not once per row.

### No chaining, enforced structurally

A derived name may not equal a declared field key, and every rule reads only the connector's output
map (outputs accumulate in a separate map, merged after the whole pass). A rule therefore cannot see
another rule's output, and the rule order in storage is presentational, not semantic. This is why the
spec can promise a flat single pass rather than merely recommend one.

### Derived columns stay default-hidden in the browse grid

`ConnectorBrowser` shows `cheap` columns by default and hides `hydrated`/`derived` behind the column
picker. Derived fields carry `tier: derived`, so they follow that convention: available in the picker,
not forced into view. Making them default-visible would shift a user's grid the moment they save a
rule, and the mapping UI — where the field actually matters — lists all columns regardless of tier.

### `PUT` validates against the stored connector, not the body

`update_connection_h` never reads `body.connector` and `Store::update_connection` takes no connector
argument (`src/api.rs:1243-1278`, `src/store.rs:675-683`): a connection's connector is fixed at
create. Transform validation on `PUT` therefore loads the stored connection first and validates
`resource` and `source` against **its** connector, so a body claiming a different connector cannot
widen what a rule may name. `transforms` joins `public_url` in using the existing
`store::UpdateField` (`Keep` / `Clear` / `Set`), which is already how "omitted keeps, empty clears" is
expressed for that column.

### UI: a rule list in the connection form, validated by the server

`ConnectionsSection` grows an add/remove list of `{ resource, source, pattern }` rows, `resource` as a
select over the connector's resources. There is no client-side regex check: the server's rejection is
the one authority, and duplicating the rules in TypeScript is how the two drift. The `400` message
renders next to the rule its index names.

## Risks / Trade-offs

- **A regex is write-only without feedback, and this change ships no live preview.** → Save-time
  validation catches every structural fault with a located message, and the Connect grid shows the
  derived columns against real rows one screen later. A follow-up issue captures a real-row preview so
  the decision is revisited with evidence rather than guessed at now.
- **`resources()` duplicates knowledge that `schema()` used to own.** → `schema()` is rewritten to
  build from `resources()`, so there is one table, not two. A test asserts the static columns of the
  schema response equal the descriptor's columns.
- **A rule can be valid and still never match** (wrong assumption about the upstream format). → The
  absent-key rule makes that visible as a blank cell in the grid before printing, rather than as a
  wrong label after.
- **Browse cannot satisfy a rule sourced from a field the list response omits**, so the grid can be
  blank where the label will not be. → Specified as a one-sided guarantee: never a different value,
  sometimes no value. The derived column is default-hidden, so the grid does not imply otherwise.
- **All-or-nothing capture.** A pattern that matches with one named group unparticipating contributes
  nothing at all. → Deliberate: a half-filled rule prints a label that looks right and is not. A user
  who wants independent parts writes two rules, which the per-resource collision check allows.
- **A `transforms` JSON blob that fails to deserialize would break reads of that connection.** → It is
  only ever written by validated input; a corrupt value is a store-level `Json` error like any other
  malformed stored config, surfaced as a `500` rather than silently dropped.

## Migration Plan

One additive `rusqlite_migration` step, `ALTER TABLE connections ADD COLUMN transforms TEXT`, following
the shape of the `public_url` migration. Existing rows read `NULL` and behave as an empty list, so the
deployment needs no data backfill and no coordination with the UI: an older UI simply never sends the
field, and an omitted `transforms` on `PUT` keeps what is stored.

Rollback is a binary rollback. The added column is ignored by the previous version's `SELECT` lists,
so a downgrade loses the feature but not the connections.
