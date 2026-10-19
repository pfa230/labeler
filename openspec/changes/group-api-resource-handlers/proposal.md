## Why

Resource routes are collected at the top of `api.rs`, away from their handlers; printer CRUD and probe handlers are further interrupted by settings handlers. Co-locating each cohesive route group with its handlers lets a resource change be followed in one module while the application router stays explicit.

Source issue: https://github.com/pfa230/labeler/issues/249

Fixes #249

## What Changes

- Group template operations/favorites/recents, printer operations, settings/variables, rendering/printing and identity operations into five API child modules.
- Move each group's existing route declarations alongside its handlers and local helpers; merge them at the existing API composition point.
- Retain the existing shared AppState, middleware and application setup, with minimal visibility changes.

## Capabilities

### New Capabilities

### Modified Capabilities

None. This is a behavior-preserving refactor; `.openspec.yaml` sets `skip_specs: true`.

## Impact

`src/api.rs`, five files under `src/api/`, and qualified references in `src/openapi.rs`. Preserve `labeler::api::valid_printer_id`, used by the MCP binary. No contract changes or new dependencies. #229 separately owns coupling route and OpenAPI registration.
