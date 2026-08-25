## Context

See `proposal.md` — Why. The constraints that shape the approach:

- `From<JsonRejection> for AppError` (`src/errors.rs:400-419`) already produces exactly the mapping
  the spec delta requires, including the `413` and `415` cases. **No new error mapping is written for
  the JSON half of this change.** The work is entirely about reaching that impl from every site.
- `From<PathRejection> for AppError` (`src/errors.rs:422-426`) takes `_rejection` and discards it,
  returning `400 InvalidRequest` / `path_param_invalid` unconditionally. It has zero call sites in
  `src/`, so it has never run in production and its behavior has never been observed.
- Nine handlers use `Json<T>` as a **response** type (`-> Result<Json<ProbeResponse>, AppError>` and
  kin), not just as an extractor. Any replacement type has to serve both roles or those nine stop
  compiling.
- `axum = "0.8"` is declared with default features and `axum-macros` is absent from `Cargo.lock`, so
  `#[derive(FromRequest)]` is not available without a dependency change.
- `src/api.rs` refers to `std::path::Path` only in fully-qualified form (`:113`, `:399`, `:423`), so
  importing a crate-local `Path` into that module collides with nothing.

## Goals / Non-Goals

**Goals:**

- One place defines what a rejected request body or path parameter returns, and the obvious way to
  write a handler reaches it. Held by convention plus a distinct type and an endpoint inventory test,
  which is what the ecosystem actually offers; the spec says so rather than promising a guarantee.
- The four endpoints that already behave correctly keep behaving identically, and that is proven
  rather than assumed.
- A routing defect in this service stays a `500` and does not masquerade as a client error.

**Non-Goals:**

- Changing what any handler does with a body it successfully received. Every success path, validation
  error and status code produced *inside* a handler is untouched.
- Extending the envelope to `Query<T>`, multipart, or the raw-YAML template endpoints. See
  `proposal.md` — What Changes for why each is excluded.
- Rewriting `From<JsonRejection>`'s **mapping**. That mapping is the contract the spec delta writes
  down, and changing it would change the four endpoints that are already correct. Its *log event* is
  not part of that contract and is changed deliberately; see Decisions.

## Decisions

### Manual `impl FromRequest`, not the derive macro and not `WithRejection`

axum 0.8 documents three ways to customise an extractor rejection: `axum_extra::WithRejection`,
`#[derive(FromRequest)]` with `via`/`rejection`, and a hand-written `FromRequest` that delegates to a
built-in extractor.

- `WithRejection<Json<T>, AppError>` changes the handler signature at all nineteen sites and leaves
  the type visible in every destructuring pattern. It is the noisiest of the three at the call site,
  which is where this change wants to be invisible.
- `#[derive(FromRequest)]` is the least code but needs `axum`'s `macros` feature, adding
  `axum-macros` to the dependency tree for roughly fifteen lines of generated code.
- The hand-written impl adds no dependency, keeps the call sites unchanged, and is short enough to
  read in full.

Chosen: the hand-written impl, in a new `src/extract.rs`. `Json<T>` delegates to
`axum::Json::<T>::from_request` and maps the rejection with `AppError::from`; `Path<T>` does the same
through `FromRequestParts` and `From<PathRejection>`. `Json<T>` additionally implements `IntoResponse` by
delegating to `axum::Json`, which is what keeps the nine response-position uses compiling; `Path<T>`
does not, because nothing returns one.

### The crate's extractors take the names `Json` and `Path`

The alternative is distinct names (`ApiJson`, `ApiPath`), which make every call site say which
extractor it is using.

Chosen: shadow the names. `src/api.rs` changes `use axum::extract::{... Json, Path ...}` to import
those two from `crate::extract` instead, and thirty-nine handler signatures are then correct without
being edited. That is also what satisfies the spec's "applies to a newly added endpoint by default"
requirement in the most direct way available: inside `src/api.rs`, writing `Json(body): Json<T>` — the
obvious thing to write — resolves to the crate's extractor. Regressing requires explicitly writing
`axum::Json`, which is a visible act rather than an omission.

The cost is that the shadowing is invisible at the call site, so a reader of one handler cannot tell
which extractor is in play without checking the imports. A module-level comment in `src/api.rs`
stating that `Json` and `Path` are the crate's, and why, is the mitigation.

### Enforcement is convention plus a distinct type, because nothing stronger exists

Three mechanisms were tried across review and all three failed. The record matters, because the
failures were not independent:

1. **A `clippy.toml` `disallowed-types` entry.** `axum::Json` and `axum::extract::Json` are the same
   type, so it fires in `src/errors.rs` (which uses `axum::Json` for the error body) as well as
   `src/extract.rs`. Two `#[allow]` suppressions, against an `AGENTS.md` rule forbidding them.
