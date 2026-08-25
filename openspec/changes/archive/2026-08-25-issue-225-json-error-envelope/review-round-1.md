## Review Metadata

- **Round**: 1
- **Prior round**: none

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/request-error-envelope/spec.md, design.md, AGENTS.md, docs/SPEC.md §§10–10.1, src/errors.rs, src/api.rs, src/reason.rs, src/lib.rs, src/openapi.rs, src/middleware.rs, Cargo.toml, Cargo.lock, ui/src/api/client.ts, docs/adr/README.md, axum 0.8.9 json/rejection/path sources, axum-core 0.5.6 rejection source, and referenced issue-197/212/226 artifacts
- **Issue**: #225

## Findings

### Critical (blocking)

1. **The proposed `PathRejection` split misidentifies routing defects and would turn some existing framework `500`s into `400 path_param_invalid`.** The design says `MissingPathParams` means the route and handler declare different parameters and maps all `FailedToDeserializePathParams` cases to 400 (`design.md:100-112`). Axum 0.8.9 instead defines `MissingPathParams` as the internal path-parameter extension being absent, commonly because `Request<_>` was extracted before `Path` (`axum-0.8.9/src/extract/rejection.rs:50-56`), and emits it only when that extension is absent (`axum-0.8.9/src/extract/path/mod.rs:166-178`). A route/handler arity mismatch becomes `ErrorKind::WrongNumberOfParameters` (`axum-0.8.9/src/extract/path/de.rs:28-31,153-160`), wrapped in `FailedToDeserializePathParams` (`axum-0.8.9/src/extract/path/mod.rs:181-187`); axum assigns that and `UnsupportedType` status 500 (`axum-0.8.9/src/extract/path/mod.rs:436-448`). Consequently, the planned match would violate the spec’s own routing-defect requirement (`spec.md:127-142`). Revise the design to inspect the inner `ErrorKind` or otherwise preserve server-error status, and require endpoint-level tests for an invalid numeric segment, an arity mismatch, and missing path extensions. Unit tests that merely construct the two outer variants (`proposal.md:103-105`) cannot prove the specified split.

2. **The design explicitly does not satisfy the core “without opt-in” requirement.** The spec says a new JSON endpoint is covered by default and “SHALL NOT be able” to emit a framework rejection through omission (`spec.md:75-82`). Import shadowing only makes the wrapper conventional; the design acknowledges that explicitly importing `axum::Json` silently regresses behavior and accepts that residual risk (`design.md:64-73,147-152`). It also rejects enforcement because the delegate needs the framework type (`design.md:75-87`), using the project’s prohibition on lint suppression to abandon enforcement rather than choose another mechanically enforced guard. The fixed list of nineteen endpoints (`spec.md:84-94`) can prove today’s inventory but goes stale when a twentieth is added. Either add a source-wide compile/lint/test guard that permits the framework types only inside `src/extract.rs`, or weaken the requirement and proposal so they state the actual convention and residual risk.

3. **The ADDED delta silently expands the frozen §10 code table while claiming that table is untouched.** Project rules require first-touch requirements to name the frozen section they supersede (`AGENTS.md:19-27`). The proposal says §10’s code table remains authoritative and untouched (`proposal.md:79-83`), and the spec repeats that claim (`spec.md:17-20`), yet the new routing-defect requirement exposes `code: Internal` (`spec.md:127-142`). `Internal` is absent from the authoritative code table (`docs/SPEC.md:678-695`), a conflict the design itself acknowledges (`design.md:114-117`). Explicitly name and supersede the affected part of §10 and state its complete post-change contract.

### Moderate

1. **The plan promotes an existing reason-classification bug into contract without a defensible semantic decision.** The authoritative definition says `json_malformed` means the body is not parseable JSON (`docs/SPEC.md:734`), while the delta assigns it to syntactically valid JSON with a schema/type mismatch or missing key (`spec.md:26-27,46-60`). The current converter does conflate `JsonSyntaxError` and `JsonDataError` (`src/errors.rs:400-418`), but that proves current implementation, not correctness; axum itself distinguishes syntactically valid data errors from syntax errors (`axum-0.8.9/src/extract/rejection.rs:9-29`). The proposal’s claim that this converter is already “correct” (`proposal.md:12-15`) is therefore unsupported. Introduce a distinct stable reason such as `json_data_invalid`/`json_schema_invalid`, including its ADR-0052 contract cost, or explicitly justify and record the breaking semantic broadening of `json_malformed`.

