# 75. Request rejections use the error envelope

Date: 2026-08-25

## Status

Accepted. Issue [#225](https://github.com/pfa230/labeler/issues/225). Partially supersedes [docs/SPEC.md](../SPEC.md) §10 (error envelope applied to extractor rejections, `Internal` code added to table) and §10.1 (`json_malformed` definition widened, `path_param_invalid` and `request_body_invalid` rows).

## Context

`docs/SPEC.md` §10 specifies that all errors return JSON in a structured envelope (`{ "error": { "code": ..., "message": ..., "details": ... } }`), and §10.1 defines machine-readable discriminator strings for `details.reason`. However, fifteen API handlers extracted bare `Json<T>` and twenty-four extracted bare `Path<T>` directly from `axum::extract`. When request extraction failed at those sites, axum produced its own built-in `text/plain` rejections:
- Deserializing valid JSON of the wrong shape or missing required fields produced `422 Unprocessable Entity` with plain text instead of the `400 InvalidRequest` required by §10.
- Deserializing path parameters produced plain text and completely bypassed `From<PathRejection> for AppError`, leaving the published `path_param_invalid` reason unreachable in the running server.

Only four handlers (`PUT /api/templates/{id}/group`, `POST /api/batch`, `POST /api/print`, and `POST /api/render/label`) handled rejections properly via `Result<Json<T>, JsonRejection>`.

Additionally, `From<JsonRejection>` logged the parser's full body text at `WARN`, and `AppError::into_response` logged `details = ?self.details` for every client error. Because four endpoints handle credentials (`POST /api/auth/login`, `/auth/setup`, `/auth/password`, and `/users`), routing those endpoints through standard error mapping would leak sensitive request payloads (including passwords) into application logs (CWE-532).

## Decision

1. **Application-level extractors in `src/extract.rs`**: Define `Json<T>` and `Path<T>` wrapping axum's extractors. `Json<T>` implements `FromRequest` (delegating to `axum::Json` and mapping rejections via `AppError::from`) and `IntoResponse` (delegating to `axum::Json::into_response` to keep the nine response-position handlers compiling). `Path<T>` implements `FromRequestParts` mapping rejections via `From<PathRejection> for AppError`.
2. **Shadowing by convention**: `src/api.rs` imports `Json` and `Path` from `crate::extract` instead of `axum::extract`. The obvious syntax at handler definitions automatically resolves to the crate's envelope-generating extractors.
3. **Collapse the four manual sites**: Convert the four existing `Result<Json<T>, JsonRejection>` handlers to use bare `Json<T>`, eliminating redundant boilerplate.
4. **Preserve server error classification for path rejections**: `From<PathRejection> for AppError` checks `rejection.status().is_server_error()`. Framework-classified server errors (e.g. handler/route arity mismatch or missing path parameters) become `500 Internal` (`AppError::internal`), while client-attributable rejections (e.g. invalid UTF-8 or type parse failures) become `400 InvalidRequest` with `Reason::PathParamInvalid`. This adds `Internal` (500) to the code table in §10.
5. **BREAKING: 15 endpoints move wrong-shape bodies from `422` to `400`**: Deserialization failures on all 19 JSON endpoints now consistently return `400 InvalidRequest` with `details.reason = "json_malformed"` and `details.error` containing the deserializer diagnostic.
6. **BREAKING: Path rejections move from plain text to the error envelope**: Malformed path parameters return `400 InvalidRequest` with `details.reason = "path_param_invalid"`.
7. **Widened `json_malformed` definition**: Redefine `json_malformed` in the published contract as "could not be deserialized into the endpoint's type", covering both syntax errors and payload shape mismatches.
8. **Sanitize rejection logs**: `AppError` tracks response-only detail keys (marking `error` in `AppError::malformed_json`). `AppError::into_response` strips these keys before emitting logs, and the explicit `warn!` in `From<JsonRejection>` is removed. `details.error` continues to reach the caller in the HTTP response, while logs safely record classification (`details.reason`) without echoing request bodies.
9. **Authentication and admission precedence**: Request admission middleware (`require_auth`) runs before extractors, so unauthenticated requests receive `401` and origin-mismatched requests receive `403` regardless of request body validity.

## Consequences

- All nineteen JSON endpoints and all path parameter endpoints return uniform JSON error envelopes under `AppError`.
- External clients matching on HTTP 422 for malformed payloads must update to match HTTP 400.
- Internal service routing defects on path parameters surface as 500 Internal instead of misleading client 400 errors.
- Sensitive request fields such as passwords are never written to log files on parser rejection.
- Handler authoring in `src/api.rs` uses standard `Json` and `Path` annotations without special error-handling wrappers.
