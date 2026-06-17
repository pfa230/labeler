# 18. API integration spine (connectors)

**Status:** Accepted

## Context

Labeler renders labels from data the user types or imports as CSV. The M7 milestone adds a way to pull
that data straight from an external system of record (first Homebox, an inventory manager) so a user can
browse their items and materialize them into label rows without re-keying. That requires outbound HTTP
from the service, a uniform way to describe and page through a third party's data, and a place to store
per-connection credentials. This ADR records the design of that integration spine: egress, the connector
framework, browse cursors, the connections store, and the Homebox connector.

## Decision

- **Hardened egress.** All outbound HTTP goes through one shared `reqwest` client (rustls). Before each
  request the target IP is allow-checked: loopback, link-local, unspecified, and multicast are blocked,
  but private LAN ranges are allowed (the target Homebox commonly lives on the same LAN). The client sets
  connect and read timeouts, caps the response body by streaming with an 8 MiB ceiling, disables
  redirects, ignores proxy environment variables, and redacts bearer tokens from any error or log output.
  **Residual risk:** the IP check resolves DNS and then reqwest re-resolves on connect, leaving a
  resolve-then-connect TOCTOU window for DNS rebinding. This is accepted for a single-tenant, authed,
  LAN-facing tool. The future tightening, if needed, is a custom `reqwest::dns::Resolve` resolver that
  pins the checked address.

- **Connector framework.** A connector exposes a `Connector`-shaped surface (schema, browse,
  materialize). Because there is exactly one connector today, dispatch goes through an enum registry
  (`Connectors` / `ConnectorRegistry`) rather than `dyn` plus an async-trait, avoiding boxing and
  async-trait machinery for a single variant. The browse model is `schema` (resources, tiered fields,
  filters, relationships) then `browse` (paged display rows) then `materialize` (selected rows to label
  data). Fields carry a tier: `Cheap` (free from the list call), `Hydrated` (needs a per-row fetch),
  `Derived` (computed).

- **Browse cursors.** Pagination cursors are opaque to the client, HMAC-SHA256 signed, and bound to the
  tuple {connector, connection, resource, filter_hash, page}. The signing key is generated per process
  lifetime, so cursors do not survive a restart; the UI simply re-browses from the first page. This keeps
  cursors unforgeable and self-describing without a server-side cursor table.

- **Connections store.** Connections live in a `connections` table (connector, name, base_url,
  credential, enabled). The credential is a pasted Homebox API key, stored as-is for now (at-rest
  encryption deferred, consistent with ADR-0017). The credential is **never** returned by the API:
  responses expose only `has_credential` so the UI can show whether one is set.

- **Homebox connector.** Talks to Homebox's unified `/v1/entities` API, verified against the Homebox
  swagger: list via `q` / `page` / `pageSize` / `tags` / `parentIds` as repeated bare-key query params
  returning an `{ items, total }` envelope, tree via `/v1/entities/tree`, single entity via
  `/v1/entities/{id}`, and field discovery via `/v1/entities/fields` (a string array). Auth is
  `Authorization: Bearer <key>` with the key pasted verbatim, since a Homebox key already carries its
  `hb_` prefix.

## Consequences

- The service now makes outbound requests; deployments must allow egress to the configured Homebox host.
  The egress allow-check intentionally permits private ranges, so it does not protect against a malicious
  operator-supplied `base_url` on the LAN. That is acceptable under single-tenant, authed operation.
- New API surface: `GET/POST /api/connections`, `GET/PUT/DELETE /api/connections/{id}`, and
  `GET /api/connections/{id}/schema`, `POST /api/connections/{id}/browse`,
  `POST /api/connections/{id}/materialize`. Upstream failures map to `502`; bad filters and budget
  overruns map to `400`.
- Cursors are not durable across restarts by design; clients must treat them as ephemeral and re-browse.
- **Deferred:** credential encryption at rest, additional connectors, a `dyn` connector surface (revisit
  once a second connector exists), and a rebind-proof pinning resolver. Any of these can supersede this
  ADR when revisited.