2. **A source-text guard.** Blind to this codebase's own import style: `src/api.rs:2-8` nests the
   extractors, so no qualified `axum::Json` string appears anywhere in the file.
3. **A coverage test derived from `ApiDoc::openapi()`.** Workable, but routes and the API description
   are maintained separately here, so an unpublished route escapes it. Closing that means enforcing
   route-to-doc completeness, which is a different problem.

The common cause was not three unlucky implementations. It was a requirement that asked for something
that does not exist: **Rust and axum provide no way to make a handler unable to use the framework's own
extractor.** `axum::Json<T>` is a valid extractor and any handler may name it. The accepted practice in
axum codebases is an application-owned extractor type plus convention and review, with a CI grep as an
optional strictness measure and explicitly not as a guarantee. axum's own
`customize-extractor-error` example defines "our own `Json` extractor" and stops there.

Chosen: match the accepted practice. The extractors live in `src/extract.rs`, `src/api.rs` imports them
under the names `Json` and `Path` so the obvious thing to write is the correct thing, a module-level
comment says why, and the endpoint inventory test proves the endpoints that exist behave. The spec
states plainly that this is convention backed by a distinct type rather than a structural guarantee.

The earlier requirement claimed more and generated the three failed mechanisms plus the
published-versus-owed distinction that came with them. Deleting it removes the machinery and the gap it
had to describe, and leaves the change saying what it actually does.

### The rejection log is sanitized, because this change multiplies its reach

`From<JsonRejection>` logs the parser's complete `body_text()` at WARN
(`src/errors.rs:405-406`), and the default filter is `labeler=info` (`src/main.rs:160-164`), so the
line is on in a stock deployment. axum's own equivalent logs at TRACE, which is off by default.

Today four endpoints reach that line. This change routes fifteen more through it, four of which take
credential-bearing bodies: `Credentials { username, password }` (`src/api.rs:2475-2478`) on
`/auth/login` and `/auth/setup`, and `PasswordChange { current_password, new_password }`
(`:2818-2821`) on `/auth/password`, plus `/users`. serde's data-error diagnostics can quote an
unexpected value verbatim, so a client that sends a malformed credential payload — a stray quote, a
number where a string was expected — could have password material written to ordinary logs and
retained by whatever ships them.

This is a regression the fix would introduce, not a pre-existing condition, because the endpoints that
would newly log are exactly the ones handling secrets. The specific weakness is CWE-532, a secret
written to a log file; CWE-209 covers the message generation on the response side.

**There are two emitters, not one.** `From<JsonRejection>` logs the diagnostic explicitly at
`src/errors.rs:406`, and it also stores it under `details.error` via `AppError::malformed_json`
(`:109-119`). `AppError::into_response` then logs `details = ?self.details` for *every* client error
(`:380-385`). Sanitizing only the first emitter leaves the second one writing the same value, so the
credential scenario would still fail. The four already-enveloped endpoints log the parser message this
way today.

Chosen: one mechanism covering both emitters. `AppError` gains a set of detail keys that are
response-only. `malformed_json` marks its `error` key as one; `into_response` logs `details` with those
keys removed, and `From<JsonRejection>` drops its explicit `warn!` entirely. The wire payload is
untouched, so `details.error` still reaches the caller as §10.1 requires, and `details.reason` still
reaches the log because classification is exactly what a log should carry.

Key-level exclusion rather than a blunt "never log details" flag, because `reason` is the useful half
and dropping it would cost real diagnostics for no safety gain. Marking at construction rather than
filtering by key name at the log site, so a future constructor that embeds request-derived text opts in
explicitly instead of relying on someone remembering that `error` is the dangerous key.

Demoting the record to `TRACE` — which is what axum itself does, and which ASP.NET Core mirrors at
`Debug` — was considered and rejected as insufficient on its own. A deployment can enable a lower
level and collectors gather every level, so the accepted guidance is that the message must be safe
before it reaches any logger, and that authentication endpoints suppress body detail outright. Level
choice is a fallback, not the fix. This service's `WARN` is in any case out of step with every
framework surveyed.

`details.error` on the **response** is untouched. It is published contract, it is returned only to the
caller who sent the body, and removing it would break §10.1. The asymmetry is deliberate: the body's
author may see the parser's complaint about their own payload; the operator's log does not need a copy.

### Extraction sits inside the auth middleware, and the contract says so

