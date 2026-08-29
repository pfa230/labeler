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

### Requirement: Connections settings UI

Settings > Connections SHALL let an operator set and clear a connection's public URL. The connection
form SHALL carry an optional **public url** field alongside **base url**, pre-filled from the stored
value, and SHALL submit `public_url` on every save so that clearing the field clears the stored value.
A non-blank entry SHALL be rejected in the form, without a request, when it is not a parseable
`http`/`https` URL. The connections table SHALL show each connection's public URL, and `-` when it has
none.

This requirement supersedes the "Using a connection (UI)" paragraph of the frozen `docs/SPEC.md` §12
as far as it describes the connection form's fields. The rest of that paragraph, covering the Connect
page, remains authoritative.

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
