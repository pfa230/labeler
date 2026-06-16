# API Integration Framework (conceptual design)

Status: draft (2026-06-16). Phase 2. Conceptual spec only, no implementation now.
Relates to: ADR-0011 (`/batch`), ADR-0008 (UI delivery), the M5 web UI design. Supersedes two earlier
drafts (declarative-connector and frontend-module-proxy), both rejected, see "Approaches rejected".

## Goal

Let external systems act as label **data sources**: browse an inventory (Homebox, InvenTree, and future
self-hosted REST APIs), select records, map their fields to a template, and print/download via the
existing `/batch`. The first targets are Homebox and InvenTree; the framework must make a third connector
a contained addition, not a rewrite.

## Decision in one line

A connector is **backend code** implementing a `Connector` trait, registered like printer drivers
(ADR-0007). Each connector translates one external API into a **normalized browse model**; a single
generic frontend renders that model. No declarative DSL, no per-connector frontend code, no generic
credentialed proxy.

### Why (vs the rejected approaches)

- **vs a declarative connector DSL:** an adversarial review against the real Homebox and InvenTree APIs
  showed "fully declarative, no code" is a false promise (varied pagination: page/size vs limit/offset vs
  cursor; auth `Bearer` vs `Token` vs Basic vs login flows; path-param and tree relationships; join
  hydration; quantity/serial expansion). A DSL covering all that becomes a programming language, yet you
  still author per-API definitions needing the same expertise as code. The "no rebuild" requirement it was
  chasing is about adding **connections** (instances), not authoring **connector types**, and concrete
  connectors + runtime connection config already satisfy that.
- **vs frontend modules + a generic credentialed proxy:** a browser-driven proxy that forwards
  client-built requests with a server-held token is an SSRF/abuse primitive and cannot survive real auth
  flows. Moving connectors server-side removes that whole surface.

The declarative DSL is **deferred**, revisit only if users demand authoring connector types without code,
and only after 2-3 real `Connector` impls reveal the genuine common shape. The trait is the seam that
keeps that option open.

## Architecture

```
Browser (generic browse UI)
  │  GET  /api/connections/{id}/schema     -> resource types, columns, typed filters, relationships
  │  POST /api/connections/{id}/browse     -> { rows:[{id, fields}], next_cursor }
  ▼
axum handlers ──► Connector trait impl (Homebox | InvenTree | …)
                    • owns auth, pagination, filter→query, relationships, hydration, identity
                    • calls the external API server-to-server (holds the secret; no browser CORS)
Connections store (SQLite): { id, connector, base_url, credential(write-only), enabled }
Selected+mapped rows ──► existing POST /batch  (render/print)
```

The backend owns the messy per-API translation (where code belongs); the frontend stays connector-agnostic
because every connector emits the same normalized model.

## Components

### 1. `Connector` trait (backend)
A registry of built-in connectors, keyed by id (`homebox`, `inventree`), like `PrinterDriver`. The contract
separates **browse** (fast, display-only) from **materialize** (deliberate hydration for the selected rows),
takes a **connection-aware async schema**, and uses structured relationships and server-issued cursors.

```
trait Connector {
    fn id(&self) -> &str;
    fn auth_kinds(&self) -> &[AuthKind];                 // StaticToken (paste) | Basic | Login{…}; OIDC later

    // Connection-aware + async: an instance's version/config changes what it offers
    // (e.g. Homebox custom fields, InvenTree detail defaults). Cache with version/TTL.
    async fn schema(&self, conn: &Connection) -> Result<ConnectorSchema, ConnectorError>;

    // Fast, bounded display rows for the grid. Owns auth, pagination strategy, filter translation,
    // and relationship traversal. next_cursor is a SERVER-ISSUED opaque token, never an upstream URL.
    async fn browse(&self, conn: &Connection, req: BrowseRequest) -> Result<BrowsePage, ConnectorError>;

    // Fetch the exact fields the selected rows need for mapping/printing (deliberate hydration with a
    // fanout/cache budget), applying the expansion policy. These rows become labels.
    async fn materialize(&self, conn: &Connection, req: MaterializeRequest) -> Result<Vec<LabelRow>, ConnectorError>;
}

struct BrowseRequest    { resource: String, filters: Map<FilterKey, FilterValue>, parent: Option<BrowseParent>,
                          cursor: Option<String>, page_size: Option<u32> }
struct BrowseParent     { relationship: RelationshipId, row: RowRef, mode: Direct | Recursive | TreeChildren }
struct BrowsePage       { rows: Vec<DisplayRow>, next_cursor: Option<String>, count: Option<u64>, has_more: bool }
struct DisplayRow       { id: RowRef, cells: Map<FieldKey, DisplayValue> }     // cheap, display only
struct RowRef           { resource: String, key: String }                      // canonical identity (e.g. InvenTree pk)

struct MaterializeRequest { rows: Vec<RowRef>, fields: Vec<FieldKey>, expansion: ExpansionPolicy }
struct LabelRow           { source: RowRef, data: Map<FieldKey, Value> }        // hydrated; one per label after expansion
enum   ExpansionPolicy    { AsListed, BySerialOrQuantity { cap: u32 } }

// schema() returns typed metadata so the mapping UI is precise, not a guess:
struct ConnectorSchema  { resources: Vec<ResourceSpec>, relationships: Vec<RelationshipSpec> }
struct ResourceSpec     { id, label, view: View, columns: Vec<FieldSpec>, filters: Vec<FilterSpec> }
struct FieldSpec        { key, label, ty: FieldType, nullable: bool, tier: Cheap | Hydrated | Derived, example }
enum   View             { Table, Tree }                                          // small, closed affordance set
// column/field display hints are a closed enum too: Text | Number | Money | Date | Status | Badge | Thumbnail
```

