# 69. Connect opens on a default connection named by an instance-wide setting

Date: 2026-08-23

## Status

Accepted. Implements [#203](https://github.com/pfa230/labeler/issues/203). Builds on
[ADR-0024](0024-app-settings-storage-and-api.md), which stays Accepted.

## Context

The Connect page opens on an empty picker. An operator must choose a connection manually on every
visit before templates, field mapping, or browse rows appear. Because most installations configure
only one connection, this manual selection carries no information and adds unnecessary friction on
every visit.

Selecting a connection immediately mounts `ConnectorBrowser`, which issues an upstream request to
load the first page of rows. Opening directly on a connection therefore issues an upstream browse
request on page load.

Furthermore, connection names are not unique, and deleting a connection that is currently configured
as the default could leave a dangling stored setting if the delete and setting clear are not
synchronized. Finally, if the effective connection changes while an operator is browsing, selected
rows from one connection could leak into another unless selection changes are controlled.

## Decision

**1. Instance-wide setting `default_connection_id`.** An application setting `default_connection_id`
is added to `app_settings` (ADR-0024). It defaults to "none" (`null` / `is_default: true`) and can be
set or cleared from Settings > Connections. Dedicated per-user preference stores were evaluated and
deferred (e.g. #208 for templates) in favor of the shared instance setting.

**2. Deterministic resolution order with total ordering fallback.** When the Connect page loads, it
resolves the initial connection by checking:
1. The connection named by `default_connection_id`, if it exists and is enabled;
2. Otherwise, the first enabled connection from `GET /api/connections`, which orders by `name, id` to
   guarantee a total, deterministic ordering across visits;
3. Otherwise, nothing (empty picker) if no connection is enabled.
A failure to load settings falls back to the first enabled connection; a failure to load connections
resolves nothing.

**3. Latched opening resolution.** The initial connection resolution on the Connect page is latched
once both queries settle. Subsequent refetches (such as window focus refreshes or background setting
changes by other users) do not alter the operator's active connection. The active connection changes
only when the operator explicitly uses the connection picker, which resets the row selection, browse
table, and composer together.

**4. Atomic delete cascade.** Deleting a connection via `DELETE /api/connections/{id}` clears
`default_connection_id` in the same database transaction whenever the deleted connection is the one
named by the setting. This ensures no reader ever observes a dangling default reference.

**5. Accepted upstream request on open.** Automatically resolving a connection on open mounts
`ConnectorBrowser` and immediately issues a request to the upstream connector without requiring a click.
This trade-off is accepted to make the Connect page immediately ready for work.

## Consequences

- Navigating to the Connect page triggers an immediate upstream browse request when an enabled connection exists.
- An admin can configure or clear the default connection from Settings > Connections, where disabled and dangling connections are explicitly distinguished.
- Deleting a connection atomically cleans up the default connection setting without leaving stale references.
- `GET /api/connections` guarantees a total order (`ORDER BY name, id`), ensuring stable fallback selection.
