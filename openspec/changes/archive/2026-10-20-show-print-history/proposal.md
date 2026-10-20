## Why

Print dispatch already records successes and failures, but operators cannot inspect those records to trace a print. A shared history screen makes the recorded time, template, printer, actor and failure detail visible.

Source issue: https://github.com/pfa230/labeler/issues/407

Fixes #407

## What Changes

- Expose recorded jobs newest first through `GET /api/jobs`, visible to every authenticated user and in no-auth mode.
- Add Print history at `/print-history`, between Import and Settings in primary navigation.
- Show recorded print facts and full failure detail, with loading, empty and error states.

## Capabilities

### New Capabilities

### Modified Capabilities

- `printing`: job-history read contract.
- `ui`: print-history screen and navigation placement.

## Impact

A read query in `src/store.rs`, an API handler/response model and OpenAPI registration, a React Query hook, page and shell route/navigation. Reuse the existing jobs table and actor identity. Activity beyond printing remains with #94.