`require_auth` is layered over the whole API router (`src/api.rs:291-295`) and returns before
`next.run` for every rejection path: `401` for absent or invalid credentials
(`src/middleware.rs:206,234,239`), `403` for a failed origin check (`:178,188,216`) and `403` for an
auth-managed route while authentication is disabled (`:173`). Extraction happens strictly inside that,
so a request the middleware turns away never reaches `Json<T>` or `Path<T>` at all.

An earlier draft of this design ignored this, and the spec inherited a scenario asserting that all
nineteen endpoints answer `400` to a malformed body. That is false for the running service: with no
credentials the answer is `401`, and frozen §11 says it should be. The spec now opens with an
admission requirement making §11's precedence explicit, and the coverage test supplies credentials and
an acceptable origin so it measures the extractor rather than the middleware.

Two consequences worth stating rather than discovering later:

- **`POST /api/auth/login` and `POST /api/auth/setup` are origin-checked even though they are
  auth-exempt — but only when no `Authorization` header is present** (`src/middleware.rs:182-190`,
  exempt list at `:40-45`). A bearer-token request skips that check by design, which frozen §11
  intends. So the coverage test must send either an acceptable origin or a bearer token for these two,
  and under `LABELER_NO_AUTH=true` the auth-managed paths answer `403` regardless of body (`:171-174`).
- **"Before any side effect" was too strong.** Bearer authentication calls `lookup_token`
  (`src/middleware.rs:198-205`) before extraction, so an authenticated request has already touched the
  store by the time a body is rejected. The accurate statement is that rejection precedes the
  *handler*, not that it precedes all work.

### The four correct sites collapse onto the same extractor

`PUT /api/templates/{id}/group`, `POST /api/batch`, `POST /api/print` and `POST /api/render/label`
keep `Result<Json<T>, JsonRejection>` + `payload.map_err(AppError::from)?` today. Leaving them alone
would work and would be a smaller diff.

Chosen: convert them too, dropping four `map_err` lines and the `JsonRejection` import from
`src/api.rs`. Two correct spellings of the same thing is how the third, wrong spelling gets written
next. Their behavior is unchanged by construction — both routes end at `From<JsonRejection>` — and
the test matrix covers them alongside the other fifteen precisely so "unchanged" is a measurement.

### `From<PathRejection>` defers to the framework's own client/server split

The impl currently discards the rejection and answers `400` for every variant. Once `Path<T>` is
wired up this becomes reachable, so the split has to be right.

**The first draft of this design got the semantics backwards and is corrected here.** It claimed
`PathRejection::MissingPathParams` means the route declares different parameters than the handler
destructures. It does not. In axum 0.8.9 `MissingPathParams` means axum's internal path-parameter
extension is absent, "commonly caused by extracting `Request<_>`" before `Path`
(`axum-0.8.9/src/extract/rejection.rs:50-56`, emitted at `src/extract/path/mod.rs:177`). A
route/handler arity mismatch is `ErrorKind::WrongNumberOfParameters`, which is wrapped in
**`FailedToDeserializePathParams`** — the same variant that carries ordinary bad-input errors — and
axum gives it status `500` (`src/extract/path/mod.rs:436-448`). So the first draft's rule, "all
`FailedToDeserializePathParams` become `400`", would have converted an existing `500` into a `400`
and violated this change's own routing-defect requirement.

Chosen: **do not match on variants at all; branch on the status axum already assigns.**
`PathRejection` is declared by `composite_rejection!`
(`axum-0.8.9/src/extract/rejection.rs:151`), exactly like `JsonRejection`, so it exposes `status()`.
`From<JsonRejection>` already uses precisely this trick for the payload-limit case
(`src/errors.rs:402`), so the pattern is established in this file rather than invented for it:

- `rejection.status().is_server_error()` → `AppError::internal` (`500`, code `Internal`).
- otherwise → `400 InvalidRequest` / `path_param_invalid`.

This is strictly better than enumerating variants. `ErrorKind` is not part of axum's public matching
surface, `PathRejection` is `#[non_exhaustive]`, and axum's own client/server judgement is the thing
the spec requirement actually names. A future axum that reclassifies a kind carries this along
automatically, and nothing here can downgrade a `5xx` to a `4xx`.

### The `Path` half is reachable today, not only future-proofing

An earlier draft of this design claimed all twenty-four `Path<String>` sites are effectively
infallible, because a `String` accepts any segment. That is wrong too, and in a way that matters:
when the router percent-decodes a segment to bytes that are not valid UTF-8 it stores
`UrlParams::InvalidUtf8InPathParam`, and `Path` turns that into `FailedToDeserializePathParams` with
`ErrorKind::InvalidUtf8InPathParam` (`axum-0.8.9/src/extract/path/mod.rs:168-175`), which axum
statuses `400` (`:436-448`).

