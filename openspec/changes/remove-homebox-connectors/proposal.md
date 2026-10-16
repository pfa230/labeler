## Why

Homebox access already belongs to the separate Homebox tooling, and agents can supply its data to Labeler's MCP adapter; CSV import covers exceptional batches. Labeler's built-in integration duplicates that access and maintains a connector framework for a browser workflow we have chosen to remove.

Fixes #452

## What Changes

- **BREAKING** Remove inventory connection CRUD, schema, browse and materialize endpoints and their OpenAPI surface, together with the Homebox connector and connector-only HTTP handling and errors.
- **BREAKING** Remove Connect and Connections navigation and pages, inventory browsing, field mapping and connector row metadata.
- **BREAKING** Remove `default_connection_id` from application settings and migrate stored databases to drop inventory connection records and that override while preserving unrelated state.
- Retain the existing REST/MCP supplied-data workflows, CSV import, shared grid behavior and generic templates, including QR templates that happen to reference Homebox URLs.
- Update current documentation to describe these remaining workflows without advertising built-in inventory connectors.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `connections`: Remove every inventory connection and connector requirement.
- `settings`: Remove the default inventory connection setting and define cleanup of persisted connector state.
- `ui`: Remove the connector screens and mapping requirements; scope shared batch behavior to Import.
- `errors`: Remove connector-only codes, reason slugs and record kinds, and move the malformed-body example to a retained endpoint.

## Impact

Backend changes touch `src/connector/`, `src/egress.rs`, application state and routes in `src/api.rs`, SQLite migrations and connection methods in `src/store.rs`, settings, errors/reasons and OpenAPI registration. Frontend changes touch Connect/Connections routes and navigation, their pages and helpers, connector row metadata, and associated styles and tests. Shared CSV grid helpers must remain available without connector imports. Documentation changes include `README.md` and `docs/VISION.md`.

This removes Labeler's direct Homebox dependency; it does not change the separate Homebox MCP service/CLI or add a replacement connector, CSV list syntax, compatibility endpoint or deprecation period. Existing MCP tools, rendering and printer behavior keep their contracts. Archived change records remain historical records.
