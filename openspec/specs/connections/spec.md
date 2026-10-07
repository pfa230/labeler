# Connections

## Purpose

Connections to upstream inventory systems and the connectors that read them: the connection record and its endpoints, the connector schema, browse and materialize contracts, multi-valued fields, the default connection, and the Connections and Connect screens.

## Requirements

### Requirement: Connection record and reads

A connection SHALL be stored by the server as `{ id, connector, name, base_url, public_url, credential }`, with a server-generated `id`. Every response SHALL represent it as `{ id, connector, name, base_url, public_url, has_credential }`: the credential SHALL never be returned, `has_credential` SHALL say whether one is stored, and `public_url` SHALL be `null` when absent.

`GET /api/connections` SHALL list every connection ordered by `name` (byte order), ties broken by `id`. `GET /api/connections/{id}` SHALL return one connection, or `404` for an unknown id.

#### Scenario: Two connections share a name

- **WHEN** two connections are both named `Homebox`, with ids `b` and `a`
- **THEN** `GET /api/connections` lists `a` before `b` on every request

### Requirement: Creating a connection

`POST /api/connections` SHALL accept `{ connector, name, base_url, public_url?, credential }` and return `201` with the created connection. A missing or blank `public_url` SHALL store none. The request SHALL be refused with `400`:

| Fault | `details.reason` |
|---|---|
| `connector` names no registered connector (only `homebox` exists) | `connector_unknown` |
| `credential` missing or `""` | `credential_required` |
| `base_url` or `public_url` invalid | `base_url_invalid` / `public_url_invalid` |

#### Scenario: Created without a public URL

- **WHEN** a client posts a connection whose `public_url` is omitted or `""`
- **THEN** the response is `201` and its `public_url` is `null`

### Requirement: Updating a connection

`PUT /api/connections/{id}` SHALL accept `{ name, base_url, public_url?, credential? }`, replace the connection with it and return `200` with the updated connection, or `404` for an unknown id. A missing or blank `public_url` SHALL clear it. `credential` is write-only: a missing one SHALL keep the stored credential, a non-empty string SHALL replace it, and `""` SHALL be refused with `400` and `details.reason` `credential_required`. The connector is fixed at create; the update body has no `connector`.

#### Scenario: Omitting public_url clears it

- **WHEN** a client updates a connection that has a public URL with a payload that omits `public_url`
- **THEN** the response is `200` and `public_url` is `null`

#### Scenario: Omitting credential keeps it

- **WHEN** a client updates a connection with a payload that omits `credential`
- **THEN** the response is `200`, `has_credential` is `true`, and browsing still authenticates with the stored credential

### Requirement: Deleting a connection

`DELETE /api/connections/{id}` SHALL return `204`, or `404` for an unknown id. When `default_connection_id` names the deleted connection, the same atomic operation SHALL clear that setting: no reader SHALL observe the connection gone while the setting still names it, and a failure SHALL leave both intact and report the failure. Deleting any other connection SHALL leave the setting untouched.

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

`GET /api/connections/{id}/schema`, `POST /api/connections/{id}/browse` and `POST /api/connections/{id}/materialize` SHALL answer `404` for an unknown connection, and `400` with `details.reason` `connection_connector_missing` when the stored connection's connector is not registered; `PUT /api/connections/{id}` SHALL refuse such a connection the same way. They SHALL use the stored `base_url` and credential, never values from the request.

#### Scenario: Unknown connection

- **WHEN** a client browses a connection id that does not exist
- **THEN** the response is `404`

### Requirement: Outbound requests are bounded

Every upstream request SHALL be an HTTP GET to `<base_url><path>` (a path in `base_url` is kept) carrying the credential as a bearer token, through one client with a 5 s connect timeout, a 20 s overall timeout, a streamed 8 MiB response cap, no redirect following and no proxy-environment use. Error messages SHALL NOT carry the credential.