2. **The Content-Type contract disagrees with the delegated axum extractor.** The table requires 415 whenever the media type is not `application/json` (`spec.md:24-31`), but axum accepts both `application/json` and any `application/*+json` media type (`axum-0.8.9/src/json.rs:138-154`). Because the wrapper delegates directly to axum (`design.md:54-57`), a request using `application/problem+json` would be accepted contrary to the requirement. Define the accepted JSON media types accurately and add a vendor-`+json` scenario.

### Suggestions

none

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: REVISE

## Required Changes (APPROVE_WITH_CHANGES only)

CHANGES_APPLIED: n/a

## Rebuttals


**Author (claude), round 1 responses.** Every finding was verified against
`~/.cargo/registry/.../axum-0.8.9/` before being accepted. All five accepted; none rebutted.

1. **Critical 1 — accepted, and the defect was worse than described.** Confirmed
   `MissingPathParams` is the absent-`UrlParams`-extension case (`axum-0.8.9/src/extract/rejection.rs:50-56`,
   emitted at `src/extract/path/mod.rs:177`), not an arity mismatch; confirmed `WrongNumberOfParameters`
   and `UnsupportedType` ride inside `FailedToDeserializePathParams` and are statused `500`
   (`src/extract/path/mod.rs:436-448`). The design's semantics were inverted, so its rule would have
   downgraded existing `500`s. Fixed by not matching variants at all: `PathRejection` is a
   `composite_rejection!` (`rejection.rs:151`) so it exposes `status()`, and the conversion now branches on
   `is_server_error()`, mirroring the `PAYLOAD_TOO_LARGE` check already at `src/errors.rs:402`.
   Also corrected a second, related error the finding did not name: the claim that `Path<String>` is
   effectively infallible. `UrlParams::InvalidUtf8InPathParam` (`path/mod.rs:168-175`) makes
   `GET /api/templates/%FF/source` a live `400` on a route that exists today, so the path scenarios are
   now testable against the current router. Endpoint-level tests for invalid UTF-8, a non-parsing typed
   segment and a server-classified rejection replace the variant-construction unit tests.

2. **Critical 2 — accepted.** The requirement was writing a cheque the design did not cash. Enforcement
   is now a source-guard unit test asserting no file but `src/extract.rs` names the framework's `Json`,
   `Path` or `JsonRejection`. It fails `cargo test`, needs no `clippy.toml`, no `#[allow]` and no new
   dependency, so it does not collide with AGENTS.md's lint-suppression prohibition. A spec scenario
   now states the build fails. The nineteen-endpoint enumeration is kept as today's inventory proof, no
   longer as the coverage mechanism.

3. **Critical 3 — accepted.** The delta now names `docs/SPEC.md` §10's code table and supersedes it for
   the addition of an `Internal` (500) row only, stating that row; the "table untouched" claim is gone
   from both proposal.md and spec.md.

4. **Moderate 1 — accepted; resolved by decision, not by a new slug.** The finding is correct that the
   converter's conflation proves current implementation, not correctness, and the proposal's word
   "correct" was unsupported. Put to the project owner as an explicit fork (new `json_data_invalid`
   slug vs. widening `json_malformed`), who chose to keep one slug and widen its published definition.
   The delta now supersedes §10.1's `json_malformed` row with "could not be deserialized into the
   endpoint's type" and records that `details.error` is the only thing separating the two cases. This
   keeps the four already-correct endpoints byte-identical and adds no ADR-0052 vocabulary.

5. **Moderate 2 — accepted.** Confirmed `json_content_type` accepts `application/json` and any
   `application/*+json` (`axum-0.8.9/src/json.rs:151-152`). The 415 row now reads "absent, unparseable,
   or not a JSON media type", the accepted set is defined, and scenarios pin `application/problem+json`
   as accepted and `text/plain` as rejected.

Verdict was REVISE, so this is not a re-check: a full round 2 runs in a fresh context.
