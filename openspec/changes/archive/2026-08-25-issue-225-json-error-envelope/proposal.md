## Why

Implements [#225](https://github.com/pfa230/labeler/issues/225).

`docs/SPEC.md` §10 opens with "All errors return JSON: `{ "error": { "code": ..., "message": ...,
"details": ... } }`", and §10.1 makes `details.reason` machine-readable and part of the contract. The
request layer does not honour either. Fifteen handlers take a bare `Json<T>` and twenty-four take a
bare `Path<T>`, so when axum cannot deserialize the body or a path segment it returns its own
rejection: `text/plain`, no `code`, no `details.reason`, and `422 Unprocessable Entity` for a body
that is valid JSON of the wrong shape where §10 says `400 InvalidRequest`.

Only four handlers do it right, by taking `Result<Json<T>, JsonRejection>` and converting through
`From<JsonRejection> for AppError` (`src/errors.rs:400`): `PUT /api/templates/{id}/group`,
`POST /api/batch`, `POST /api/print`, `POST /api/render/label`. The conversion exists and is correct;
fifteen of the nineteen JSON extraction sites simply never reach it.

The path half is worse than inconsistent, it is dead. `From<PathRejection> for AppError`
(`src/errors.rs:422`) has **zero call sites** in `src/`, so the `path_param_invalid` reason that
§10.1 publishes as contract is unreachable in the running server. A client cannot switch on
`error.code` for any of this, which is exactly what §10 promises it can.

## What Changes

- **Two project extractors replace axum's at every site**, in a new `src/extract.rs`:
  - `Json<T>`, a manual `FromRequest` impl that delegates to `axum::Json` and maps its rejection
    through the existing `From<JsonRejection> for AppError`. Handler destructuring is unchanged
    (`Json(body): Json<ConnectionInput>`), so the fifteen bare sites change by import only.
  - `Path<T>`, a `FromRequestParts` impl doing the same through `From<PathRejection>`.
  - **`Json<T>` alone** also implements `IntoResponse`, delegating to `axum::Json`, because nine
    handlers return `Json<T>` as their response type (`-> Result<Json<ProbeResponse>, AppError>` and
    kin). A wrapper that only extracts would break those nine at compile time. `Path<T>` gets no
    `IntoResponse`: no handler returns one.
- **The four `Result<Json<T>, JsonRejection>` sites collapse to the new extractor**, dropping the
  `payload.map_err(AppError::from)?` line each carries. Their observable behavior is unchanged; this
  is what stops the codebase from having two right answers.
- **`From<PathRejection>` stops flattening a server bug into a client error.** It currently maps
  every variant to `400 InvalidRequest` / `path_param_invalid`. Some path failures are this service's
  fault, not the caller's: a handler declaring a different number of parameters than its route, a type
  that cannot be deserialized from path parameters at all, or a request that arrived without the
  router's parameters attached. axum already statuses those `500`, and flattening them to `400` would
  tell a client its URL was wrong when the URL was fine. After this change the conversion branches on
  the status axum itself assigns — server error stays `5xx` as `Internal`, everything else becomes
  `400 InvalidRequest` / `path_param_invalid` — rather than enumerating rejection variants.
- **BREAKING (error contract, fifteen endpoints).** A body that is valid JSON of the wrong shape
  moves from `422` + plain text to `400` + `{"error":{"code":"InvalidRequest","details":{"reason":
  "json_malformed", "error": "<parser message>"}}}`. A client matching on `422` for these endpoints
  breaks. This aligns them with §10 and with the four endpoints that already behave this way; the
  status change is the defect being fixed, not a side effect.
- **BREAKING (error contract, path params).** A path segment that fails to deserialize moves from
  axum plain text to the envelope with `path_param_invalid`. This is reachable on today's routes, not
  only on future ones: every site is `Path<String>`, but a segment that percent-decodes to invalid
  UTF-8 fails even a `String`, so `GET /api/templates/%FF/source` changes shape. The requirement is
  also written so the twenty-fifth site, taking a `Path<u32>`, gets the envelope without anyone
  remembering to ask for it.
- **`json_malformed`'s published definition widens; no endpoint changes what it emits.** §10.1
  defines it as "not parseable JSON", but the service has always emitted it for a syntactically valid
  body of the wrong shape too, on the four endpoints that already return the envelope. The spec delta
  redefines the reason as "could not be deserialized into the endpoint's type" so the published
  contract matches the behavior, rather than adding a second slug and changing what those four
  endpoints report. To be exact about who changes: those four report the same reason before and after;
  the other fifteen report no reason at all today and begin reporting `json_malformed`. `details.error` remains the only way to tell a syntax error from a shape mismatch.
- **Coverage is convention plus a distinct type, which is all that exists.** The extractors live in
  `src/extract.rs` and `src/api.rs` imports them as `Json` and `Path`, so the obvious way to write a
  handler is the correct one. Rust and axum offer no way to make the framework's own extractor
  unusable, and the accepted practice in axum codebases is exactly this: an application-owned type,
  convention, and review. Three stronger mechanisms were tried during review and each failed for its
  own reason; they are recorded in `design.md` so nobody retries them. The endpoint inventory test
  proves the nineteen endpoints that exist behave, and the spec states outright that this is a
  convention and not a guarantee.
- **The rejection log stops echoing the request body, while the wire mapping is untouched.**
  `From<JsonRejection>` logs the parser's full `body_text()` at WARN (`src/errors.rs:405-406`), and the
  default filter is `labeler=info` (`src/main.rs:160-164`), so that line ships. Today only four
  endpoints reach it; routing the other fifteen through it would newly put parser text from
  `POST /api/auth/login`, `/auth/setup`, `/auth/password` and `/users` into ordinary logs, and those
  bodies carry `password` (`src/api.rs:2475-2478`) and `current_password`/`new_password`
  (`:2818-2821`). serde's diagnostics quote an unexpected value verbatim, so a mistyped credential
  payload could be persisted in plaintext — CWE-532. axum itself logs rejection bodies only at TRACE,
  so this would be a regression introduced by the fix. Two emitters carry it, not one: the explicit
  `warn!` in the converter (`src/errors.rs:406`) and `AppError::into_response`, which logs
  `details = ?self.details` for every client error (`:380-385`) and so re-emits the parser message that
  `malformed_json` stored under `details.error` (`:109-119`). Both are closed by one mechanism:
  `AppError` marks response-only detail keys, `into_response` omits them from the log, and the converter
  drops its own `warn!`. `details.reason` still reaches the log; `details.error` still reaches the
  caller. Demoting the level instead was considered and rejected, since a deployment can enable a lower
  level and collectors gather every level. `details.error` on the **response** is unchanged: it is the contract, it
  goes only to the caller who sent the body, and the spec's mapping does not move.
- **`PayloadTooLarge` and `UnsupportedMediaType` keep their current mapping.** `413` for a body over
  the limit (including `POST /api/print`'s 64 KiB `DefaultBodyLimit`, `src/api.rs:261`) and `415` for
  a missing or non-JSON `Content-Type` are already produced by `From<JsonRejection>` and are
  unchanged. "JSON" here means `application/json` **or** any `application/<subtype>+json`, which is
  what the framework accepts; the spec states that explicitly so a vendor media type is not later
  "fixed" into a rejection.
  Naming them matters because routing everything through one extractor is exactly where they would
  be lost by accident.
- Not in this change: the `Query<T>` extractor, which has its own rejection type and no existing
  `From` impl; the raw-YAML body endpoints (`POST /api/templates`, `PUT /api/templates/{id}`), which
  do not use `Json`; `POST /api/import/csv`, which takes a raw `text/csv` body (`src/api.rs:2334`) rather than a JSON one; and the `/api/*` 404 fallback,
  which already returns the envelope.

## Capabilities

### New Capabilities

- `request-error-envelope`: what the service returns when the request layer, not the handler,
  rejects a request. Covers the complete mapping from every `Json` and `Path` extractor failure to
  status, `code`, `details.reason` and `details.error`; the precedence of authentication and origin
  checks over that mapping; and the rule that a handler reading a JSON body or a path parameter gets
  the mapping by writing the obvious thing rather than by remembering to ask. It also covers the rule
  that a rejected body is never echoed into the service log, which is a security property rather than a
  diagnostic preference.

### Modified Capabilities

None. `openspec/specs/` holds `auto-length-layout`, `connections`, `connector-browser`,
`connector-field-transforms`, `datetime-params`, `default-connection`, `template-format-badge`,
`template-groups`, `template-registry` and `ui-colour-palette`. `connections` specifies the two connection endpoints whose
bodies this touches, but it specifies their handler semantics, not what happens to a body that never
reaches the handler, so no requirement in it changes.

This is a **first-touch** on behavior documented only in the frozen spec, so the new capability
states the complete post-change contract and names what it supersedes: `docs/SPEC.md` §10's blanket
"All errors return JSON" claim as it applies to extractor rejections; §10.1's `json_malformed`
(redefined and widened), `request_body_invalid` and `path_param_invalid` rows; and §10's code table,
**for the addition of an `Internal` (500) row only**. `AppError::internal` already exists
(`src/errors.rs:332`) and the table never listed it, so the requirement that returns `500 Internal`
for a routing defect would otherwise contradict the table it claims not to touch. Every other row,
and every other reason, is untouched and stays authoritative.

## Impact

- **Code.** New `src/extract.rs` (~60 lines, two extractors). `src/api.rs`: the `axum::extract`
  import swaps `Json` and `Path` for the crate's, fifteen `Json` sites and twenty-four `Path` sites
  change by that import alone, and four sites drop a `map_err` line and their `JsonRejection` import.
  `src/errors.rs`: `From<PathRejection>` stops discarding the rejection it is handed and branches on
  `rejection.status().is_server_error()`, deferring to axum's own client/server classification rather
  than matching rejection variants.
- **Auth precedence is unchanged and now stated.** `require_auth` wraps the whole API router
  (`src/api.rs:291-295`) and runs before extraction, so `401`/`403` continue to outrank any body
  mapping and frozen §11 keeps precedence. This change specifies that ordering rather than altering
  it; the spec's admission requirement exists so the coverage test measures the extractor and not the
  middleware.
- **API.** No route, request-body schema or success response changes. Error responses change shape
  for a rejected JSON body on fifteen endpoints, and for a rejected path parameter on every route
  taking one — twenty-four sites, reachable today via a segment that percent-decodes to invalid UTF-8.
  The four already-enveloped JSON endpoints are unchanged.
- **OpenAPI.** `src/openapi.rs` needs no new registration: the twenty-two `#[utoipa::path]` blocks
  declare `request_body` explicitly rather than inferring it from the extractor type, so a custom
  extractor is invisible to the generated document.
- **Dependencies.** None. The manual `FromRequest` impl is chosen over axum's `#[derive(FromRequest)]`
  precisely to avoid adding `axum`'s `macros` feature; `axum-macros` is absent from `Cargo.lock`
  today.
- **Docs.** ADR-0075 and its row in `docs/adr/README.md`. The ADR records both breaking contract
  moves and the widened definition of `json_malformed`. `0070` is claimed by `issue-212`, `0072`, `0073` **and `0074`** by
  `issue-226`, and `0071` is on `main`; `0067` is an unused gap. `issue-226` added its `0074` claim
  after this change first checked, so the number **must be re-verified against every worktree at the
  moment the ADR file is written**, not trusted from this document.
- **Tests.** A table-driven integration test over all nineteen JSON endpoints asserting the envelope
  for each rejection class. Unit tests on `From<PathRejection>` for both variants. The four
  already-correct endpoints get the same assertions, which is what proves the collapse changed
  nothing for them; `malformed_json_body_keeps_its_shape` (`src/lib.rs:1800`) already covers one of
  them and is the pattern to generalise. Path coverage is an endpoint test for each of the four
  outcomes — invalid UTF-8 (`400`), a segment that does not parse as its declared type (`400`), a
  route/handler arity disagreement (`500`), and path parameters absent from the request (`500`) — not
  unit tests that merely construct the rejection variants, which would prove nothing about which status
  the service actually returns. The two `500` cases are listed separately on purpose: they travel
  different axum paths, one as `WrongNumberOfParameters` inside `FailedToDeserializePathParams` and one
  as the distinct `MissingPathParams`, and conflating them is exactly what produced this change's
  round-1 blocking defect. Plus the OpenAPI-derived
  coverage test described above, and one asserting a suffixed `+json` media type is still accepted.
  Every body-rejection assertion runs as an admitted caller — authenticated, acceptable origin — and
  the matrix additionally pins all three admission outcomes themselves (`401` with no credentials,
  `403` on a mismatched origin, and `403` on an authentication-managed route under
  `LABELER_NO_AUTH=true`) so a future change cannot quietly let a malformed body outrank §11. The
  third case needs a malformed body specifically: the existing no-auth coverage at `src/lib.rs:6335`
  sends valid bodies, so it cannot show that a rejected body does not outrank the admission result.
- **Collides with the unmerged `issue-197` branch.** #225 cites a test at `src/lib.rs:920-975`; that
  test does not exist on `main`. It is
  `update_connection_undeserializable_body_is_rejected_before_the_connector_check`, at
  `src/lib.rs:924-981` on the `issue-197-connector-immutable` branch, and its doc comment and
  assertions pin the current behavior: "The status is axum's own (400 for a syntax error, 422 for a
  shape error) and the body is its plain text, not this API's error envelope". That test **will fail**
  once this change lands. Whichever branch merges second must update it to assert the envelope; it is
  named here so that is a decision and not a surprise. Nothing else in #197 depends on this change,
  and the narrowing #197 made (the connector comparison cannot outrank a body that never
  deserializes) stays correct either way.
- **Compatibility.** The UI is the main client and today it degrades on exactly these fifteen:
  `toError` (`ui/src/api/client.ts:19-26`) parses the envelope only when the response is
  `application/json` and otherwise constructs `ApiError` with the literal code `"Unknown"` and
  axum's plain text as the message. So every bare-`Json` rejection currently surfaces as an
  `"Unknown"`-coded error, and moving into the envelope can only improve it. The `422` → `400` move
  touches no UI branch: the only status branches in `ui/src` are `Catalog.tsx:138,154` (409 and 422
  on `POST /api/templates`, a raw-YAML endpoint outside this change) and the three
  `err.code === "BatchInvalid"` checks (`Connect.tsx:269`, `PrintForm.tsx:137`, `Import.tsx:339`),
  which are on `/api/batch`, already enveloped.
- **Evidence.** Backend and contract-shaped, so the proof is the test matrix plus a live `curl` of one
  endpoint per rejection class against a running server, not a green `cargo test` alone.