An upstream failure SHALL be `502 Upstream` with this `details.reason`:

| Situation | `details.reason` |
|---|---|
| upstream `401` or `403` | `auth` |
| upstream `429` | `rate_limited` |
| unresolvable host, timeout, transport failure | `unreachable` |
| a body that is not the expected JSON, any other non-2xx status, a response over 8 MiB | `bad_response` |

#### Scenario: A rate-limited upstream

- **WHEN** the upstream answers a browse with `429`
- **THEN** the response is `502` with `code` `Upstream` and `details.reason` `rate_limited`

### Requirement: Connector schema

`GET /api/connections/{id}/schema` SHALL return `{ version, resources, relationships }`.

- A resource SHALL be `{ id, label, view, columns, filters, fields_incomplete }`: `view` is `table` or `tree`; `fields_incomplete` (boolean, always present) is `true` when runtime discovery of the resource's fields failed.
- A column (`FieldSpec`) SHALL be `{ key, label, ty, tier, multi_valued }`, every key always present. `ty` is the display type: `text`, `number`, `money`, `date` or `badge`. `tier` is `cheap` (from the list call), `hydrated` (needs a per-row fetch) or `derived` (computed). `multi_valued` is `true` exactly when the column's value is a list of strings; `ty` is then its elements' type.
- A filter (`FilterSpec`) SHALL be `{ key, label, ty }` with `ty` `search`, `location_id` or `label_id`.
- A relationship SHALL be `{ id, label, from, to }`, linking a row of resource `from` to the rows of `to` it contains.

A failed field discovery SHALL NOT fail the schema request.

#### Scenario: Every column declares its cardinality

- **WHEN** a client reads any connection's schema
- **THEN** every `FieldSpec` carries `multi_valued`, `false` included

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
| | `custom:<name>` per upstream custom field | `<name>` | text | hydrated |

`entities` SHALL carry the filters `q` (Search, `search`), `parent` (Location, `location_id`) and `tag` (Tags, `label_id`). `locations` SHALL carry no filters. Both resources SHALL carry `fields_incomplete` `true` exactly when fetching `/api/v1/entities/fields` failed (their `custom:` columns are then absent).

Both resources SHALL be read from `/api/v1/entities` with `isLocation` `false` or `true`. `tags` SHALL be the names of the item's tags in upstream order, with no other tag attribute, and `[]` when it has none. `location` SHALL be the parent's name. `custom:<name>` SHALL be that custom field's `textValue`, else its `value`.

#### Scenario: Discovery failure is reported

- **WHEN** the upstream refuses the custom-field request
- **THEN** the schema request succeeds and each resource carries its declared columns and `fields_incomplete` `true`

#### Scenario: A browsed row carries tag names

- **WHEN** an item upstream carries the tags `KIDS` then `CONSUMABLE`
- **THEN** its browsed `tags` cell is `["KIDS", "CONSUMABLE"]`

#### Scenario: A location custom field

- **WHEN** a location upstream carries the custom field `Code` with value `BOX.123` and is materialized for `fields: ["custom:Code"]`
- **THEN** `data` is `{"custom:Code": "BOX.123"}`

### Requirement: Browse

`POST /api/connections/{id}/browse` SHALL take `{ resource, filters?, parent?, page?, page_size? }`, where `filters` maps a filter key to a string or a list of strings and `parent` is `{ relationship, key }`, and SHALL return `{ rows, has_more, count }`. A row SHALL be `{ id: { resource, key }, cells, url? }`, where `url` links the row in the source system and a cell is a JSON string, a JSON number (for `number` and `money` columns) or a JSON array of strings (for a multi-valued column, `[]` when empty). Browse SHALL make no per-row upstream request.

`page` SHALL default to 1 and `page_size` to 50; both SHALL be positive integers, anything else being refused as a malformed body (`errors`), and SHALL be passed to the upstream unchanged, with no upper bound. Each page is an independent request: a client pages by repeating `resource`, `filters` and `parent` with the next `page`.

