# default-connection Specification

## Purpose
Defines the connection the Connect page opens on: the instance-wide setting that names it, the
deterministic fallback used when no setting is stored, what the page does once a connection is
resolved, and how an operator sets and clears the default.

## Requirements

### Requirement: The default connection setting

`default_connection_id` SHALL be a known application setting holding the id of the connection the
Connect page opens on. Its in-code default SHALL be "none": with no override stored,
`GET /api/settings` SHALL report the key with value `null` and `is_default: true`, alongside every
other known setting.

`PUT /api/settings/default_connection_id` SHALL take the request body every settings write takes,
`{ "value": <json> }`. Its `value` SHALL be a JSON string. Surrounding whitespace SHALL be trimmed
before storage, and the trimmed id SHALL be the value the response reflects back. A `value` that is
not a JSON string, is empty or whitespace-only after trimming, or names no existing connection SHALL
be rejected with `400` and `details.reason` `setting_value_invalid`. An id naming a connection that
exists but is **disabled** SHALL be accepted: disabling a connection is temporary and SHALL NOT cost
the operator their stored choice.

`DELETE /api/settings/default_connection_id` SHALL clear the override and return `204`, as it does for
every other setting, after which the key reads back as `null` with `is_default: true`.

Stored text SHALL be treated the way every other setting's stored override is treated (ADR-0024):
text that is empty or whitespace-only is **corrupt**, and reading settings SHALL surface an error
rather than silently substituting a default. A well-formed id that names no connection is **not**
corrupt: it is the dangling case this capability deliberately supports, and `GET /api/settings` SHALL
report it as stored.

This requirement extends the known-settings list of the frozen `docs/SPEC.md` §12 ("Settings") with one
key. Every other part of that section, including the endpoint contract and the corrupt-override
behavior it states, remains authoritative and is unchanged.

#### Scenario: No default is stored

- **WHEN** a client reads `GET /api/settings` and no override has been written
- **THEN** the response contains `default_connection_id` with value `null` and `is_default: true`

#### Scenario: Setting the default to an existing connection

- **WHEN** a client sends `PUT /api/settings/default_connection_id` with body
  `{ "value": "  conn-1  " }` and `conn-1` is an existing connection
- **THEN** the response is `200` with value `conn-1` and `is_default: false`
- **AND** `GET /api/settings` reports `conn-1`

#### Scenario: Setting the default to a disabled connection

- **WHEN** a client sets `value` to the id of a connection whose `enabled` is `false`
- **THEN** the response is `200` and the id is stored

#### Scenario: Rejecting an id that names no connection

- **WHEN** a client sets `value` to an id no connection has
- **THEN** the response is `400` with `details.reason` `setting_value_invalid`

#### Scenario: Rejecting a value that is not a usable id

- **WHEN** a client sends `value` as `""`, `"   "`, `null`, a number, an array, or an object
- **THEN** the response is `400` with `details.reason` `setting_value_invalid`

#### Scenario: Reading a stored id whose connection was never created

- **WHEN** `default_connection_id` holds a well-formed id that names no connection
- **THEN** `GET /api/settings` reports that id with `is_default: false`, and does not error

#### Scenario: Clearing the default

- **WHEN** a client sends `DELETE /api/settings/default_connection_id`
- **THEN** the response is `204` and `GET /api/settings` reports the key as `null` with
  `is_default: true`

### Requirement: Connect opens on a resolved connection

The Connect page SHALL resolve a connection to open on, rather than starting with none selected. It
SHALL resolve, in order:

1. the connection named by `default_connection_id`, when that connection exists and is enabled;
2. otherwise the first enabled connection in the order `GET /api/connections` returns, which is a
   total order (see the `connections` capability), so the same installation resolves the same
   connection on every visit;
3. otherwise nothing, when no connection is enabled.

**The resolution is latched.** The page SHALL resolve once, the first time both the connection list
and the settings are available, and SHALL NOT re-resolve afterwards. A later refetch of either, by a
window-focus refresh or by another operator changing the instance-wide setting, SHALL NOT change what
is selected underneath the operator. After the initial resolution, the selection SHALL change only
when the operator changes the picker.

Whenever the selected connection changes, every piece of connection-scoped state SHALL reset together:
the row selection, the browse table's rows and browsing context, and the composer. No row selected
against one connection SHALL ever be presented or materialized against another.

A resolved connection SHALL be selected in the connection picker as though the operator had chosen it,
which SHALL load its schema and the browse table's first page of rows without any click. Opening the
Connect page therefore issues a request to the upstream system the resolved connection points at.

When nothing resolves, the page SHALL present exactly what it presents today: the connection picker,
and none of the sections below it.

