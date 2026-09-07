## ADDED Requirements

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
**api key**, an **enabled** checkbox, and the ordered field-transform rule editor that submits the
connection's `transforms`. The form SHALL be pre-filled from the stored connection when editing one,
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

## REMOVED Requirements

### Requirement: Connections settings UI

**Reason**: The Settings page no longer has a Connections section, so a requirement naming
"Settings > Connections" describes a place that does not exist. Its whole contract, the form's fields,
the public-URL validation and the table's public-URL column, is restated at its new home by the ADDED
requirement "Connection management on the Connect page", which also carries the `docs/SPEC.md` §12
supersession this one made.

**Migration**: None. Pre-1.0, a change that alters behavior breaks what came before: the connections UI
is on `/connect` and nothing stands in for it on `/settings`. No API, request body, response body or
stored value changes, so nothing an operator stored needs migrating.
