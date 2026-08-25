## 1. The extractors

- [x] 1.1 Create `src/extract.rs` with `Json<T>`: a hand-written `FromRequest` delegating to
      `axum::Json::<T>::from_request` and mapping its rejection with `AppError::from`. No new
      dependency; do not use `#[derive(FromRequest)]`, which needs axum's `macros` feature.
- [x] 1.2 Implement `IntoResponse` for `Json<T>` by delegating to `axum::Json`. Required: nine handlers
      use `Json<T>` in response position and stop compiling without it.
- [x] 1.3 Add `Path<T>` to `src/extract.rs`: a `FromRequestParts` delegating to `axum::extract::Path`
      and mapping through `From<PathRejection>`. Do **not** implement `IntoResponse` for it; no handler
      returns a `Path<T>`.
- [x] 1.4 Declare the module in `src/lib.rs`.

## 2. The path conversion

- [x] 2.1 Rewrite `From<PathRejection> for AppError` (`src/errors.rs:422`) to stop discarding the
      rejection and branch on `rejection.status().is_server_error()` → `AppError::internal`, else
      `400 InvalidRequest` / `Reason::PathParamInvalid`. Do not match rejection variants: axum classes
      `WrongNumberOfParameters` and `UnsupportedType` as 500 *inside* `FailedToDeserializePathParams`.
- [x] 2.2 Confirm by test, not by reading, that a `500`-classified path rejection is never downgraded
      to `400`.

## 3. Sanitize the rejection log

- [x] 3.1 Close **both** log emitters, not just the obvious one:
      (a) add to `AppError` a set of detail keys that are response-only, and have `malformed_json`
      (`src/errors.rs:109-119`) mark its `error` key;
      (b) in `AppError::into_response` (`:368-396`), log `details` with those keys removed — this is the
      emitter that would otherwise leak, since it logs `details = ?self.details` for every client error
      at `:380-385`;
      (c) drop the explicit `warn!` in `From<JsonRejection>` (`:405-406`).
      The **wire** mapping must not change: `details.error` still carries the parser message on the
      response, and `details.reason` must still appear in the log.
- [x] 3.2 Test that `{"username":"admin","password":12345}` to `POST /api/auth/login` still returns
      `details.error` in the **response**, and that **no** emitted log record contains `12345`. Capture
      every record for the request with a `tracing` subscriber, not just the converter's, since the leak
      this guards against came from a second emitter. Assert `details.reason` still appears in the log,
      so the fix is not "log nothing".

## 4. Swap the extractors in

- [x] 4.1 In `src/api.rs`, import `Json` and `Path` from `crate::extract` instead of `axum::extract`.
      Add a module-level comment saying these are the crate's and why, since the shadowing is invisible
      at the call site.
- [x] 4.2 Convert the four `Result<Json<T>, JsonRejection>` sites (`:757`, `:2142`, `:2181`, `:2233`) to
      the plain extractor, dropping each `payload.map_err(AppError::from)?` and the now-unused
      `JsonRejection` import.
- [x] 4.3 `cargo check`. The fifteen bare `Json` sites and twenty-four `Path` sites should need no edit
      beyond 4.1; anything that does not compile is a finding worth reading, not a signature to patch.

## 5. Tests: JSON bodies

- [x] 5.1 Table-driven test over all nineteen JSON endpoints: malformed body → `400` / `InvalidRequest`
      / `json_malformed` / non-empty `details.error`. Every request must be *admitted* — authenticated
      per §11 with an acceptable origin — or it will measure the middleware instead.
- [x] 5.2 Wrong-shape body and missing-required-key cases on `PUT /api/connections/{id}`: both `400`,
      both `json_malformed`. Assert `400` explicitly; `422` is the bug being fixed.
- [x] 5.3 Content-type cases: absent → `415`; `text/plain` → `415`; `application/problem+json` →
      **not** `415`, body deserialized. axum accepts any `application/*+json`.
- [x] 5.4 Oversized body to `POST /api/print` → `413` / `PayloadTooLarge`.
- [x] 5.5 Assert the four already-enveloped endpoints are byte-identical before and after. This is what
      proves the 4.2 collapse changed nothing.

## 6. Tests: path parameters and admission

- [x] 6.1 `GET /api/templates/%FF/source` → `400` / `path_param_invalid`, JSON envelope. This is the live
      case on a current route; do not settle for a unit test constructing the rejection.
- [x] 6.2 A segment that does not parse as its declared type → `400` / `path_param_invalid`.
- [x] 6.3 Both server-classified cases → `500` / `Internal`, asserted separately because they travel
      different axum paths: route/handler arity disagreement, and path parameters absent from the request.
- [x] 6.4 Admission precedence, all three outcomes, each with a **malformed** body so the test shows the
      body does not outrank §11: no credentials → `401`; mismatched origin → `403`; auth-managed route
      under `LABELER_NO_AUTH=true` → `403`.

## 7. Tests: coverage and OpenAPI

- [x] 7.1 Source-level check that `src/api.rs` binds `Json` and `Path` from `crate::extract`, so the
      import swap cannot be silently reverted. A grep-style assertion, not a guarantee: the spec says
      this is convention. Do not attempt a general "no handler may use axum's extractor" check; three
      such mechanisms were tried in review and none is achievable.
- [x] 7.2 No OpenAPI assertion is made: the extractor swap is invisible to utoipa because request bodies
      and path parameters are declared explicitly in each `#[utoipa::path]` attribute rather than
      derived from extractor types.

## 8. ADR and docs

- [x] 8.1 **Re-scan ADR numbers first**, across `main` and every worktree, immediately before creating
      the file. `0075` is a starting guess, not an allocation: `issue-226` took `0074` mid-review after
      this change had checked it.
- [x] 8.2 Write `docs/adr/0075-request-rejections-use-the-error-envelope.md` recording both breaking
      moves (`422`→`400` for wrong-shape bodies; path rejections into the envelope), the widened
      `json_malformed` definition, the `Internal` row added to §10's table, and the log sanitization.
- [x] 8.3 Add its row to `docs/adr/README.md`.

## 9. Gates

- [x] 9.1 `cargo fmt`
- [x] 9.2 `cargo clippy --all-targets --all-features` — fix root causes, never `#[allow]`
- [x] 9.3 `cargo test`
- [x] 9.4 Against a running server with `LABELER_NO_AUTH=true`, `curl` one endpoint per rejection class
      and read the responses. A green test run is not the same as having seen the envelope.