When the settings read fails, the page SHALL resolve as though no default were stored, using the
fallback, rather than waiting indefinitely or leaving the operator on an empty page. A failure to load
the connection list resolves nothing, because there is then nothing to select.

Changing the picker by hand SHALL keep working unchanged, including clearing the row selection, and
SHALL NOT write `default_connection_id`: the default is set from Settings, never as a side effect of
working on the Connect page.

This requirement supersedes one clause of the frozen `docs/SPEC.md` §12 ("Using a connection (UI)"):
the statement that the Connect page's flow begins by picking a connection. The rest of that paragraph
is untouched here, and parts of it are already superseded elsewhere: the browse table by
`connector-browser`, and the connection form by the `connections` capability's "Connections settings
UI" requirement.

#### Scenario: A stored default resolves

- **WHEN** `default_connection_id` names an enabled connection and an operator opens Connect
- **THEN** that connection is selected in the picker
- **AND** its schema is loaded and the browse table has requested its first page of rows

#### Scenario: No default stored, several connections enabled

- **WHEN** no `default_connection_id` is stored and an operator opens Connect
- **THEN** the first enabled connection in the listed order is selected

#### Scenario: The stored default is disabled

- **WHEN** `default_connection_id` names a connection whose `enabled` is `false`, and another
  connection is enabled
- **THEN** the first enabled connection is selected instead

#### Scenario: The stored default names no connection

- **WHEN** `default_connection_id` names an id no connection has, and a connection is enabled
- **THEN** the first enabled connection is selected instead

#### Scenario: Only disabled connections exist

- **WHEN** no connection is enabled and an operator opens Connect
- **THEN** no connection is selected, the picker shows its "choose a connection" state, and no
  template picker, composer or browse table is shown

#### Scenario: The settings read fails

- **WHEN** `GET /api/settings` fails and an enabled connection exists
- **THEN** the first enabled connection is selected, exactly as when no default is stored

#### Scenario: A later refetch does not move the operator

- **WHEN** an operator is working on a resolved connection and the connections list or the settings
  are refetched, with the stored default now naming a different connection
- **THEN** the selected connection, the browse table and the row selection are unchanged

#### Scenario: Changing the picker by hand

- **WHEN** an operator opens Connect on a resolved connection, selects rows, and then picks a
  different connection
- **THEN** the new connection's schema and rows load, the row selection is empty, the composer is
  reset, and `default_connection_id` is unchanged

### Requirement: Settings names the default connection

Settings > Connections SHALL let an operator choose which connection is the default and clear that
choice, without typing an id. The control SHALL offer the existing connections by name, SHALL show
which one is currently stored, and SHALL offer a "no default" choice that clears the setting. The
control SHALL state that the default applies to everyone, because `default_connection_id` is
instance-wide.

Because connection names are not unique, an entry SHALL carry enough to tell two identically named
connections apart. A connection that is disabled SHALL be marked as such, so an admin choosing one is
not surprised when Connect falls through to the fallback.

When the stored id names no connection, the control SHALL show an explicit unavailable state naming
the stored id, rather than showing "no default" or an arbitrary connection, and that state SHALL still
be clearable.

After a connection is deleted, the control SHALL show the resulting state without a page reload:
deleting the connection that was the default SHALL leave the control showing no default.

The control is an addition to that settings section. It SHALL NOT change the connection form or
the connections table, whose contract stays the one `connections` states.

This requirement supersedes nothing in `docs/SPEC.md`, which does not describe a default connection.

#### Scenario: Choosing a default

- **WHEN** the operator picks a connection in the default-connection control
- **THEN** `PUT /api/settings/default_connection_id` is sent with that connection's id
- **AND** the control shows that connection as the stored default

#### Scenario: Clearing the default

- **WHEN** the operator picks the "no default" choice
- **THEN** `DELETE /api/settings/default_connection_id` is sent
- **AND** the control shows no stored default

#### Scenario: No default stored

- **WHEN** the operator opens Settings > Connections with no default stored
- **THEN** the control shows the "no default" choice as selected

#### Scenario: Two connections share a name

- **WHEN** two connections are both named `Homebox`
- **THEN** the control presents them distinguishably, so the operator can tell which one they picked

#### Scenario: A disabled connection in the control

- **WHEN** a connection whose `enabled` is `false` appears in the control
- **THEN** it is marked as disabled

#### Scenario: The stored default names no connection

- **WHEN** the operator opens Settings > Connections and `default_connection_id` holds an id no
  connection has
- **THEN** the control shows an unavailable state naming that id, and the operator can still clear it

#### Scenario: Deleting the default connection

- **WHEN** the operator deletes the connection that is the stored default
- **THEN** the control shows no default without the operator reloading the page
