# connections Specification

## Purpose
Defines the connection record that binds Labeler to an upstream inventory system, its CRUD contract,
and the two addresses a connection carries: the base URL Labeler fetches from and the optional public
URL that Labeler puts into the links and QR codes it generates for people.

## Requirements

### Requirement: Connection record

A connection SHALL be `{ id, connector, name, base_url, public_url, transforms, credential, enabled }`,
persisted by the server. `public_url` SHALL be optional and default to absent. The credential SHALL
never be returned by any endpoint; responses SHALL expose it only as the boolean `has_credential`;
`transforms` SHALL be returned in full.

This requirement supersedes the connection record shape in the frozen `docs/SPEC.md` §12
("Integrations (connectors)"). Every other part of §12 remains authoritative. `transforms` is named
here only so the record is stated whole: every rule about what a transform is, when it is accepted,
and what it derives belongs to `connector-field-transforms`, which this requirement neither restates
nor alters.

`GET /api/connections` SHALL list every connection ordered by `name`, ties broken by `id`, so the
listed order is total: two connections sharing a name SHALL still list in the same order on every
request. `GET /api/connections/{id}` SHALL return one, or `404` when the id is unknown.

#### Scenario: Reading a connection

- **WHEN** a client reads a connection through `GET /api/connections` or `GET /api/connections/{id}`
- **THEN** the response contains `id`, `connector`, `name`, `base_url`, `public_url`, `transforms`,
  `enabled`, and `has_credential`
- **AND** it contains no credential value
- **AND** `public_url` is `null` when the connection has none

#### Scenario: Reading a connection that does not exist

- **WHEN** a client requests `GET /api/connections/{id}` for an unknown id
- **THEN** the response is `404`

#### Scenario: Listing connections that share a name

- **WHEN** two connections are both named `Homebox`, with ids `b` and `a`
- **THEN** `GET /api/connections` lists `a` before `b`, on every request

### Requirement: Creating a connection

`POST /api/connections` SHALL accept
`{ connector, name, base_url, public_url?, transforms?, credential, enabled? }` and return `201` with
the created connection. A missing, `null`, or blank `public_url` SHALL store no
public URL. A missing `enabled` SHALL default to `true`. An unknown `connector` SHALL be rejected with
`400` and reason `connector_unknown`; a `credential` that is missing, `null`, or empty SHALL be
rejected with `400` and reason `credential_required`.

This requirement supersedes the `POST /api/connections` payload description in the frozen
`docs/SPEC.md` §12, alongside `connector-field-transforms`, which already superseded it to the extent
of adding `transforms`. The handling of `transforms` on create stays that capability's.

#### Scenario: Created with a public URL

- **WHEN** a client posts a connection whose `public_url` is `https://homebox.example.com`
- **THEN** the response is `201` and its `public_url` is `https://homebox.example.com`

#### Scenario: Created without a public URL

- **WHEN** a client posts a connection that omits `public_url`, or sends it as `null` or `""`
- **THEN** the response is `201` and its `public_url` is `null`

#### Scenario: Created without a credential

- **WHEN** a client posts a connection whose `credential` is missing, `null`, or `""`
- **THEN** the response is `400` with `details.reason` `credential_required`

#### Scenario: Created with an unknown connector

- **WHEN** a client posts a connection whose `connector` is not a registered connector
- **THEN** the response is `400` with `details.reason` `connector_unknown`

### Requirement: Updating a connection

`PUT /api/connections/{id}` SHALL accept the same payload shape as create and return `200` with the
updated connection, or `404` when the id is unknown. It SHALL update `name`, `base_url`, `public_url`,
`enabled`, and, when one is supplied, `credential`. `connector` SHALL remain a required key of the
payload and SHALL still never be applied, because a connection's connector is fixed at creation; but
a payload whose `connector` is not exactly the stored one SHALL be rejected with `400` and
`details.reason` `connector_immutable`, and SHALL change nothing. The comparison SHALL be exact
equality against the stored value and SHALL NOT consult the connector registry, so a name that is not
a registered connector is rejected by the same rule and carries the same reason rather than
`connector_unknown`. The unknown-id `404` SHALL take precedence: a `PUT` to an id that does not exist
SHALL be `404` whatever its `connector` says. Omitting `credential`, or sending it empty, SHALL keep
the stored credential. For `public_url` the three input forms SHALL be distinguished: omitting the key
keeps the stored value, sending `null` or a blank string clears it, and sending a URL replaces it.
`transforms` is accepted on update under the rules stated by `connector-field-transforms`.

