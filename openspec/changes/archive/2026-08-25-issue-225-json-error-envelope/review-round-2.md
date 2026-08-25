## Review Metadata

- **Round**: 2
- **Prior round**: round 1 by codex returned REVISE (3 Critical, 2 Moderate); all five were accepted by the author and the artifacts were revised

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/request-error-envelope/spec.md, design.md, review-round-1.md, AGENTS.md, docs/SPEC.md §§10–10.1, docs/adr/README.md, src/errors.rs, src/api.rs, src/reason.rs, src/lib.rs, src/openapi.rs, src/middleware.rs, Cargo.toml, Cargo.lock, ui/src/api/client.ts, axum 0.8.9 rejection/path/router/json sources, axum-core 0.5.6 rejection macros, and utoipa-gen 5.5.0 axum argument inference
- **Issue**: #225

## Findings

### Critical (blocking)

1. **The round-1 source-guard fix still cannot satisfy its normative guarantee as designed.** The requirement says the check SHALL reject every use of axum’s JSON or path extractor outside the one service-extractor module and that such a handler makes the build fail (`spec.md:103-106,120-124`). The design specifies a text check for `axum::Json`, `axum::extract::Json`, `axum::extract::Path`, and `axum::extract::rejection::JsonRejection`, permitting those names only in `src/extract.rs` (`design.md:86-90`). That literal check misses the project’s present nested imports—`use axum::{ extract::rejection::JsonRejection, extract::{..., Json, Path, ...} }` (`src/api.rs:2-5`) contains none of those qualified strings—so the exact regression being removed can evade it without editing the guard. Conversely, an identifier-level check for `JsonRejection` outside `extract.rs` cannot pass the planned implementation: `src/errors.rs` necessarily names `JsonRejection` and `PathRejection` in its imports and `From` impls (`src/errors.rs:1-6,400-425`), and the design explicitly keeps those conversions there and changes the path conversion in place (`design.md:5-10,111-139`). The plan also says only that the unit test fails `cargo test` (`design.md:86-89`), while the normative scenario says the build fails. Revise the enforcement mechanism and its allowed locations so it both passes the planned code and catches nested/aliased framework-extractor imports, or weaken the absolute requirement to the guarantee a source-text guard can actually provide.

### Moderate

1. **The test plan covers only one of two materially distinct server-classified path failures specified as scenarios.** The specification separately requires a `500 Internal` for route/handler arity disagreement and for a missing router path-parameter extension (`spec.md:188-201`), but the proposal promises only one endpoint test for “a server-classified rejection” (`proposal.md:125-128`). These exercise different axum paths: arity mismatch is `FailedToDeserializePathParams` with `WrongNumberOfParameters`, statused 500 (`axum-0.8.9/src/extract/path/mod.rs:436-448`), while an absent extension is the separate `MissingPathParams` rejection (`axum-0.8.9/src/extract/rejection.rs:50-56`, `src/extract/path/mod.rs:166-178`). Both should be named in the test plan, particularly because confusing these paths caused round 1’s blocking defect.

### Suggestions

1. Qualify the claim that “no endpoint changes the reason it reports.” Fifteen bare-`Json` endpoints currently report no reason and will begin reporting `json_malformed` (`proposal.md:43-46`); only the four already-enveloped endpoints keep their existing reason. The broader wording appears in both `proposal.md:53-58` and `spec.md:24-27`.

2. Correct the Impact summary saying error responses change shape on fifteen endpoints (`proposal.md:109-110`). The same proposal establishes that all twenty-four path sites are converted (`proposal.md:104-108`) and that invalid UTF-8 makes the path-envelope change reachable today (`proposal.md:48-52`; `design.md:141-153`).

3. Only the `Json<T>` wrapper needs `IntoResponse`. The design proposes it for both wrappers (`design.md:54-57`), but all nine response-position uses are `Json<T>` and no handler returns `Path<T>` (`src/api.rs:387,888,1015,1061,1078,1096,1450,2401,2460`). Omitting `IntoResponse for Path<T>` keeps that type limited to its extractor purpose.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: REVISE

## Required Changes (APPROVE_WITH_CHANGES only)

CHANGES_APPLIED: n/a

## Rebuttals

**Author (claude), round 2 responses.** All findings verified against the source before being
accepted. All five accepted; none rebutted.

1. **Critical 1 — accepted, and confirmed worse than described.** Verified the guard as designed is
   blind to the codebase's own import style: `src/api.rs:2-8` nests the extractors
   (`use axum::{ extract::rejection::JsonRejection, extract::{..., Json, Path, ...} }`) and
   `rg "axum::extract::Json|axum::Json|axum::extract::Path" src/api.rs` returns nothing, so the exact
   regression evades a literal-path scan. The finding's second half is also right and I had it wrong in
   a way it did not name: the allowed-location list was incorrect, because `src/errors.rs:1-6` imports
   `axum::Json` itself for the error response body, not merely the rejection types. Tightening to
   identifier level cannot resolve `Json`/`Path` to a crate from text; a `clippy.toml` entry could, but
   `axum::Json` and `axum::extract::Json` are one type, so it would fire in `src/errors.rs` as well as
   `src/extract.rs` — two `#[allow]`s against an AGENTS.md prohibition.
   Fixed by abandoning source-text enforcement entirely for a behavioural one: a test derived from
   `ApiDoc::openapi()` that fires a malformed body at every operation declaring a JSON request body.
   Verified viable before adopting: `src/openapi.rs:29` registers all 51 operations, all fifteen
   bare-`Json` handlers declare `request_body`, and the three non-JSON bodies carry explicit
   `content_type` (`src/api.rs:642,692` `text/yaml`; `:2334` `text/csv`).
   The normative wording is corrected too, per the finding's closing option: the requirement no longer
   claims the build fails for any framework-extractor use. It now states a behavioural guarantee, and
   says explicitly that it does not make such a handler impossible to write, only impossible to ship
   unnoticed.

2. **Moderate 1 — accepted.** The test plan now names both server-classified cases separately —
   arity disagreement (`WrongNumberOfParameters` inside `FailedToDeserializePathParams`) and absent
   path parameters (`MissingPathParams`) — with a note that conflating these two is precisely what
   produced round 1's blocking defect.

3. **Suggestion 1 — accepted.** "No endpoint changes the reason it reports" was overbroad. Corrected:
   the four already-enveloped endpoints report the same reason before and after; the other fifteen
   report no reason today and begin reporting `json_malformed`.

4. **Suggestion 2 — accepted.** The Impact summary said fifteen endpoints change shape while the same
   document established twenty-four path sites also change and that invalid UTF-8 makes that reachable
   today. Corrected to state both, and that the four already-enveloped JSON endpoints are unchanged.

5. **Suggestion 3 — accepted.** Verified no handler returns `Path<T>`; all nine response-position uses
   are `Json<T>`. `IntoResponse` is now specified for `Json<T>` only.

Verdict was REVISE, so this is not a re-check: a full round 3 runs in a fresh context. The project's
two-consecutive-REVISE stop was explicitly lifted by one round by the project owner.
