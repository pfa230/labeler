# Connections

## Purpose

Connections to upstream inventory systems and the connectors that read them: the connection record and its endpoints, the connector schema, browse, materialize and transform-preview contracts, field transforms, multi-valued fields, the default connection, and the Connections and Connect screens.

## Requirements

### Requirement: Connection record and reads

A connection SHALL be stored by the server as `{ id, connector, name, base_url, public_url, credential, enabled, transforms }`, with a server-generated `id`. Every response SHALL represent it as `{ id, connector, name, base_url, public_url, enabled, has_credential, transforms }`: the credential SHALL never be returned, `has_credential` SHALL say whether one is stored, `public_url` SHALL be `null` when absent, and `transforms` SHALL be returned in full.

`GET /api/connections` SHALL list every connection ordered by `name` (byte order), ties broken by `id`. `GET /api/connections/{id}` SHALL return one connection, or `404` for an unknown id.

#### Scenario: Two connections share a name

- **WHEN** two connections are both named `Homebox`, with ids `b` and `a`
- **THEN** `GET /api/connections` lists `a` before `b` on every request

### Requirement: Creating a connection

`POST /api/connections` SHALL accept `{ connector, name, base_url, public_url?, credential, enabled?, transforms? }` and return `201` with the created connection. A missing, `null` or blank `public_url` SHALL store none; a missing `enabled` SHALL default to `true`; a missing or `null` `transforms` SHALL store an empty list. The request SHALL be refused with `400`:

| Fault | `details.reason` |
|---|---|
| `connector` names no registered connector (only `homebox` exists) | `connector_unknown` |
| a transform is rejected | `connection_transform_invalid` |
| `credential` missing, `null` or `""` | `credential_required` |
| `base_url` or `public_url` invalid | `base_url_invalid` / `public_url_invalid` |

#### Scenario: Created without a public URL

- **WHEN** a client posts a connection whose `public_url` is omitted, `null` or `""`
- **THEN** the response is `201` and its `public_url` is `null`

### Requirement: Updating a connection

`PUT /api/connections/{id}` SHALL accept the create payload and return `200` with the updated connection, or `404` for an unknown id; the `404` SHALL take precedence over every payload check, and SHALL also answer an update whose connection is gone when the service reads it back. It SHALL write `name`, `base_url` and `enabled`, and:

| Key | Omitted | `null` | Empty (`""` / `[]`) | Value |
|---|---|---|---|---|
| `credential` | keep | keep | keep | replace |
| `public_url` | keep | clear | clear (blank too) | replace |
| `transforms` | keep | clear | clear | replace |

`connector` SHALL be required and never written. When it differs from the stored value by exact comparison (case included, registry not consulted), the update SHALL fail with `400` and `details.reason` `connector_immutable`, change nothing, and take precedence over every other payload check. A body the request layer cannot deserialize SHALL fail there and never report `connector_immutable`.

#### Scenario: Omitting public_url keeps it

- **WHEN** a client updates a connection that has a public URL with a payload that omits `public_url`
- **THEN** the response is `200` and `public_url` is unchanged

#### Scenario: A mismatched connector outranks other faults

- **WHEN** a client updates a `homebox` connection with `connector` `Homebox` and `base_url` `not a url`
- **THEN** the response is `400` with `details.reason` `connector_immutable`, and a later read shows the connection unchanged

#### Scenario: Unknown id with a mismatched connector

- **WHEN** a client updates an id no connection has, with any `connector`
- **THEN** the response is `404`

### Requirement: Deleting a connection

`DELETE /api/connections/{id}` SHALL return `204`, or `404` for an unknown id. When `default_connection_id` names the deleted connection, the same atomic operation SHALL clear that setting: no reader SHALL observe the connection gone while the setting still names it, and a failure SHALL leave both intact and report the failure. Deleting any other connection SHALL leave the setting untouched; disabling a connection SHALL never clear it.

#### Scenario: Deleting the default connection

- **WHEN** `default_connection_id` names a connection and a client deletes it
- **THEN** the response is `204` and `GET /api/settings` reports `default_connection_id` as `null` with `is_default: true`

### Requirement: Connection URL validation

`base_url` and `public_url` SHALL each be trimmed of surrounding whitespace and SHALL be accepted only when they parse as an absolute URL with scheme `http` or `https`, a host, and no query, fragment or userinfo. Every trailing `/` SHALL be removed before storage. A rejected value SHALL fail with `400`, `details.reason` `base_url_invalid` or `public_url_invalid`, and a message naming the field.

#### Scenario: Rejecting a malformed URL

- **WHEN** a client sends `public_url` as `not a url`, `ftp://host`, `http://`, `https://host?x=1`, `https://host#f` or `https://user:pass@host`
- **THEN** the response is `400` with `details.reason` `public_url_invalid`