So `GET /api/templates/%FF/source` is a live `400` on a route that exists today, returning axum's
plain text now and the envelope after this change. The spec's path scenarios are testable against the
current router rather than against a hypothetical twenty-fifth handler, and the test matrix uses that
request as the concrete case.

The forward-looking argument still holds and is still worth stating: `path_param_invalid` is published
contract in §10.1 that no code path can currently produce, and the first handler to take a
`Path<u32>` would otherwise return plain text. But the change is not speculative.

### OpenAPI schema generation is unaffected

The twenty-two `#[utoipa::path]` blocks declare `request_body` explicitly rather than inferring it
from the extractor type, so swapping `axum::extract::{Json, Path}` for `crate::extract::{Json, Path}`
is invisible to utoipa and leaves the generated OpenAPI document unchanged.

### ADR-0075

`docs/adr/0075-request-rejections-use-the-error-envelope.md`, plus its row in `docs/adr/README.md`.

Numbering as of this writing: `0067` is an unused gap, `0071` is on `main` (`9200835`), `0070` is
claimed by `issue-212`, and `0072`, `0073` and `0074` are claimed by `issue-226`.

**This number is racy and must be re-checked when the file is created.** An earlier draft of this
design claimed `0074` after checking every worktree, and that check was correct when it was made;
`issue-226` then added its own `0074` claim while this change was still in review, invalidating it.
Concurrent worktrees make ADR numbering a first-writer-wins race that no amount of checking at plan
time can settle. The implementation step therefore re-runs the scan across `main` and every worktree
immediately before writing the file, and treats the number here as a starting guess rather than an
allocation.

## Risks / Trade-offs

- **A future handler explicitly imports `axum::Json` and silently regresses.** → Not prevented, and
  the spec no longer claims otherwise. Import shadowing makes the correct spelling the default and the
  regression an explicit act visible in review. No stronger mechanism exists; three were tried and are
  recorded in Decisions so the next person does not retry them. Residual risk accepted, and it is the
  same risk every axum codebase carries.
- **The `422` → `400` move breaks an external client matching on status.** → Verified to break no
  branch in `ui/src` (see `proposal.md` — Impact). For any client outside this repo it is a genuine
  break, which is why the proposal marks it **BREAKING** and the ADR records it. The alternative,
  keeping `422`, means the fifteen endpoints stay inconsistent with §10 and with the four correct
  ones, which is the defect.
- **The `issue-197` branch has a test asserting the behavior this change deletes.** → Named in
  `proposal.md` — Impact with its exact location and content. Whichever branch merges second updates
  it. This is a merge-order hazard, not a design flaw, and it is cheap once expected.
- **Shadowing `Path` while `std::path::Path` is in the same crate.** → Verified: `src/api.rs` uses
  `std::path::Path` only fully-qualified (`:113`, `:399`, `:423`) and imports only `PathBuf` (`:11`).
  `cargo check` settles it definitively at implementation time.
- **`IntoResponse` on the wrapper drifts from `axum::Json`'s.** → It delegates rather than
  re-implements, so there is nothing to drift. The nine response-position uses are the test: if the
  delegation were wrong, their existing response assertions would fail.
- **The new extractor changes the payload-limit or content-type behavior by accident.** → These are
  the two cases most easily lost when funnelling everything through one path, which is why both have
  their own spec scenario and their own row in the test matrix rather than being left implicit. The
  content-type scenarios pin `application/problem+json` as *accepted*, which is what axum actually
  does (`axum-0.8.9/src/json.rs:151-152`) and what an earlier draft of the spec got wrong by
  requiring `application/json` exactly.
- **Branching on `rejection.status()` couples this service to axum's classification.** → Deliberate.
  The spec requirement is phrased as "never downgrade what the framework classifies as a server
  error", so the coupling is the contract rather than an accident of implementation. The alternative,
  enumerating `ErrorKind`s, couples to a *private* surface and silently rots when axum adds one.

## Migration Plan

None. The service is stateless, holds no persisted representation of an error, and the change adds no
configuration, no schema and no dependency. Deployment is the ordinary one; rollback is reverting the
commit, after which the fifteen endpoints return plain text again. No data is written in either
direction, so rollback is clean at any point.

## Open Questions

None. The judgement calls raised in review are closed in Decisions: enforcement is convention plus a
distinct type, after three stronger mechanisms were tried and shown not to exist; the wrong-shape body
keeps `json_malformed` under a widened published definition rather than taking a new reason slug; the
path conversion defers to the framework's own client/server classification rather than enumerating
rejection variants; and the rejection log drops the parser message rather than being demoted.