Rejecting the mismatch rather than absorbing it is what makes the two parties agree: a client that
sends a connector Labeler will not apply has a bug, and a `200` that changes nothing hides it.

The rejection SHALL precede every check the update performs on the rest of the payload, so a payload
carrying both a mismatched `connector` and an otherwise invalid field reports `connector_immutable`.
It cannot precede reading the payload itself: a body that does not deserialize into the update shape
is rejected by the request layer before any `connector` is available to compare, and SHALL NOT report
`connector_immutable`. What that rejection does report is the request layer's, and this requirement
does not specify it.

`connector_immutable` is a new entry in the error contract, and this requirement is its published
home: the frozen `docs/SPEC.md` §10.1 is not edited, and its existing rows remain authoritative. The
complete mapping is `code` `InvalidRequest`, status `400`, reason `connector_immutable`, raised when
the `connector` in a `PUT /api/connections/{id}` payload differs from the stored connector of the
connection being updated. The response body SHALL carry it in the standard error shape, as
`error.code` `InvalidRequest` with `error.details.reason` `connector_immutable`.

This requirement supersedes the `PUT /api/connections/{id}` description in the frozen
`docs/SPEC.md` §12, alongside `connector-field-transforms` for the `transforms` key.

#### Scenario: Omitting public_url keeps the stored one

- **WHEN** a client updates a connection that has a public URL, with a payload that omits `public_url`
- **THEN** the response is `200` and the connection still has its previous `public_url`

#### Scenario: Blanking public_url clears it

- **WHEN** a client updates a connection that has a public URL, sending `public_url` as `null` or `""`
- **THEN** the response is `200` and the connection's `public_url` is `null`

#### Scenario: Setting a new public_url

- **WHEN** a client updates a connection sending `public_url` as `https://homebox.example.com/`
- **THEN** the response is `200` and the connection's `public_url` is `https://homebox.example.com`

#### Scenario: Connector in an update payload is ignored

- **WHEN** a client updates a connection sending the `connector` it already has
- **THEN** the response is `200`, the rest of the payload is applied, and the connection's
  `connector` is unchanged, the supplied value having been compared and never written

#### Scenario: Updating with a connector that is not the stored one

- **WHEN** a client updates a connection stored as `homebox`, sending `connector` as any other value,
  whether that value names another registered connector, a name no connector has, or the stored name
  in different case
- **THEN** the response is `400` with `details.reason` `connector_immutable`, never
  `connector_unknown`
- **AND** a subsequent read shows the connection unchanged, including the fields the rejected payload
  would otherwise have updated

#### Scenario: A connector mismatch outranks a field the update itself rejects

- **WHEN** a client updates a connection with a payload whose `connector` is not the stored one and
  which also carries an invalid `base_url` such as `not a url`, an invalid `public_url`, or an
  invalid transform rule
- **THEN** the response is `400` with `error.code` `InvalidRequest` and `details.reason`
  `connector_immutable`, rather than `base_url_invalid`, `public_url_invalid` or
  `connection_transform_invalid`

#### Scenario: A payload that is not a valid update body never reaches the comparison

- **WHEN** a client sends a `PUT` body that cannot be read as an update payload at all, because it is
  not valid JSON, omits a required key, or gives one the wrong type, and whose `connector` would also
  have mismatched
- **THEN** the request layer rejects it and the response does not report `connector_immutable`,
  because the payload is rejected before any `connector` exists to compare

#### Scenario: Updating a connection that does not exist

- **WHEN** a client updates an unknown id
- **THEN** the response is `404`

#### Scenario: Updating an unknown id with a mismatched connector

- **WHEN** a client updates an unknown id with a payload whose `connector` is also not the one any
  connection holds
- **THEN** the response is `404`, not `400`

### Requirement: Deleting a connection

`DELETE /api/connections/{id}` SHALL delete the connection and return `204`, or `404` when the id is
unknown. When the deleted connection is the one named by the `default_connection_id` setting, the
delete SHALL also clear that setting, so no stored default can outlive the connection it names.

The two SHALL be one atomic operation: either the connection is gone and the setting is cleared, or
neither happened. No reader SHALL ever observe a state in which the connection is deleted and the
setting still names it, and a failure part-way SHALL leave the connection in place with its setting
intact and SHALL report the failure rather than `204`. Deleting a connection that is not the default
SHALL leave the setting untouched.