For Homebox, `count` SHALL be the upstream total and `has_more` SHALL be `page × page_size < total`. Filters SHALL apply as follows, and anything else SHALL fail with `400` and `details.reason` `filter_invalid`:

- `q` and `parent`: one string each, trimmed, ignored when empty; `parent` filters by location id.
- `tag`: a string or list; values trimmed, empties dropped, duplicates removed; each at most 64 bytes and at most 16 after deduplication.
- the request `parent` (drill-down) filters by its `key` and SHALL NOT be combined with a `parent` filter.

#### Scenario: Paging through a resource

- **WHEN** a client browses `entities` and the upstream reports 120 items
- **THEN** the response has 50 rows, `count` 120 and `has_more` `true`, and the same request with `page: 2` returns the next 50

### Requirement: Materialize

`POST /api/connections/{id}/materialize` SHALL take `{ rows: [{ resource, key }], fields, expansion: "as_listed" }` and return `[{ source, data }]` in request order, fetching each row's detail from `/api/v1/entities/{key}`. More than 200 rows SHALL fail with `400` and `details.reason` `row_limit_exceeded`; an empty key, or a key containing `/` or starting with `.`, SHALL fail with `400` and `details.reason` `row_key_invalid`.

`data` SHALL carry exactly the requested fields. A multi-valued field SHALL be a JSON array of strings (`[]` when empty); every other value SHALL be a JSON string, never a number: an upstream number is stringified, and a field the upstream lacks or holds as another JSON type is `""`. `item_url` and `location_url` are the generated row link; `location` is the parent's name; `custom:<name>` is that custom field's value or `""`.

#### Scenario: A multi-valued field materializes as an array

- **WHEN** a row is materialized for `fields: ["name", "quantity", "tags"]` and the item is untagged
- **THEN** `data.name` and `data.quantity` are strings and `data.tags` is `[]`

### Requirement: The default connection setting

`default_connection_id` SHALL be a known setting (see `settings`) whose in-code default is `null`. `PUT /api/settings/default_connection_id` with `{ "value": <string> }` SHALL trim the string, store it and reflect it back, and SHALL fail with `400` and `details.reason` `setting_value_invalid` when the value is not a string, is blank, or names no existing connection. `DELETE` SHALL clear it with `204`.

#### Scenario: Setting the default

- **WHEN** a client sends `{ "value": "  conn-1  " }` and `conn-1` exists
- **THEN** the response is `200` with value `conn-1` and `is_default: false`

#### Scenario: Rejecting an unusable value

- **WHEN** a client sends `value` as `""`, a number, or an id no connection has
- **THEN** the response is `400` with `details.reason` `setting_value_invalid`

### Requirement: Connections page

Primary navigation SHALL carry **Connections**, directly after **Connect**, opening `/connections`. `/connections` SHALL show **Add connection** (to `/connections/new`) and a table in API order with columns Name, Connector, Base URL, Public URL (`-` when none), API key (`set` / `none`) and an **Edit** link to `/connections/{id}`. It SHALL show `Loading connections...`, `Failed to load connections.` or `No connections configured.` in place of the table while loading, on failure, and when empty. Create, edit and delete happen only in the form; the form routes render the form alone.

#### Scenario: Listing connections

- **WHEN** the list loads holding a connection with no public URL
- **THEN** its row shows `-` for Public URL and offers **Edit** and no delete

### Requirement: Default-connection control

`/connections` SHALL let the operator choose any connection, or no default, as the default connection, state that the default applies to everyone on this instance, and show the stored default as chosen. Choosing SHALL write `default_connection_id` and nothing else.

#### Scenario: Deleting the default connection

- **WHEN** the operator deletes the default connection from its form
- **THEN** they land on `/connections` and the control shows no default without a reload

### Requirement: Connection form