- **`schema` is async and per-connection** (not a compiled constant): a Homebox/InvenTree instance's
  version and custom fields determine what it exposes. Results are cached with a TTL + version key.
- **`browse` vs `materialize`** keeps the grid fast and bounded while making hydration (InvenTree stock →
  part/supplier/location detail) an explicit, budgeted step on just the selected rows, avoiding N+1 and
  under-hydration.
- **Cursors are server-issued, signed/bound tokens** carrying `{ connector, connection, resource,
  filter_hash, page_size, upstream_state, expiry }`. An upstream `next` URL is never used as a cursor (that
  would be an SSRF gadget); a cursor whose connection/resource/filter_hash does not match the request is
  rejected.
- **Filters are a curated subset**, declared in `schema` (search, location/category, booleans, date range,
  enum/status, schema-declared ordering keys), not a pass-through of every upstream filter, which would
  reintroduce a DSL.

### 2. Browse model + endpoints
- `GET  /api/connections/{id}/schema` → `ConnectorSchema` (cached, TTL + version).
- `POST /api/connections/{id}/browse` `{ resource, filters?, parent?, cursor?, page_size? }` → `BrowsePage`.
- `POST /api/connections/{id}/materialize` `{ rows, fields, expansion }` → `[LabelRow]`.
The frontend renders resources as grids or trees (per `View`), shows the curated filter widgets, drills via
structured relationships, paginates via `next_cursor` + `has_more`/`count`, and multi-selects. Selecting
"use for labels" calls `materialize` for exactly the mapped field keys, then maps and posts to `/batch`.

`ConnectorError` is a stable category set: `AuthFailed`, `Forbidden`, `ConnectionFailed`, `InvalidFilter`,
`UpstreamSchemaMismatch`, `RateLimited`, `PartialHydration`, `BudgetExceeded`.

### 3. Connections store (backend)
A `connections` table: `{ id, connector, base_url, credential, enabled, created_at }`, managed by a CRUD
API (`/api/connections`), mirroring the printer-store pattern. Credentials are **write-only**: never
returned by reads (redacted), updatable, deletable. Encryption-at-rest is a later hardening; redaction and
lifecycle are not.

### 4. Field mapping (separate from rendering)
Per `(connection, template)`: each template field ← a source `FieldKey` chosen from the connector's
`FieldSpec` list (typed: label, type, nullability, tier, example), a precise field picker over the
normalized model, **distinct** from template `{field}` / `{settings.*}` rendering interpolation. The chosen
field keys drive `materialize` (only those fields are hydrated). The resulting `LabelRow`s, after the
selected `ExpansionPolicy`, become data rows in the **existing editable grid** (review/edit/copies), then
post to `/batch`. Mapping config is stored per `(connection, template)`; the mapping UI flags
missing/renamed template fields and stale source keys (drift).

### 5. Generic frontend
One browse component drives any connector from `schema()` + `browse()`. Connectors may include UI hints in
the schema (e.g. a column is a thumbnail, a resource is a tree) so the generic engine can specialize
rendering without per-connector code.

## Security