This mirrors the cascade a template delete already performs on favorites, and is the only cleanup
`default_connection_id` gets: a connection that is merely disabled keeps the setting, and the Connect
page falls through to its fallback for as long as it stays disabled. The setting itself, its
validation and the resolution order are specified by `default-connection`, which this requirement
neither restates nor alters.

This requirement supersedes the `DELETE /api/connections/{id}` row of the frozen `docs/SPEC.md` §12.

#### Scenario: Deleting a connection

- **WHEN** a client deletes an existing connection
- **THEN** the response is `204` and the connection no longer appears in `GET /api/connections`

#### Scenario: Deleting a connection that does not exist

- **WHEN** a client deletes an unknown id
- **THEN** the response is `404`

#### Scenario: Deleting the default connection

- **WHEN** `default_connection_id` names a connection and a client deletes that connection
- **THEN** the response is `204`
- **AND** `GET /api/settings` reports `default_connection_id` as `null` with `is_default: true`
- **AND** no read of `GET /api/settings` at any point returns the deleted id while the connection is
  already absent from `GET /api/connections`

#### Scenario: Deleting a connection that is not the default

- **WHEN** `default_connection_id` names one connection and a client deletes a different one
- **THEN** the response is `204` and `default_connection_id` still names the first connection

### Requirement: Connection URL validation

`base_url` and `public_url` SHALL each be validated the same way before storage: surrounding
whitespace trimmed, parseable as an absolute URL, scheme `http` or `https`, a host present, and no
query string, fragment, or embedded userinfo (`user:pass@`). Userinfo is rejected because a connection
URL is printed into QR codes and rendered as a link, so credentials carried in it would end up on a
physical label. Every trailing `/` SHALL be trimmed before storage, so `https://host/sub/path///` and
`https://host/sub/path` store the same value. A rejected value SHALL produce `400` with
`details.reason` `base_url_invalid` for `base_url` and `public_url_invalid` for `public_url`, and a
message naming the rejected field. A save SHALL also be rejected for a bad transform, under the rules
and the `connection_transform_invalid` reason stated by `connector-field-transforms`.

This requirement supersedes the `base_url_invalid` row of the reason table in the frozen
`docs/SPEC.md` §10.1 and extends it with `public_url_invalid`. The other rows remain authoritative.

#### Scenario: Rejecting a malformed public URL

- **WHEN** a client sends `public_url` as `not a url`, `ftp://host`, `http://`, `https://host?x=1`,
  `https://host#f`, or `https://user:pass@host`
- **THEN** the response is `400` with `details.reason` `public_url_invalid`

#### Scenario: Rejecting a malformed base URL

- **WHEN** a client sends `base_url` as `not a url` or `https://user:pass@host`
- **THEN** the response is `400` with `details.reason` `base_url_invalid`

#### Scenario: Normalizing a stored URL

- **WHEN** a client sends `base_url` as `  http://homebox.lan:7745/  `
- **THEN** the stored and returned value is `http://homebox.lan:7745`

### Requirement: Generated links use the public URL

Every URL Labeler generates for a person to open SHALL be built from the connection's `public_url`
when it has one, and from `base_url` otherwise. This covers the `url` on a browsed row and the
`item_url` and `location_url` fields materialized into label data, which is what a printed QR code
encodes. A blank or whitespace-only stored `public_url` SHALL behave as absent. Requests Labeler
itself makes to the upstream system SHALL always use `base_url`, never `public_url`.

This requirement supersedes nothing in `docs/SPEC.md`, which does not specify how entity links are
built.

#### Scenario: Public URL set

- **WHEN** a connection has `base_url` `http://homebox:7745` and `public_url`
  `https://homebox.example.com`, and a client browses or materializes a row for entity `e1`
- **THEN** the row's `url` and any materialized `item_url` or `location_url` are
  `https://homebox.example.com/entity/e1`
- **AND** the upstream request Labeler made went to `http://homebox:7745`

#### Scenario: Public URL absent

- **WHEN** a connection has `base_url` `http://homebox:7745` and no `public_url`, and a client browses
  or materializes a row for entity `e1`
- **THEN** the row's `url` and any materialized `item_url` or `location_url` are
  `http://homebox:7745/entity/e1`

### Requirement: Connection management on the Connect page

Connection management SHALL live on the Connect page, in a collapsible block labelled **Manage
connections** that renders below the page's connection and template pickers and above the composer and
the browse table. The Settings page SHALL render no connections UI: no connections table, no connection
form, no default-connection control, and no link or redirect standing in for any of them.