`/connections/new` and `/connections/{id}` SHALL offer a form with the connector (fixed, `homebox` on create), name, base URL, public URL and API key. The edit form SHALL be pre-filled from the stored connection, with the API key left blank to keep the stored one. Opening a form SHALL always show the destination's stored values, with nothing carried over from an earlier visit; an id the connections list lacks SHALL show that the connection was not found, with a way back to `/connections`.

On **Save** the form SHALL refuse, without a request, a blank name, a base URL or non-blank public URL that is not an `http`/`https` URL, and on create a blank API key. A blank public URL SHALL clear the stored one, and a blank API key on edit SHALL keep the stored one. A save error SHALL be shown. The edit form SHALL offer **Delete**, which sends nothing until confirmed.

**Save** and **Cancel** SHALL return to the page the form was opened from, Connect or `/connections`, else to `/connections`; a successful delete SHALL return to `/connections`.

#### Scenario: Clearing a public URL

- **WHEN** the operator empties **public url** on a connection that has one and saves
- **THEN** the stored connection has no public URL

#### Scenario: Keeping the stored API key

- **WHEN** the operator saves an edit with **api key** blank
- **THEN** the stored API key is kept

#### Scenario: Returning to Connect

- **WHEN** the operator follows **Manage connections** from Connect, edits a connection and saves
- **THEN** the application returns to Connect

### Requirement: Connection selection on Connect

The Connect page SHALL open on the default connection, else the first connection in API order, else none, and SHALL load its schema and first browse page without a click. It SHALL reflect connections and the default as last saved. Picking a connection by hand SHALL NOT change the default. Changing the selected connection SHALL reset the row selection, the browse table and the composer.

#### Scenario: Opening on the default

- **WHEN** the default names the second connection in API order and the operator opens Connect
- **THEN** that connection is selected and its rows are requested without a click

#### Scenario: Picking another connection

- **WHEN** the operator picks a connection other than the default
- **THEN** `default_connection_id` is unchanged

### Requirement: Connect page layout

The Connect page SHALL show a **Connection** picker (`choose a connection` plus every connection by name) and a **Manage connections** link to `/connections`. It SHALL show `Failed to load connections.` when the list fails, and `No connections configured.` with an **Add connection** link to `/connections/new` when the loaded list is empty. Once a connection and its schema are loaded it SHALL show a **Template** picker (`choose a template`; `Couldn't load templates.` on failure; the empty-templates notice when there are none) and the browse table; once a template is chosen it SHALL show the composer. The label grid, preview, copies, start slot, printer and Print/Download under the composer are owned by `ui`.

#### Scenario: No connection configured

- **WHEN** no connection exists
- **THEN** only the picker, **Manage connections** and **Add connection** are shown, with no template picker, composer or browse table

### Requirement: Field mapping and adding rows

The composer's **Field mapping** SHALL offer, for each of the template's parameters, a select of `(blank)` and every column key of every schema resource, pre-filled with the same-named column when one exists and its cardinality fits. A multi-valued column SHALL map only to a `list` parameter and a single-valued column only to a non-`list` parameter; a mismatch SHALL show `Cannot map multi-valued column "<col>" to scalar parameter "<param>".` or `Cannot map scalar column "<col>" to list parameter "<param>".` and disable adding rows.

**Add `<n>` rows** SHALL be disabled with nothing selected. It SHALL refuse more than 200 selected rows (`Select at most 200 rows at a time.`) and a grid that would exceed 500 rows (`That would exceed the 500-row limit.`), then materialize the selection for the distinct mapped columns and append one grid row per result, each mapped parameter holding the materialized value (a list stays a list, in order) or `""` when absent, and each unmapped parameter `""`. In the grid a `list` cell SHALL be read-only, showing its elements joined with `", "`, or `—` when not a list or empty.

#### Scenario: A tag list prints on the label

