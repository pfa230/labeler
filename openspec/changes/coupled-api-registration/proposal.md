# Proposal

## Why

API handlers are registered independently in the router and OpenAPI path list, allowing a future endpoint to be served without documentation. The existing Utoipa integration provides a compatible way to collect both from one registration.

Source issue: https://github.com/pfa230/labeler/issues/229

Fixes #229

## What Changes

- Register documented handlers through `utoipa-axum`, producing routing and OpenAPI operations together.
- Serve the collected document through the existing JSON and Swagger endpoints, retaining explicit schema metadata.
- Replace the independent OpenAPI paths list and check a real endpoint through the coupled registration.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None. This refactor preserves the HTTP surface and published operations; `skip_specs: true`.

## Impact

Touches `src/api.rs`, `src/openapi.rs`, OpenAPI tests in `src/lib.rs`, and the Cargo manifest and lockfile. Adds `utoipa-axum` 0.3, compatible with the existing Axum 0.8 and Utoipa 6.

Non-goals: route reorganization, endpoint changes, new public documentation operations and a custom registration macro. Implementation follows the ordinary refactor workflow.
