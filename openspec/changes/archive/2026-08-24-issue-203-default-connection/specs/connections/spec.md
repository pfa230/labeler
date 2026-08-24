## MODIFIED Requirements

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