- **WHEN** an item tagged `KIDS` then `CONSUMABLE` is added with `tags` mapped to a `list` parameter rendered as `{tags:join(', ')}`
- **THEN** the batch carries `"tags": ["KIDS", "CONSUMABLE"]` and the label prints `KIDS, CONSUMABLE`

#### Scenario: A scalar column onto a list parameter

- **WHEN** the operator maps `name` to a `list` parameter
- **THEN** the refusal names both and no rows can be added

#### Scenario: An incompatible same-named column is not pre-filled

- **WHEN** the template declares a `string` parameter `tags` and the schema a multi-valued `tags`
- **THEN** that parameter starts as `(blank)` with no refusal shown

### Requirement: Browse table

The browse table SHALL show one tab per schema resource, the first selected, and SHALL page through the resource with **Load more** while `has_more`, starting again from the first page whenever the connection, resource, applied source filters or drill-down parent change; rows from a superseded request SHALL never be shown. A resource with filters SHALL offer them as source filters, which query the connection and restrict the whole result when applied. When the resource is the `from` of a relationship, each row SHALL let the operator drill in to browse the relationship's `to` resource under that row, showing the parent with a way to clear it. Each column shows its cell's display text, a list joined with `", "` in order; the `name` cell SHALL link to the row's `url` in a new tab.

#### Scenario: Drilling into a location

- **WHEN** the operator drills into a location row
- **THEN** the table browses `entities` under that location and shows the location with a way to clear it

### Requirement: Row selection

Each row SHALL be selectable by `resource:key`, and the selection SHALL survive tab, drill-down, sort and filter changes. At most 200 rows SHALL be selectable. While anything is selected, the page SHALL show the selected count, how many of them are among the loaded rows and how many elsewhere, and the selection grouped by resource with each row removable, and SHALL offer clearing the whole selection or only the rows elsewhere.

#### Scenario: A filtered-out row stays selected

- **WHEN** a selected row is hidden by a column filter
- **THEN** it stays selected, listed and counted among the loaded rows

#### Scenario: The selection cap

- **WHEN** 200 rows are selected
- **THEN** no further row can be selected

### Requirement: Sorting and filtering loaded rows

Each displayed column SHALL sort the loaded rows by its values, ascending or descending, one column at a time, or restore the connector's order. Each displayed column SHALL have a filter matching its display text case-insensitively as a substring, applied as the operator types and combined across columns, without a browse request. Sorting and column filters SHALL cover rows loaded later, and the page SHALL make clear that they cover only the rows loaded so far. Hiding a column SHALL clear its filter, and one control SHALL clear every source and column filter.

Switching resource tab SHALL clear the drill-down parent, the source filters, the sort and every column filter; drilling in and clearing the parent SHALL clear the sort and column filters (drilling also clears source filters). Sort and filters SHALL NOT survive leaving the page.

#### Scenario: Filtering does not re-query

- **WHEN** the operator types into a column filter
- **THEN** no browse request is made

#### Scenario: A multi-valued cell matches its display text

- **WHEN** the operator types `kids, cons` into the `tags` filter
- **THEN** the row showing `KIDS, CONSUMABLE` is shown

#### Scenario: Switching resources

- **WHEN** the operator sorts and filters one resource, then switches tab
- **THEN** the second resource shows the connector's order with no filters

### Requirement: Column visibility

The operator SHALL be able to show or hide each column, show all, or reset to the opening set, and SHALL never hide the last visible column. The opening set SHALL be the resource's `cheap` columns, or all columns when it has none. The choice SHALL be remembered in the browser per connection and resource.

#### Scenario: A resource opens with its cheap columns

- **WHEN** a resource with no remembered choice has `cheap` columns and `item_url`
- **THEN** the `cheap` columns are shown and `item_url` is not

#### Scenario: A choice is remembered

- **WHEN** the operator hides a column and later returns to the same connection and resource
- **THEN** the column is still hidden