- **App-level auth is a prerequisite, and connection CRUD is admin-only.** The service currently binds
  `0.0.0.0` with no auth. Storing external credentials and browsing inventory through them materially
  raises blast radius. The deferred app token-auth must land before this ships, with CSRF/origin protection
  on state-changing calls; creating/editing connections (which sets `base_url`) is an admin-only operation.
- **Server-side egress policy (one hardened HTTP client for all connectors):** `https`/`http` only with a
  port policy; connect/read timeouts; max response bytes; decompression bounds; content-type enforcement;
  do not follow cross-host redirects; ignore proxy environment variables. **Resolve and check the target
  IP**, blocking link-local, multicast, unspecified, and cloud-metadata addresses by default. `base_url` is
  user-set and often legitimately on the LAN/Docker network, so reaching private ranges is an **explicit
  per-deployment opt-in**, not the default. Connectors only call their own known API shapes; no
  client-supplied URL or path reaches the HTTP client. Residual SSRF risk is acknowledged and bounded by
  admin-only connections + app-auth + the IP/redirect policy, not eliminated.
- **Read-only** with respect to the external system's data. A connector may POST internally only to obtain
  or refresh its own auth token; it never mutates inventory.
- **Redaction** of `Authorization`, cookies, credentials, query-string tokens, and cursor payloads in all
  reads and structured logs.

## Connector capability notes (real-API grounded)

- **Homebox:** unified `/v1/entities` model (locations and items are entities); auth is a token the user
  pastes (preferred path) or obtains via the login/refresh endpoints, exposed as connector `auth_kinds`
  (`StaticToken`, `Login`), never requiring stored username/password as the only option; OIDC is a future
  capability. The connector maps entity types to resources and the location→contained-items relation.
- **InvenTree:** Django-REST style, `limit`/`offset` with `count/next/previous/results`; auth
  `Authorization: Token <T>` or Basic; relationships via path params and `?location=<pk>&cascade=true`;
  hydration via `*_detail` flags; trees (`/stock/location/tree/`); identity is `pk`. The connector encodes
  all of this in code.
- **Expansion (first-class, not deferred):** the `ExpansionPolicy` is part of the contract with two modes:
  `AsListed` (one selected row → one grid row, copies editable, the safe default suited to Homebox assets)
  and `BySerialOrQuantity { cap }` (when a row carries serial/quantity, e.g. InvenTree stock, the connector
  expands one label per serial/unit, capped and confirmed). The connector declares which modes a resource
  supports; the UI offers them on selection.

## Scope

- In: the `Connector` trait, the normalized browse model + `/schema` + `/browse`, the `connections` store
  and CRUD, field mapping → editable grid → `/batch`, security prerequisites, and the first two connectors
  (Homebox, InvenTree) as the proving implementations.
- Out / deferred: the declarative connector DSL; user-authored connector types; OAuth/refresh/cookie auth
  (beyond a connector's own simple token/login); write-back to external systems; per-connector advanced
  expansion. Adding a new connector type is a code change + release; connections are runtime config.

## Approaches rejected
1. **Declarative runtime-loaded connector DSL** — would become a programming language and still be
   incomplete for real APIs; defer until grounded by real trait impls.
2. **Frontend connector modules + generic credentialed proxy** — SSRF/abuse surface; cannot handle real
   auth; backend connectors remove the proxy entirely.

## Testing (when built)
- Per connector: a mocked external API (wiremock-style) exercising the auth header, pagination across
  pages, a relationship drill-down, `materialize` hydration, and identity. Use **realistically-shaped
  fixtures** (a Homebox `/v1/entities` page; an InvenTree stock list with `count/next/results` + a
  location tree), not only happy-path mocks. One positive + one negative (auth failure, empty page,
  budget exceeded) each.
- The generic browse/materialize endpoints against a fake connector: schema shape, browse paging
  (cursor bind/reject), error category mapping, expansion modes.
- Egress policy: cross-host redirect blocked; cloud-metadata/link-local IP blocked; oversized response
  capped; timeout enforced; secrets/cursor redacted in logs.
- Mapping → `materialize` → grid → `/batch` round-trip with a couple of records.

## Open items (resolve at implementation-plan time)
- Thumbnails: do not hand token-bearing external image URLs to the browser; serve via a narrow media
  proxy endpoint or omit thumbnails in v1.
- Schema caching invalidation (version/TTL) and per-connection schema-version pinning so a connector or
  instance upgrade does not silently change a saved mapping's meaning.
- Concrete `FilterSpec` widget set (search, id/enum, boolean, date-range, ordering) and how unknown
  upstream filters are intentionally excluded.
