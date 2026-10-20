## Context

See proposal.md for motivation. The current `api_router` declares resource routes independently of their handler locations. Printer create/get precede settings, while replace/delete/probe/cut follow settings. Template writes have local filesystem/validation helpers; auth/users/tokens share account/password/session helpers and `HttpsHint`; render/download/print share `run_batch` and `BatchTarget`. These cohesive dependency groups, rather than a line count, justify the boundaries below.

`AppState` owns the registry, directory, lock and Store and is imported by middleware. `src/openapi.rs` references handler paths and API-local response/request types. The MCP binary imports `labeler::api::valid_printer_id`; tests reference the API panic marker. Those are the concrete cross-module consumers inspected.

## Goals / Non-Goals

Co-locate route definitions, handlers and their local helpers, with one clear composition point. This pure refactor sets `skip_specs: true` and needs no spec delta or mandatory ADR. Do not split shared state, redesign handlers or models, alter visibility beyond consumers, change route registration technology, move middleware, add a service/repository layer, or implement #229's registration coupling.

## Decisions

**Routine grouping defaults:** Keep `src/api.rs` as the parent and add these child files, each with its resource router next to its handlers:

| Child module | Existing code to group | Concrete benefit |
|---|---|---|
| `templates.rs` | template list/CRUD/source/thumbnail/reload and their local helpers, favorites/recents | One place to follow template admission, registry publication and actor-specific template lists. Separate favorites modules would split a small registry-dependent group. |
| `printers.rs` | printer CRUD/probe/cut, validation and probe response types | Removes the current settings interruption and keeps connection validation/probing beside printer routes. |
| `settings.rs` | variables, typed settings and datetime preview with their API models/helpers | Routes and their small store-backed editors can be followed together; no separate module per setting. |
| `rendering.rs` | label render, batch download/print, `run_batch`, `BatchTarget`, download helper and render query | Keeps the existing shared batch path private rather than exposing it across separate render/print modules. |
| `auth.rs` | auth, users, tokens, API models, validation/session helpers and `HttpsHint` | Keeps the shared account/password/token-owner helpers private; separate users/token files would require new sharing. |

Keep AppState, `app`, API composition, health, OpenAPI/Swagger setup, SPA fallback and panic handling/test routes in the parent. Each child returns `Router<Arc<AppState>>`; parent composition merges the five routers before the existing API auth layer. Preserve `/api` nesting, state injection, fallback and outer trace/panic layer placement. Axum's [Router documentation](https://docs.rs/axum/latest/axum/routing/struct.Router.html#method.merge) supports merging routers with the same state type; children need no middleware or fallback of their own.

Child modules and the handler/type paths consumed by `src/openapi.rs` need crate visibility. Their router factories need only parent visibility. Existing AppState fields and methods can remain private because children can access their parent's private items; add no getters and do not make fields public. Keep leaf-only helpers private. Re-export the existing public `valid_printer_id` at `api::valid_printer_id` for the actual MCP consumer, rather than widening unrelated helpers or building a compatibility facade for every moved handler. Keep the test panic constants in the parent. Record any additional visibility change with its actual consumer in the implementation diff review.

Update OpenAPI qualified paths/imports directly to the owning child; preserve generated operation/schema names and payloads. #229 remains a separate issue: router merging does not couple the independent route/OpenAPI lists. If it lands first, use its actual registration mechanism without reverting it. If #407 has landed, keep its read handler beside print dispatch in `rendering.rs` rather than creating a sixth one-handler module.

## Risks / Trade-offs

Moving declarations can change paths used by utoipa macros, or accidentally move middleware across the API boundary. Review the composition diff and use the existing HTTP/OpenAPI/auth/panic/MCP coverage; no new behavior is proposed that needs a new test framework. Keep request extractors from `crate::extract`, error constructors and handler bodies intact. Existing tests must catch a missing route, changed authentication/extraction or missing published operation.

The result has five explicit router factories rather than one route chain, traded for resource-local navigation and existing-helper locality. A generic registration framework, separate DTO tree or shared API utility module would add indirection without serving this split.
