# Homebox Integration — design

M7 sub-project 2 (issue #35). Concretizes the approved, 3x-codex-reviewed **API integration framework**
(`docs/superpowers/specs/2026-06-16-api-integration-framework-design.md`) for the first connector,
Homebox, now that app authentication has shipped (ADR-0017). The framework design is the source of truth
for the `Connector` trait, the normalized browse model, and the `schema`/`browse`/`materialize` split;
this spec records the Homebox-scoped decisions and the resolutions to the framework's "open items". It
drops the framework's interim "LAN-trust" caveat (the app is now authenticated).

## Decisions settled in brainstorming
1. **Egress is lightly hardened, not paranoid.** The framework's "block private IP ranges by default +
   opt-in" was over-engineered for this app (a single-tenant, self-hosted LAN tool behind app auth, whose
   connector calls only Homebox's known endpoints, no arbitrary-URL primitive). Reaching the LAN is the
   whole point. So: keep the cheap, high-value protections; allow private LAN; block only link-local.
2. **Homebox auth = pasted API key.** Verified against the live swagger: Homebox supports static API keys
   (shipped in v0.26; `hb_`-prefixed, created at `POST /v1/users/self/api-keys`, listed/revoked at
   `GET`/`DELETE /v1/users/self/api-keys/{id}`, optional `expiresAt` so it can be long-lived, takes the
   creating user's access level). The user generates a key in Homebox and pastes it; the connector sends
   it as `Authorization: Bearer hb_...`. No stored username/password, no login/refresh/expiry handling, and
   the key is independently revocable in Homebox (lower blast radius than storing the master credential).
   The connection credential is the API key.
3. **Unified entity model.** Verified: Homebox merged items and locations into one `/v1/entities` resource
   (the April-2026 "entity merge"), discriminated by an `entityType` field. Both item-type and
   location-type entities are labelable, each with a derived back-link URL.
4. **Thumbnails omitted in v1** (would need a token-safe media proxy; deferred).
5. **Expansion = `AsListed` only** (one selected entity -> one grid row; copies via the grid's multiplier).
6. **Decomposition:** two implementation plans, (A) backend spine + Homebox connector, (B) frontend.

## 1. Hardened egress client (one shared client for all connectors)
A single outbound HTTP client (`reqwest` or hyper) used by every connector, configured once:
- `http`/`https` only; a sane port (default 80/443/the explicit port in `base_url`); ignore proxy env vars.
- Connect + read timeouts; a max response-body byte cap; a decompression bound; a JSON content-type check.
- **Do not follow cross-host redirects** (a `3xx` to a different host is rejected, not followed, so a
  redirect cannot bounce the credential to another host).
- **IP check:** resolve the `base_url` host, and **deny** link-local `169.254.0.0/16` (covers the
  cloud-metadata endpoint), the unspecified `0.0.0.0`/`::`, and **loopback** `127.0.0.0/8`/`::1` (so the
  connector cannot be used to probe localhost-only services on the labeler's own host; a same-host Homebox
  is reached via its LAN IP or a Docker network alias, not the labeler's loopback). Private LAN ranges
  (`10/8`, `172.16/12`, `192.168/16`) are **allowed** (that is where Homebox lives). Pin the connection to
  the vetted IP to avoid DNS-rebinding between check and connect.
- **Redact** `Authorization`, cookies, the stored credential, and cursor payloads in all logs.
- Connectors only ever pass their own constructed requests to this client; no client-supplied URL or path
  reaches it. The `base_url` is the only user-controlled input.

## 2. Connections store + CRUD
A `connections` table mirroring the printer-store pattern: `{ id, connector, name, base_url, credential,
enabled, created_at }`. For Homebox the `credential` is the pasted API key (`hb_...`). It is
**write-only / redacted**: never returned by reads (the API returns the connection without the credential,
plus a `has_credential` bool), updatable, deletable. Encryption-at-rest is a later hardening; redaction and
lifecycle are in scope now. CRUD endpoints `/api/connections` (list/create) and `/api/connections/{id}`
(get/update/delete) require app auth (flat: any authenticated user; no admin tier). Because the credential
is a static API key used directly as a bearer header, there is no in-memory token cache or refresh logic.

## 3. `Connector` trait + registry
Exactly the framework trait (`schema` / `browse` / `materialize`, connection-aware async, server-issued
bound cursors, `RowRef` identity, structured relationships, `ExpansionPolicy`, the stable `ConnectorError`
set). A registry keyed by connector id (`"homebox"`), like `PrinterDriver`. This sub-project ships the
trait + one impl; InvenTree and the declarative DSL stay deferred behind the seam.

## 4. Homebox connector (unified `/v1/entities` model)
- **Auth:** send the stored API key as `Authorization: Bearer hb_...` on every request. No login, refresh,
  or token cache. `auth_kinds` advertise `StaticToken` (paste). A `401` surfaces as `ConnectorError::AuthFailed`.
- **Resources & schema** (one upstream resource, split into two connector resources by `entityType`):
  - `items` (`View::Table`): `FieldSpec`s `name`, `description`, `quantity`, `location` (the `parent`
    name), `assetId`, `serialNumber`, `modelNumber`, `manufacturer`, `purchasePrice`, `tags`
    (comma-joined label names), `id`, and a derived `item_url` = `{base_url}/entity/{id}` (confirm the
    web route; post-merge entities share a route). Filters: text `q`, `parent` (a location id from the
    tree / a picker, NOT a free-text name), `tag` (label id), `archived` (bool).
  - `locations` (`View::Tree`, also labelable): `FieldSpec`s `name`, `description`, `parent` (name),
    `itemCount`, `id`, and a derived `location_url` = `{base_url}/entity/{id}`.
  - The `location -> contained items` relationship drills via the `parentIds` filter.
- **browse:**
  - items: `GET {base_url}/api/v1/entities?q=&page=&pageSize=&tags=<ids>&parentIds=<locId>` and keep only
    `entityType == item` rows (the list mixes types; if a server-side type filter is absent it is applied
    in the connector before returning the page. confirm whether a type param exists at implementation).
    Response rows come from `repo.EntitySummary` (cheap display fields).
  - locations: `GET {base_url}/api/v1/entities/tree?withItems=false` (the tree nodes are the locations).
  - `next_cursor` is a server-issued bound token carrying `{connector, connection, resource, filter_hash,
    page, page_size}`; an upstream URL is never used as a cursor.
- **materialize:** for the selected `RowRef`s and mapped `FieldKey`s, fetch `GET {base_url}/api/v1/entities/{id}`
  (`repo.EntityOut` carries all detail fields: `manufacturer`, `modelNumber`, `serialNumber`,
  `purchasePrice`, custom `fields`, etc.) within a fanout/cache budget, build the derived URL, and return
  one `LabelRow` per selected entity (`AsListed`).
- **Identity:** Homebox entity ids (uuid) are the `RowRef.key`.
- **Verified API facts** (from the live swagger, `sysadminsmedia/homebox`): `GET /v1/entities` params are
  `q`, `page`, `pageSize`, `tags[]` (label ids), `parentIds[]` (location ids); `GET /v1/entities/tree`
  (`withItems`); detail `GET /v1/entities/{id}`; `entityType` discriminates item vs location; API keys at
  `/v1/users/self/api-keys`. (Confirm the web entity-URL path at implementation.)

## 5. Browse endpoints (backend)
- `GET  /api/connections/{id}/schema` -> `ConnectorSchema` (cached, TTL + a version key; a connector or
  instance change must not silently reinterpret a saved mapping, so the cache key includes a schema
  version and saved mappings pin it).
- `POST /api/connections/{id}/browse` `{ resource, filters?, parent?, cursor?, page_size? }` -> `BrowsePage`.
- `POST /api/connections/{id}/materialize` `{ rows, fields, expansion }` -> `[LabelRow]`.
All require app auth. `ConnectorError` maps to stable HTTP codes (auth -> 502/"AuthFailed" upstream,
invalid filter -> 400, etc.).

## 6. Field mapping (separate from rendering)
Per `(connection, template)`: each template field <- a source `FieldKey` chosen from the connector's typed
`FieldSpec` list, distinct from template `{field}` / `{settings.*}` interpolation. The chosen keys drive
`materialize` (only those are hydrated). The resulting `LabelRow`s become `origin:"connector"` rows in the
**existing `LabelGrid`** (review/edit/copies), carrying `source = { connector, connection, resource, key }`,
then post to `/batch`. Mapping config is stored per `(connection, template)`; the UI flags missing/renamed
template fields and stale source keys (drift).

## 7. Frontend
- A **Connections** management screen (list/add/edit/delete; the add form picks connector = Homebox, sets
  name + base_url + the pasted Homebox API key `hb_...`; credential write-only, shown only as "set").
  The form links to where Homebox generates a key (Profile -> API keys).
- A generic **browse** view driven by `schema()` + `browse()`: renders items as a grid and locations as a
  tree (per `View`), shows the curated filter widgets, drills location -> items via the relationship,
  paginates via `next_cursor`/`has_more`, and multi-selects.
- "Use for labels": pick the template, map fields (the precise picker over `FieldSpec`s), choose
  `AsListed`, call `materialize`, and load the rows into the `LabelGrid` -> `/batch`.

## 8. Security
- App auth gates all connection CRUD and browse endpoints (flat; any authenticated user).
- Egress hardening as in section 1 (link-local blocked; private LAN allowed; no cross-host redirects;
  timeouts/size caps; secret redaction).
- The connector is **read-only** against Homebox: only `GET` requests; it never mutates inventory.
- The stored credential is a Homebox **API key**, not the account password: it can be scoped/revoked
  independently in Homebox, so a leaked labeler DB exposes a revocable key rather than the master
  credential (the reason API-key auth is preferred over the login flow). The key is redacted in all reads
  and logs; at-rest encryption is deferred (consistent with the rest of the store) and is the recommended
  next hardening for credential columns.

## 9. Testing
- **Egress client:** cross-host redirect rejected; link-local/metadata IP blocked; private LAN allowed;
  oversized response capped; timeout enforced; secrets redacted in logs.
- **Homebox connector** (mocked Homebox API, realistically-shaped fixtures): the `Authorization: Bearer
  hb_...` header is sent; a `401` maps to `AuthFailed`; `entities` browse paging with `entityType`
  filtering to items; the `entities/tree` location tree + a location->items drill-down via `parentIds`;
  `materialize` hydration of the mapped fields incl. the derived URLs; identity. One positive + one
  negative (auth failure, empty page) each.
- **Browse endpoints** against a fake connector: schema shape + version pinning; browse cursor bind/reject;
  error-category -> HTTP mapping; `AsListed`.
- **Mapping -> materialize -> grid -> /batch** round-trip with a couple of Homebox records.
- **Frontend:** connections CRUD; browse grid + location-tree render from a stubbed schema/browse; mapping
  picker; selection -> materialize -> grid.

## 10. Scope
- **In:** the hardened egress client, the `connections` store + CRUD, the `Connector` trait + registry,
  the Homebox connector (API-key auth, the unified `/v1/entities` model browsed as items + locations,
  browse/materialize/schema, derived URLs), the browse endpoints, field mapping, the generic browse UI +
  connections screen, `AsListed` expansion.
- **Out / deferred (behind the trait seam):** InvenTree + other connectors, the declarative connector DSL,
  user-authored connector types, thumbnails / media proxy, `BySerialOrQuantity` expansion, OAuth/OIDC
  Homebox auth, write-back to Homebox, connection-credential at-rest encryption, per-connection egress
  policy.

## 11. Decomposition (two implementation plans)
- **Plan A (backend):** egress client; connections store + CRUD; `Connector` trait + registry; Homebox
  connector; `/api/connections/{id}/schema|browse|materialize`. Verifiable via mocked-Homebox + browse
  endpoint tests. Self-contained working software (the API can be driven by curl/tests without the UI).
- **Plan B (frontend):** Connections screen; generic browse/drill-down UI; field-mapping -> `LabelGrid`
  -> `/batch`. Builds on Plan A.

## Open items resolved
- **Thumbnails:** omitted in v1 (no media proxy).
- **Schema caching:** TTL + version key; saved mappings pin the schema version so a connector/instance
  upgrade does not silently change a mapping's meaning.
- **Filter set:** Homebox items expose `search`, `location`, `label`, `archived` (a curated subset, not a
  pass-through of every upstream filter).
