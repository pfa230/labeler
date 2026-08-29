## MODIFIED Requirements

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
