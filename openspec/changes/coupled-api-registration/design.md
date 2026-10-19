# Design

## Context

See `proposal.md` for motivation. At `3dddfb20`, `api_router()` lists documented handlers independently of `ApiDoc`'s `paths(...)`. `ApiDoc` also owns schemas, tags and the relative `/api` server URL. Auth middleware wraps the assembled API before the outer `/api` nest; panic handling and SPA/assets sit outside it. Existing tests inspect `ApiDoc::openapi()`, including malformed-body coverage driven by its paths.

## Goals / Non-Goals

**Goal:** Make a documented handler registration supply its runtime method/path and OpenAPI operation together.

**Non-goal:** Reorganize handler modules or change the API/documented operation set.

## Decisions

- Default: add `utoipa-axum` 0.3 and use its `OpenApiRouter<Arc<AppState>>` with `routes!(...)`. Its published dependencies match Axum 0.8 and Utoipa 6, and it collects routes and operations from existing handler annotations: https://docs.rs/crate/utoipa-axum/0.3.0. Keeping two lists fails the requested coupling; a custom macro duplicates integration the existing ecosystem already supplies.
- Put the documented registrations in one builder. Retain schema/tag/server metadata in `src/openapi.rs` without a second paths list, and compose it with the collected operations. The runtime router, JSON document, Swagger UI and schema tests must all use that builder's collected document. Keep full-document construction separate from metadata construction to avoid recursive router/document building.
- Preserve relative documented paths and the `/api` server URL; split the router/document before adding the existing outer `/api` nest. Retain state, auth, fallback, assets, panic handling and Swagger's explicit `/api/openapi.json` URL.
- Deliberately undocumented surfaces remain `/api/openapi.json`, Swagger's `/api/docs` UI routes, assets/SPA and test-only panic routes. Add these outside collected handler registration, as today. No intentional documentation correction is planned.
- Use a focused integration check on the existing health endpoint: request `/api/health`, fetch `/api/openapi.json`, and assert its relative `/health` GET operation and `/api` server URL. It fails if the shared registration is omitted or the served document bypasses collection. Existing schema and malformed-body tests must consume the collected document; review method/path pairs against the pre-refactor lists to establish preservation without adding a second permanent registry.

## Risks / Trade-offs

Automatic component collection may alter document structure; retain explicit component registrations and compare the published operations and schemas during implementation. The integration dependency replaces the duplicate path list without introducing a project-specific registration framework.

## Migration Plan

Land as an ordinary behavior-preserving refactor. No stored-data migration is needed.