The block SHALL start collapsed, and SHALL start expanded when the connections list has loaded and
holds no connection, which is the state in which the rest of the page has nothing to show. A list that
is still loading, or that failed to load, SHALL leave the block collapsed, because neither says the
installation has no connections. Whether the block is open is component state: the operator SHALL be
able to open and close it at any time, and the state SHALL NOT survive a reload.

The block SHALL let an operator add, edit, delete, enable and disable a connection without navigating
away. It SHALL list the connections in a table showing, for each, its name, connector, base URL, public
URL, whether an API key is set, and whether it is enabled, with `-` for a connection that has no public
URL.

The connection form SHALL carry **connector**, **name**, **base url**, an optional **public url**, an
**api key** and an **enabled** checkbox. The form for an existing connection SHALL additionally carry
the ordered field-transform rule editor that submits the connection's `transforms`; the form for a new
connection SHALL carry no rule editor and SHALL say that rules are added after saving. What that
editor offers, when it submits `transforms` and how a rule is previewed belong to
`connector-field-transforms`, which this requirement neither restates nor overrides.
The form SHALL be pre-filled from the stored connection when editing one,
and SHALL submit `public_url` on every save so that clearing the field clears the stored value. A
non-blank **base url** or **public url** SHALL be rejected in the form, without a request, when it is
not a parseable `http`/`https` URL.

The API key SHALL be write-only: the form SHALL present it as a password field, responses expose only
`has_credential`, and saving an edit with the field left blank SHALL keep the stored key. Creating a
connection with the field blank SHALL be rejected in the form, without a request.

This requirement supersedes the frozen `docs/SPEC.md` §12 ("Using a connection (UI)") as far as that
paragraph says where connections are added and edited and how the API key behaves, namely its first two
sentences. The rest of that paragraph is untouched here and is superseded elsewhere: the browse table by
`connector-browser`, and the opening of the Connect flow by `default-connection`.

#### Scenario: Adding a connection without leaving Connect

- **WHEN** the operator opens the Manage connections block on the Connect page, fills the form and saves
- **THEN** the connection is created and appears in the block's table, with no navigation away from
  the Connect page

#### Scenario: Settings has no connections UI

- **WHEN** the operator opens the Settings page
- **THEN** it renders no connections table, no connection form and no default-connection control, and
  requests no connection list for one

#### Scenario: The block starts collapsed when a connection exists

- **WHEN** the operator opens the Connect page and the connections list loads with at least one
  connection
- **THEN** the Manage connections block is collapsed, and opens when the operator opens it

#### Scenario: The block starts expanded when no connection exists

- **WHEN** the operator opens the Connect page and the connections list loads empty
- **THEN** the Manage connections block is expanded

#### Scenario: The connections list fails to load

- **WHEN** the operator opens the Connect page and the request for the connections list fails
- **THEN** the Manage connections block is collapsed

#### Scenario: Setting a public URL

- **WHEN** the operator edits a connection, types `https://homebox.example.com` into **public url**,
  and saves
- **THEN** the request body carries `public_url: "https://homebox.example.com"`
- **AND** the table row shows that public URL

#### Scenario: Clearing a public URL

- **WHEN** the operator edits a connection that has a public URL, empties the **public url** field,
  and saves
- **THEN** the request body carries `public_url: null`
- **AND** the table row shows `-`

#### Scenario: Rejecting an invalid public URL in the form

- **WHEN** the operator types `homebox.example.com` into **public url** and saves
- **THEN** the form shows a validation error and sends no request

#### Scenario: Leaving a public URL unset

- **WHEN** the operator adds a connection and leaves **public url** empty
- **THEN** the request body carries `public_url: null` and the connection is created

#### Scenario: Keeping the stored API key

- **WHEN** the operator edits a connection whose `has_credential` is `true`, leaves **api key** blank,
  and saves
- **THEN** the request body carries no credential and the stored key is kept

#### Scenario: Creating a connection with no API key

- **WHEN** the operator adds a connection and leaves **api key** empty
- **THEN** the form shows a validation error and sends no request

### Requirement: Saving or deleting a connection refreshes what the open Connect page holds

The Connect page holds two answers that a change to the connection can invalidate: the connector
schema it read for the selected connection, and the browse rows it fetched at some earlier moment.
Both SHALL follow the connection.