#### Scenario: Normalizing a stored URL

- **WHEN** a client sends `base_url` as `  http://homebox.lan:7745///  `
- **THEN** the stored and returned value is `http://homebox.lan:7745`

### Requirement: Generated links use the public URL

Every URL Labeler builds for a person to open (a browsed row's `url` and the `item_url` and `location_url` fields) SHALL be `<public_url>/entity/<url-encoded id>` when the connection has a non-blank `public_url`, and `<base_url>/entity/<id>` otherwise. Requests to the upstream SHALL always use `base_url`.

#### Scenario: Public URL set

- **WHEN** a connection has `base_url` `http://homebox:7745` and `public_url` `https://homebox.example.com`, and entity `e1` is browsed or materialized
- **THEN** its `url`, `item_url` and `location_url` are `https://homebox.example.com/entity/e1`, and the upstream request went to `http://homebox:7745`

### Requirement: Connector endpoints

`GET /api/connections/{id}/schema`, `POST /api/connections/{id}/browse`, `POST /api/connections/{id}/materialize` and `POST /api/connections/{id}/transforms/preview` SHALL answer `404` for an unknown connection, and `400` with `details.reason` `connection_connector_missing` when the stored connection's connector is not registered (the same applies to `PUT`). They SHALL use the stored `base_url` and credential, never values from the request.

#### Scenario: Unknown connection

- **WHEN** a client browses a connection id that does not exist
- **THEN** the response is `404`

### Requirement: Outbound requests are bounded

Every upstream request SHALL be an HTTP GET to `<base_url><path>` (a path in `base_url` is kept) carrying the credential as a bearer token, through one client with a 5 s connect timeout, a 20 s overall timeout, a streamed 8 MiB response cap, no redirect following and no proxy-environment use. Before connecting, the host SHALL be resolved and the request refused when any resolved address (IPv4-mapped IPv6 included) is loopback, link-local, unspecified or multicast; private LAN addresses SHALL be allowed. Error messages SHALL NOT carry the credential.

Failures SHALL map to these error codes (statuses per `errors`):

| Situation | `code` |
|---|---|
| upstream `401` or `403` | `ConnectorAuthFailed` |
| upstream `429` | `RateLimited` |
| refused address, unresolvable host, timeout, transport failure, materialize body that is not JSON | `ConnectorUnreachable` |
| browse body that is not the expected JSON | `UpstreamSchemaMismatch` |
| any other non-2xx status, response over 8 MiB | `Upstream` |
| bad browse filter, bad cursor, invalid row key | `InvalidFilter` |
| more than 200 rows to materialize | `BudgetExceeded` |

#### Scenario: A loopback target is refused

- **WHEN** a connection's `base_url` resolves to `127.0.0.1` or `169.254.169.254` and it is browsed
- **THEN** no request is sent and the response carries `code` `ConnectorUnreachable`

### Requirement: Connector schema

`GET /api/connections/{id}/schema` SHALL return `{ version, resources, relationships }`.

- A resource SHALL be `{ id, label, view, columns, filters, dynamic_source_prefix, fields_incomplete }`: `view` is `table` or `tree`; `dynamic_source_prefix` (string or `null`, always present) is the key prefix under which the connector accepts transform sources it does not enumerate; `fields_incomplete` (boolean, always present) is `true` when runtime discovery of the resource's fields failed.
- A column (`FieldSpec`) SHALL be `{ key, label, ty, tier, multi_valued, transform_source }`, every key always present. `ty` is the display type: `text`, `number`, `money`, `date` or `badge`. `tier` is `cheap` (from the list call), `hydrated` (needs a per-row fetch) or `derived` (computed). `multi_valued` is `true` exactly when the column's value is a list of strings; `ty` is then its elements' type. `transform_source` is `true` exactly when the connector declares the column single-valued `text`, whatever its tier, and `false` for every transform-derived column.
- A filter (`FilterSpec`) SHALL be `{ key, label, ty }` with `ty` `search`, `location_id` or `label_id`.
- A relationship SHALL be `{ id, label, from, to }`, linking a row of resource `from` to the rows of `to` it contains.

A failed field discovery SHALL NOT fail the schema request.

#### Scenario: Every column declares its cardinality and source eligibility

- **WHEN** a client reads any connection's schema
- **THEN** every `FieldSpec` carries `multi_valued` and `transform_source`, `false` included

### Requirement: Homebox connector

The `homebox` connector SHALL report `version` `homebox-1`, the relationship `{ id: "location_children", label: "Contents", from: "locations", to: "entities" }`, and two `table` resources:

| Resource (label) | Column | Label | `ty` | `tier` |
|---|---|---|---|---|
| `entities` (Items) | `name`, `description`, `assetId` | Name, Description, Asset ID | text | cheap |
| | `quantity` / `purchasePrice` | Quantity / Price | number / money | cheap |
| | `tags` (`multi_valued: true`) | Tags | text | cheap |
| | `location` | Location | text | cheap |
| | `manufacturer`, `modelNumber`, `serialNumber` | Manufacturer, Model, Serial | text | hydrated |
| | `item_url` | Homebox URL | text | derived |
| | `custom:<name>` per upstream custom field | `<name>` | text | hydrated |
| `locations` (Locations) | `name`, `description` | Name, Description | text | cheap |
| | `itemCount` | Items | number | cheap |
| | `location_url` | Homebox URL | text | derived |

`entities` SHALL carry the filters `q` (Search, `search`), `parent` (Location, `location_id`) and `tag` (Tags, `label_id`), `dynamic_source_prefix` `"custom:"`, and `fields_incomplete` `true` exactly when fetching `/api/v1/entities/fields` failed (its `custom:` columns are then absent). `locations` SHALL carry no filters, `dynamic_source_prefix` `null` and `fields_incomplete` `false`.

Both resources SHALL be read from `/api/v1/entities` with `isLocation` `false` or `true`. `tags` SHALL be the names of the item's tags in upstream order, with no other tag attribute, and `[]` when it has none. `location` SHALL be the parent's name. `custom:<name>` SHALL be that custom field's `textValue`, else its `value`.

#### Scenario: Discovery failure is reported

- **WHEN** the upstream refuses the custom-field request
- **THEN** the schema request succeeds and `entities` carries its declared columns and `fields_incomplete` `true`

#### Scenario: A browsed row carries tag names

- **WHEN** an item upstream carries the tags `KIDS` then `CONSUMABLE`
- **THEN** its browsed `tags` cell is `["KIDS", "CONSUMABLE"]`

### Requirement: Browse

`POST /api/connections/{id}/browse` SHALL take `{ resource, filters?, parent?, cursor?, page_size? }`, where `filters` maps a filter key to a string or a list of strings and `parent` is `{ relationship, key }`, and SHALL return `{ rows, next_cursor, has_more, count }`. A row SHALL be `{ id: { resource, key }, cells, url? }`, where `url` links the row in the source system and a cell is a JSON string, a JSON number (for `number` and `money` columns) or a JSON array of strings (for a multi-valued column, `[]` when empty). Browse SHALL make no per-row upstream request.

`page_size` SHALL default to 50 and be clamped to `1..=200`. `next_cursor` SHALL be an opaque signed token, present exactly when `has_more` is `true`, bound to the connector, connection, resource and effective filters and parent, and SHALL carry its page size forward. A cursor that is malformed, forged, issued before a restart, or presented with a different binding SHALL fail with `InvalidFilter`.

For Homebox, `count` SHALL be the upstream total and `has_more` SHALL be `page × page_size < total`. Filters SHALL apply as follows, and anything else SHALL fail with `InvalidFilter`:

- `q` and `parent`: one string each, trimmed, ignored when empty; `parent` filters by location id.
- `tag`: a string or list; values trimmed, empties dropped, duplicates removed; each at most 64 bytes and at most 16 after deduplication.
- the request `parent` (drill-down) filters by its `key` and SHALL NOT be combined with a `parent` filter.

#### Scenario: Paging through a resource

- **WHEN** a client browses `entities` and the upstream reports 120 items
- **THEN** the response has 50 rows, `count` 120, `has_more` `true` and a `next_cursor`, and posting that cursor returns the next 50

#### Scenario: A cursor reused with other filters

- **WHEN** a client posts a cursor with a `q` filter different from the one it was issued for
- **THEN** the response carries `code` `InvalidFilter`

### Requirement: Materialize

`POST /api/connections/{id}/materialize` SHALL take `{ rows: [{ resource, key }], fields, expansion: "as_listed" }` and return `[{ source, data }]` in request order, fetching each row's detail from `/api/v1/entities/{key}`. More than 200 rows SHALL fail with `BudgetExceeded`; an empty key, or a key containing `/` or starting with `.`, SHALL fail with `InvalidFilter`.

`data` SHALL carry exactly the requested fields. A multi-valued field SHALL be a JSON array of strings (`[]` when empty); every other value SHALL be a JSON string, never a number: an upstream number is stringified, and a field the upstream lacks or holds as another JSON type is `""`. `item_url` and `location_url` are the generated row link; `location` is the parent's name; `custom:<name>` is that custom field's value or `""`.

#### Scenario: A multi-valued field materializes as an array

- **WHEN** a row is materialized for `fields: ["name", "quantity", "tags"]` and the item is untagged
- **THEN** `data.name` and `data.quantity` are strings and `data.tags` is `[]`

### Requirement: Field transforms on a connection

A connection's `transforms` SHALL be an ordered list of `{ resource, source, pattern }`: `resource` is a resource id, `source` a field key of that resource, and `pattern` a regular expression whose named capture groups name the fields the rule derives. The stored order SHALL be preserved. An empty list SHALL change nothing about the schema, browse or materialize.

#### Scenario: A connection round-trips its transforms

- **WHEN** a connection is created with two transforms
- **THEN** every read returns both, in the order supplied

### Requirement: Transform validation

`POST` and `PUT /api/connections` (and the transform preview) SHALL validate the whole list without contacting the upstream, and SHALL refuse it with `400`, `details.reason` `connection_transform_invalid`, a message `rule <index>: <cause>` naming the first offending rule by zero-based index (index 32 for an over-long list), and nothing stored, when:

- the list holds more than 32 rules;
- a `pattern` exceeds 512 bytes, does not compile, compiles to more than 65536 bytes, or declares no named group;
- `resource` is not a resource of the connector;
- `source` is neither a `transform_source` column of that resource nor a key starting with its `dynamic_source_prefix` (a prefixed key is accepted unproven);
- a group name equals a column the connector declares for that resource, repeats within the rule, is derived by another rule on the same resource, or does not match `^[a-zA-Z0-9_-]+$`.

The same name MAY be derived on two different resources. A connection whose upstream is unreachable SHALL still save valid transforms.

#### Scenario: A multi-valued source is refused

- **WHEN** a transform on `entities` sources `tags`
- **THEN** the response is `400` with `details.reason` `connection_transform_invalid` and a message naming rule 0 and `tags`

#### Scenario: A derived name that is not a bare token name

- **WHEN** a transform derives `datetime.short_date`, `vars.site` or `printed_on:long_date`
- **THEN** the save is refused, while a group named `datetime` is accepted

#### Scenario: A source under the dynamic prefix

- **WHEN** a transform on `entities` sources `custom:Internal SKU` that the upstream does not have
- **THEN** the save succeeds and the rule never matches

### Requirement: Derived fields

For each rule whose `resource` names a schema resource, the schema SHALL add to that resource one column per capture-group name, keyed and labelled by the name, with `ty` `text`, `tier` `derived`, `multi_valued` `false` and `transform_source` `false`. A stored rule naming a resource the connector does not offer SHALL be inert.

A rule SHALL derive fields for a row only when its source value is present, at most 8192 bytes, matched by the pattern, and every named group participates in the match; a participating group that captures nothing yields `""`. Otherwise the rule SHALL contribute nothing to that row: its derived keys are absent, never empty or the unsplit source, and the call and other rows and rules are unaffected. Rules SHALL read only the connector's own fields for the row, so no rule reads another's output.

Browse SHALL add derived cells from the cells the connector produced, without fetching more. Materialize SHALL accept a derived name in `fields`, fetch the rule's `source` when needed without returning it unless requested, and apply rules only to rows of the rule's resource. A browsed derived cell SHALL equal what materialize produces for that row; browse MAY lack a cell that materialize produces when the source is a hydrated field.

#### Scenario: A derived field is materialized without its source

- **WHEN** a rule on `entities` derives `location_id` and `location_name` from `location` `BOX.123 | Motorcycle parts`, and that row is materialized for `fields: ["location_id"]`
- **THEN** `data` is `{"location_id": "BOX.123"}`

#### Scenario: Groups that do not all participate

- **WHEN** a pattern with named groups in different alternation branches matches a row
- **THEN** the row carries no field from that rule

#### Scenario: A non-matching row among many

- **WHEN** 200 rows are materialized and one row's source does not match
- **THEN** the call succeeds and only that row lacks the derived keys

### Requirement: Transform preview

`POST /api/connections/{id}/transforms/preview` SHALL take `{ transforms, rule, page_size? }`, where `transforms` is a candidate list and `rule` the index of the rule to preview. A `rule` that indexes no entry SHALL fail with `400` and `details.reason` `request_body_invalid`; otherwise the whole list SHALL be validated as a save validates it, before any upstream request. It SHALL then browse the first page of the rule's resource with no filters, `page_size` defaulting to 10 and clamped to `1..=200`, evaluate at most that many rows, and evaluate the rule exactly as the read path does. Nothing SHALL be stored; upstream failures SHALL be reported as browse reports them.

The response SHALL be `{ rule, resource, source, row_count, matched_count, rows }` with one entry per evaluated row in browse order: `{ id, source_value?, matched, value_truncated, derived? }`. `source_value` SHALL be present exactly when the row carries a text value for the source; `derived` (every captured name to its value) exactly when `matched` is `true`. Each reported value SHALL be cut to at most 512 bytes on a character boundary, setting `value_truncated` `true`; matching SHALL use the full value.

#### Scenario: A rule that matches some rows

- **WHEN** a rule is previewed over 10 rows, 7 of which match
- **THEN** the response is `200` with `row_count` 10, `matched_count` 7 and 10 `rows` entries

#### Scenario: An invalid rule elsewhere in the list

- **WHEN** rule 0 is previewed and rule 1 declares no named group
- **THEN** the response is `400` with `details.reason` `connection_transform_invalid`, the message names rule 1, and no upstream request is made

#### Scenario: An over-returning upstream

- **WHEN** a preview asks for 10 rows and the upstream returns 50
- **THEN** `row_count` is 10

### Requirement: The default connection setting

`default_connection_id` SHALL be a known setting (see `settings`) whose in-code default is `null`. `PUT /api/settings/default_connection_id` with `{ "value": <string> }` SHALL trim the string, store it and reflect it back, and SHALL fail with `400` and `details.reason` `setting_value_invalid` when the value is not a string, is blank, or names no existing connection. A disabled connection SHALL be accepted. `DELETE` SHALL clear it with `204`. A stored, non-blank id that names no connection SHALL be reported as stored.

#### Scenario: Setting the default

- **WHEN** a client sends `{ "value": "  conn-1  " }` and `conn-1` exists
- **THEN** the response is `200` with value `conn-1` and `is_default: false`

#### Scenario: Rejecting an unusable value

- **WHEN** a client sends `value` as `""`, `null`, a number, or an id no connection has
- **THEN** the response is `400` with `details.reason` `setting_value_invalid`

### Requirement: Connections page

Primary navigation SHALL carry **Connections**, directly after **Connect**, opening `/connections`. `/connections` SHALL show **Add connection** (to `/connections/new`) and a table in API order with columns Name, Connector, Base URL, Public URL (`-` when none), API key (`set` / `none`), Enabled (`yes` / `no`) and an **Edit** link to `/connections/{id}`. It SHALL show `Loading connections...`, `Failed to load connections.` or `No connections configured.` in place of the table while loading, on failure, and when empty. Create, edit, enable and delete happen only in the form; the form routes render the form alone.

#### Scenario: Listing connections

- **WHEN** the list loads holding a connection with no public URL
- **THEN** its row shows `-` for Public URL and offers **Edit** and no delete

### Requirement: Default-connection control

`/connections` SHALL carry a **Default connection** select stating `The default connection applies to everyone on this instance.` It SHALL offer `(no default)`, which sends `DELETE /api/settings/default_connection_id`, and each connection as `<name> (<id>)`, suffixed ` (disabled)` when disabled, which sends `PUT` with that id. It SHALL show the stored default as selected; when the loaded list lacks the stored id, it SHALL show `<id> (unavailable)`, still clearable. While the list is loading or failed, or a write is pending, the select SHALL be disabled and SHALL NOT mark the stored id unavailable. Writing the default SHALL change nothing else.

#### Scenario: The stored default names no connection

- **WHEN** the list loads and the stored id names no connection
- **THEN** the control shows `<id> (unavailable)` and offers `(no default)`

#### Scenario: Deleting the default connection

- **WHEN** the operator deletes the default connection from its form
- **THEN** they land on `/connections` and the control shows `(no default)` without a reload

### Requirement: Connection form

`/connections/new` (titled `New connection`) and `/connections/{id}` (titled `Edit <name>`) SHALL render a single-column form with a **Details** section (connector, fixed and not editable, `homebox` on create; name; base url; public url; api key as a password field; enabled, checked by default on create) and a **Field transforms** section, which on create SHALL hold only the text `Transform rules can be added after saving the connection.` The edit form SHALL be pre-filled from the stored connection, with an empty api key labelled `api key (leave blank to keep)`.

On **Save** the form SHALL trim every value and refuse, without a request, a blank name (`name must not be empty`), a base url or non-blank public url that is not a parseable `http`/`https` URL, and on create a blank api key (`api key is required`). It SHALL send `public_url: null` when blank, `credential` only when non-blank, and `transforms` only when the rule editor is live; create SHALL send no `transforms`. A save error SHALL be shown and toasted; one whose message starts `rule <n>:` SHALL be shown against rule `n`.

The edit form SHALL offer **Delete**, which sends nothing until **Confirm**. Each entry into a form route SHALL be a fresh editor: navigating between entries, including back to the same id, SHALL show the destination's stored values with no draft, preview or message carried over, and a request still in flight SHALL change nothing on the new entry.

`/connections/{id}` SHALL resolve on a cold load: `Loading connections...` while the list loads, `Failed to load connections.` on failure, and `Connection "<id>" not found.` with a **Back to connections** link when the loaded list lacks the id.

#### Scenario: Clearing a public URL

- **WHEN** the operator empties **public url** on a connection that has one and saves
- **THEN** the request carries `public_url: null`

#### Scenario: Keeping the stored API key

- **WHEN** the operator saves an edit with **api key** blank
- **THEN** the request carries no `credential`

#### Scenario: An invalid URL in the form

- **WHEN** the operator types `homebox.example.com` into **public url** and saves
- **THEN** the form shows a validation error and sends no request

### Requirement: Where a connection form returns to

A form route SHALL honor an origin carried in router state as `from` only when it is a string starting with `/` but not `//` that resolves to the application's own origin. **Save** and **Cancel** SHALL navigate to an honored origin, else `/connections`; a successful delete SHALL navigate to `/connections`. The Connect page's **Manage connections** link and its empty-list call to action SHALL record `/connect`, and `/connections` SHALL pass the origin it received to its **Add connection** and **Edit** links. A write that completes after the operator left the entry that started it SHALL navigate nowhere.

#### Scenario: Returning to Connect by way of the list

- **WHEN** the operator follows **Manage connections** from Connect, presses **Edit**, and saves
- **THEN** the application navigates to `/connect`

#### Scenario: An origin outside the application

- **WHEN** the form is reached with `from` `https://elsewhere.example.com/` or `//elsewhere.example.com`
- **THEN** **Save** and **Cancel** navigate to `/connections`

### Requirement: Transform rule editor

On the edit form the **Field transforms** section SHALL list the rules from `GET /api/connections/{id}/schema`-driven controls: a **resource** select over the schema's resource ids; a **source** select offering the resource's `transform_source` columns and, when `dynamic_source_prefix` is set, a `<prefix><name>` choice that reveals an input for the name alone (the source becomes prefix plus name); a **pattern** input; **Preview**; and **Remove**. **+ Add rule** SHALL append a rule on the first resource with its first `transform_source` column (else the prefix) and an empty pattern. Changing the resource SHALL reset the source likewise. A stored resource or source the schema does not offer SHALL be shown as `<value> (unavailable)` and kept. The editor SHALL show `Field list may be short; fields can still be specified by name.` when `fields_incomplete`, and `No transformable sources available for this resource.` when there is neither a source column nor a prefix.

The editor SHALL be read-only, listing the stored rules with a reason, and the save SHALL omit `transforms`, while the schema request is failing (`Failed to load connector schema. Transform rules cannot be edited.`) or while the form's base url differs from the stored one or an api key is typed (`Connection details must be saved first.`); entering that state SHALL discard any preview result.

**Preview** SHALL post every rule in editor order with that rule's index. The panel SHALL show `Matched <m> of <n> rows` and, per row, `<resource>:<key>`, the source value or `(no source value)`, `(shortened)` when truncated, and the derived `name="value"` pairs, `(no captures)`, or `Did not match`. A refusal naming a rule SHALL be shown against that rule. Any edit to any rule SHALL discard displayed results, and only the response to the latest preview request of a rule over the unedited list SHALL ever be displayed.

#### Scenario: The source select offers exactly the accepted sources

- **WHEN** the schema reports for `entities` `location` and `item_url` with `transform_source` `true`, `tags` and `location_id` with `false`, and prefix `custom:`
- **THEN** the source select offers `location`, `item_url` and `custom:<name>`

#### Scenario: Editing the base url suspends the editor

- **WHEN** the operator changes **base url** while a preview result is shown, then saves
- **THEN** the result disappears, the rules are read-only with `Connection details must be saved first.`, and the request carries no `transforms`

#### Scenario: A late response is discarded

- **WHEN** the operator previews a rule, edits any rule, and the response then arrives
- **THEN** nothing from it is displayed

### Requirement: Connect resolves a connection per visit

On each visit the Connect page SHALL resolve, once, the connection named by `default_connection_id` when it exists and is enabled, else the first enabled connection in API order, else none; a failed settings read SHALL count as no default, and a failed connections read SHALL resolve none. It SHALL resolve only after both reads answered or failed and no connection-management write is in flight, and SHALL then select the connection as if picked, loading its schema and first browse page. Later refetches SHALL NOT re-resolve. Leaving the page ends the visit: a return resolves afresh and restores no hand-picked connection, rows, selection or composer.

When a loaded list no longer offers the selected connection as enabled, the selection SHALL clear to `choose a connection`. Every change of selected connection, including that clearing, SHALL reset the row selection, the browse table and the composer. Picking a connection by hand SHALL NOT write `default_connection_id`.

#### Scenario: The stored default is disabled

- **WHEN** the default names a disabled connection and another is enabled
- **THEN** the first enabled connection is selected and its rows are requested without a click

#### Scenario: A later refetch does not move the operator

- **WHEN** the operator works on a resolved connection and the settings refetch with a different default
- **THEN** the selection, browse table and row selection are unchanged

#### Scenario: Another operator disables the selected connection

- **WHEN** the connections list refetches without the selected connection enabled
- **THEN** the picker shows `choose a connection` and no browse table, selection or composer remains

### Requirement: Connection writes settle before Connect acts

A connection-management write is a connection create, update or delete, or a write or clear of `default_connection_id`. A successful write SHALL evict, not merely mark stale, the cached answers it affects: create, update and delete evict the connections list; update and delete evict that connection's schema; delete and default writes evict the settings. A failed write SHALL evict nothing. Eviction SHALL happen whether or not the view that started the write is still shown. While any such write is in flight, the Connect page SHALL show `Waiting...`, disable its picker, and resolve, read and browse nothing.

#### Scenario: A saved rule shows on the next visit

- **WHEN** the operator saves a rule deriving `location_id` and returns to Connect with that connection and resource selected
- **THEN** the schema and rows are read after the save, and the table shows a `location_id` column

#### Scenario: Leaving the form while a save is in flight

- **WHEN** the operator presses **Save**, then **Cancel** to Connect before the response
- **THEN** Connect shows `Waiting...` until the save settles, then resolves from a list read after it, and the save does not move the operator

### Requirement: Connect page layout

The Connect page SHALL show a **Connection** picker (`choose a connection` plus enabled connections by name) and a **Manage connections** link to `/connections`. It SHALL show `Failed to load connections.` when the list fails, and `No connections configured.` with an **Add connection** link to `/connections/new` when the loaded list is empty. Once a connection and its schema are loaded it SHALL show a **Template** picker (`choose a template`; `Couldn't load templates.` on failure; the empty-templates notice when there are none) and the browse table; once a template is chosen it SHALL show the composer. The label grid, preview, copies, start slot, printer and Print/Download under the composer are owned by `ui`.

#### Scenario: Nothing resolves

- **WHEN** no connection is enabled
- **THEN** only the picker and **Manage connections** are shown, with no template picker, composer or browse table

### Requirement: Field mapping and adding rows

The composer's **Field mapping** SHALL offer, for each of the template's inputs, a select of `(blank)` and every column key of every schema resource, pre-filled with the same-named column when one exists and its cardinality fits. A multi-valued column SHALL map only to a `list` input and a single-valued column only to a non-`list` input; a mismatch SHALL show `Cannot map multi-valued column "<col>" to scalar parameter "<param>".` or `Cannot map scalar column "<col>" to list parameter "<param>".` and disable adding rows.

**Add `<n>` rows** SHALL be disabled with nothing selected. It SHALL refuse more than 200 selected rows (`Select at most 200 rows at a time.`) and a grid that would exceed 500 rows (`That would exceed the 500-row limit.`), then materialize the selection for the distinct mapped columns and append one grid row per result, each mapped input holding the materialized value (a list stays a list, in order) or `""` when absent, and each unmapped input `""`. In the grid a `list` cell SHALL be read-only, showing its elements joined with `", "`, or `—` when not a list or empty.

#### Scenario: A tag list prints on the label

- **WHEN** an item tagged `KIDS` then `CONSUMABLE` is added with `tags` mapped to a `list` input rendered as `{tags:join(', ')}`
- **THEN** the batch carries `"tags": ["KIDS", "CONSUMABLE"]` and the label prints `KIDS, CONSUMABLE`

#### Scenario: A scalar column onto a list input

- **WHEN** the operator maps `name` to a `list` input
- **THEN** the refusal names both and no rows can be added

#### Scenario: An incompatible same-named column is not pre-filled

- **WHEN** the template declares a `string` input `tags` and the schema a multi-valued `tags`
- **THEN** that input starts as `(blank)` with no refusal shown

### Requirement: Browse table

The browse table SHALL show one tab per schema resource (the first selected), reset to the first page whenever the connection, resource, applied filters or drill-down parent change, and append further pages through **Load more** while `has_more`; a response superseded by a newer request SHALL be dropped. Each column shows its cell's display text: a list joined with `", "` in order (`""` when empty), any other value as text. The `name` cell SHALL link to the row's `url` in a new tab. When the resource is the `from` of a relationship, each row SHALL offer **Drill in**, which browses the relationship's `to` resource under that row and shows `in <name>` with a **clear** control.

Each row SHALL have a selection checkbox; selection is by `resource:key` and SHALL survive sorting, filtering, drill-down and tab changes. At 200 selected rows every unselected checkbox SHALL be disabled. While anything is selected, a summary SHALL show `<n>/200 selected (<x> in this view, <y> elsewhere)`, where in this view counts selected rows among the loaded rows, with **Clear all**, **Clear hidden** when `y > 0`, and the selection grouped by resource label with a count and removable `<name> · <location>` chips.

#### Scenario: A filtered-out row stays selected

- **WHEN** a selected row is hidden by a column filter
- **THEN** it stays selected, listed and counted in this view

### Requirement: Column visibility

A **Columns (`<shown>/<total>`)** control SHALL let the operator toggle each column, show **All**, or **Reset** to the opening set, and SHALL never hide the last visible column. The opening set SHALL be the resource's `cheap` columns plus its transform-derived columns (`tier` `derived` and `transform_source` `false`), or all columns when that is empty. The choice SHALL be stored in the browser per connection and resource, recording visible columns and hidden transform-derived columns: on restore, a transform-derived column is shown unless recorded hidden, other columns follow the recorded visible set, and an empty result falls back to the opening set. A choice recorded as a plain list of visible keys SHALL show every transform-derived column.

#### Scenario: A resource opens with the columns a rule derived

- **WHEN** a resource with no stored choice has `cheap` columns, `item_url` and a rule-derived `location_id`
- **THEN** the `cheap` columns and `location_id` are shown, and `item_url` is not

#### Scenario: A new derived column appears in a customized resource

- **WHEN** the operator has hidden some columns and then saves a rule deriving a new name
- **THEN** on the next visit the new column is shown and the hidden ones stay hidden

### Requirement: Sorting loaded rows

Each displayed column's header SHALL cycle ascending, descending, unsorted (the connector's order), one column at a time, with the state exposed visually and through `aria-sort`. `text` and `badge` columns compare display text case-insensitively; `number` and `money` compare finite numbers; `date` compares values matching ISO-8601 date or date-time. Absent, empty or uninterpretable cells, including multi-valued cells in non-text columns, SHALL sort after every value in both directions; ties keep the connector's order. The order SHALL cover rows appended later.