Saving a connection SHALL cause every consumer of that connection's schema to re-read it, so a saved
change is offered without a reload and without remounting the page. This SHALL hold for every save,
whichever control performed it and whatever the save changed: it is a property of saving, not of one
form remembering to ask. A read of that schema already in flight when the save succeeds SHALL
NOT become what the page holds: it was answered before the save, and the page SHALL end up with a
schema read after it.

Saving the connection currently being browsed SHALL re-browse the rows on screen, for the resource,
the connection filters and the drill-down parent in force at that moment. **The trigger is the save,
not what the save changed.** A save can point the connection at a different upstream, or replace the
credential it authenticates with, and the second of those is not visible in any response the page can
compare: `has_credential` says a key is set, never which one. Rows fetched from the previous upstream
SHALL NOT stay on screen under a schema read from the new one.

Neither refresh SHALL depend on the control that started it still being on screen. The operator may
collapse the Manage connections block, or close the form, while the request is still in flight; the
save completes either way, and what it re-reads and what it refreshes SHALL be what they would have
been had the operator waited.

A refresh SHALL be dropped rather than shown if a later request supersedes it: if the operator switches
resource, drills in, clears a parent or applies a filter while a refresh is in flight, the refresh's
rows SHALL never reach the table.

A refresh is not a change of browsing context. The active sort, the column filters, the visible column
set and the row selection SHALL survive it, except where the saved change removed the column they name,
which `connector-browser` governs. The refresh SHALL start from the first page; rows beyond it are
reloaded by the operator, their cells being as stale as the first page's were.

Deleting a connection SHALL leave no held schema for it and SHALL issue no request for it. That rests
on the page having retired the deleted connection's selection at the delete, which `default-connection`
requires, rather than on the connections list reporting the absence later.

An editor open on the deleted connection SHALL close at the delete on the same terms, whatever was
open when the delete started, and whether or not the Manage connections block was collapsed and
reopened while the request was in flight. An editor open on a different connection SHALL stay open: it
names a connection that still exists.

#### Scenario: A save that changes no rule still re-reads the schema

- **WHEN** the operator saves a connection having changed only its name
- **THEN** that connection's schema is requested again

#### Scenario: A save that points the connection at another upstream replaces the rows

- **WHEN** the operator saves the connection being browsed with a different base URL, changing no rule
- **THEN** the rows on screen are replaced by rows browsed from the new upstream, with no reload

#### Scenario: The Manage connections block is collapsed before the save completes

- **WHEN** the operator saves the connection being browsed and collapses the Manage connections block
  before the request completes
- **THEN** the connection's schema is re-read and the rows on screen are refreshed

#### Scenario: Saving another connection does not disturb the browse table

- **WHEN** the operator saves a connection other than the one being browsed
- **THEN** no browse request is made

#### Scenario: A refresh superseded by a resource switch is dropped

- **WHEN** the operator switches to another resource while a save's refresh is still in flight
- **THEN** the refresh's rows are discarded, and the table shows the newly selected resource's rows
  alone

#### Scenario: The browsing context survives the refresh

- **WHEN** a save refreshes the rows while a sort, a column filter and a row selection are in force,
  and the saved change removes no column they name
- **THEN** the sort, the column filter, the visible columns and the selection are all still in force
  afterwards

#### Scenario: A save while the first schema read is still in flight

- **WHEN** the operator saves the connection being browsed while the first read of its schema has not
  yet answered
- **THEN** a fresh schema read is made
- **AND** the earlier read answering afterwards does not become the page's schema

#### Scenario: A deleted connection's schema is not kept

- **WHEN** the operator deletes a connection whose schema the page had read
- **THEN** no schema for that connection is still held, and no schema request is made for the deleted
  id

#### Scenario: An editor opened after the delete starts still closes

- **WHEN** the operator confirms deleting a connection with no editor open, opens the editor for that
  same connection before the request completes, and the connections list refetch is delayed or fails
- **THEN** that editor closes when the delete succeeds
- **AND** re-rendering makes no request for the deleted connection's schema and leaves none held

#### Scenario: The block is collapsed and reopened before the delete completes

- **WHEN** the operator confirms deleting a connection, collapses the Manage connections block,
  reopens it and opens an editor before the request completes, with the connections list refetch
  delayed or failed
- **THEN** an editor open on the deleted connection closes, and one open on another connection stays
  open
- **AND** re-rendering makes no request for the deleted connection's schema and leaves none held