#### Scenario: Uninterpretable values sort with the blanks

- **WHEN** a `number` column holding 2, `n/a` and 10 is sorted ascending, then descending
- **THEN** the orders are 2, 10, `n/a` and 10, 2, `n/a`

#### Scenario: A multi-valued text column

- **WHEN** a `tags` column holding `["KIDS", "CONSUMABLE"]`, `["ATTIC"]` and `[]` is sorted ascending
- **THEN** the order is `ATTIC`, `KIDS, CONSUMABLE`, then the empty list

### Requirement: Filtering loaded rows

Every displayed column SHALL have a header filter (`Filter by <label>`) matching its display text case-insensitively as a substring, applied as the operator types, combined across columns with AND, and covering later pages, without a browse request. Hiding a column SHALL clear its filter. These filters SHALL be described once, for assistive technology too, as `Refine loaded rows` / `Narrow the rows already loaded, as you type.`

A resource with filters SHALL present them in a `Source filters` group described as `Queries the connection and restricts the whole result. Takes effect on Apply.`; **Apply** sends the trimmed non-empty values and the tags (including one still typed). The `tag` filter is a chip input: Enter or **Add** appends a trimmed, unique tag, and `×` removes one.

**Clear all filters** SHALL appear whenever any source or column filter is set and SHALL clear both kinds. Below the grid a status region SHALL show `Showing <shown> of <loaded> loaded rows` while a column filter is active and `Sorting and refining cover only the <loaded> rows loaded so far` while `has_more`, both replaced by `No loaded row matches. More rows can be loaded.` when a column filter matches nothing and `has_more`.

#### Scenario: Filtering does not re-query

- **WHEN** the operator types into a column filter
- **THEN** no browse request is made and the cursor is unchanged

#### Scenario: A multi-valued cell matches its display text

- **WHEN** the operator types `kids, cons` into the `tags` filter
- **THEN** the row showing `KIDS, CONSUMABLE` is shown

### Requirement: View controls reset with the browsing context

Switching resource tab SHALL clear the drill-down parent, the source filters, the sort and every column filter; drilling in and clearing the parent SHALL clear the sort and column filters (drilling also clears source filters). Sort and filters SHALL NOT survive leaving the page. A column that a refetched schema no longer offers SHALL stop ordering and narrowing the table without clearing its sort or filter, which resume if the column returns.

#### Scenario: Switching resources

- **WHEN** the operator sorts and filters one resource, then switches tab
- **THEN** the second resource shows the connector's order with no filters

#### Scenario: A sorted column is removed by a save

- **WHEN** the operator sorts by a derived column and a refetched schema no longer offers it
- **THEN** the rows show in the connector's order
