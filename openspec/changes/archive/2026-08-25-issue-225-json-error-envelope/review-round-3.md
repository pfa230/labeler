## Review Metadata

- **Round**: 3
- **Prior round**: round 2 by codex returned REVISE (1 Critical, 1 Moderate, 3 Suggestions); all were accepted by the author and the artifacts were revised

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/request-error-envelope/spec.md, design.md, review-round-1.md, review-round-2.md, AGENTS.md, docs/SPEC.md §§10–10.1 and §11, src/errors.rs, src/api.rs, src/reason.rs, src/lib.rs, src/openapi.rs, src/middleware.rs, src/store.rs, Cargo.toml, Cargo.lock, ui/src/api/client.ts, docs/adr/README.md, openspec/specs/ui-colour-palette/spec.md, axum 0.8.9 JSON/path/rejection/router sources, axum-core 0.5.6 rejection macros, and utoipa 5.5.0 OpenAPI/path/request-body sources
- **Issue**: #225

## Findings

### Critical (blocking)

1. **The OpenAPI-derived test is viable for registered operations, but still does not enforce the claimed “without per-endpoint opt-in” guarantee because its completeness premise is false.** Utoipa 5 does expose the necessary runtime data: `OpenApi.paths.paths`, each `PathItem` method, `Operation.request_body`, and `RequestBody.content` are public (`utoipa-5.5.0/src/openapi/path.rs:38-42,253-283,491-493`; `openapi/request_body.rs:21-28`), so registered JSON operations can be enumerated and their brace-style paths can be routed after applying the `/api` server prefix (`src/openapi.rs:29-82`; `src/api.rs:212-278`). The unsound part is completeness: routes and OpenAPI operations are maintained independently (`src/api.rs:212-283`; `src/openapi.rs:29-80`), and contrary to the design’s premise, `AGENTS.md` requires every exposed **model**, not every endpoint, to be registered (`AGENTS.md:231`). A developer can therefore add a route using `axum::Json` without adding it to `ApiDoc`; the derived set does not shrink, the nineteen-entry floor still passes, and the regression ships undetected. This contradicts the requirement that every new JSON endpoint is covered automatically and “cannot ship undetected” (`spec.md:100-111,125-138`) and the design’s false claim that omission violates a standing project rule (`design.md:110-119,198-203`; `proposal.md:60-65`). Either add mechanical route-to-OpenAPI completeness enforcement or narrow the normative guarantee everywhere to registered/published operations and stop claiming all API endpoints are protected.

2. **The request-body contract ignores authentication and CSRF precedence, so its universal `400` scenario is false for the actual application, especially on the auth endpoints.** The spec says every JSON endpoint returns the body-rejection mapping and that sending malformed JSON to all nineteen endpoints yields `400` (`spec.md:12-19,98-123`), without requiring authentication or a valid origin. Frozen §11 remains authoritative and requires `401` for unauthenticated protected routes and `403` for failed origin checks (`docs/SPEC.md:790-811`). The router runs `require_auth` outside every API handler (`src/api.rs:290-295`); that middleware returns `401`/`403` before `next.run` for protected routes, no-auth credential-management routes, and origin-invalid login/setup requests (`src/middleware.rs:165-190,193-239`). A coverage test can obtain `400` by supplying suitable credentials and origin headers, but that precondition must be part of the contract and test design. The design’s stronger assertion that rejection occurs “before any side effect” is also false: bearer authentication calls `lookup_token` before extraction (`src/middleware.rs:193-205`), and a successful lookup may update `last_used_at` (`src/store.rs:610-625`). State explicitly that the extractor mapping applies only after outer authentication/origin middleware admits the request, preserve §11 precedence, and specify the authenticated test setup.

### Moderate

1. **The proposal still contains several mutually inconsistent remnants of the abandoned round-1/round-2 designs.** It says both wrappers implement `IntoResponse` through `axum::Json` (`proposal.md:24-31`), while the accepted design correctly limits that impl to `Json<T>` (`design.md:54-58`). It says current path failures are reachable only through non-`String` extractors (`proposal.md:48-52`), contradicting the verified invalid-UTF-8 `Path<String>` route (`design.md:162-174`; `src/api.rs:224,907-910`). Its test plan still calls for the abandoned source guard (`proposal.md:126-138`), although the design rejects that mechanism (`design.md:76-102`). These contradictions leave implementation tasks with multiple incompatible instructions and must be reconciled.

2. **The round-2 correction to the `json_malformed` wording was incomplete in the normative spec.** The spec still says “no endpoint changes the reason it reports” (`spec.md:21-27`), while the proposal accurately acknowledges that fifteen endpoints currently report no reason and will begin reporting `json_malformed` (`proposal.md:53-59`). Replace the spec’s overbroad sentence with the same four-versus-fifteen distinction.

### Suggestions

1. Correct the proposal’s count: it establishes fifteen bare extractors plus four already-enveloped extractors, so “nineteen of twenty-three sites” is wrong (`proposal.md:7,12-15`); fifteen of nineteen extraction sites currently miss the conversion.

2. Add `ui-colour-palette` to the proposal’s inventory of existing OpenSpec capabilities (`proposal.md:90-94`; `openspec/specs/ui-colour-palette/spec.md:1-9`).

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: REVISE

## Required Changes (APPROVE_WITH_CHANGES only)

CHANGES_APPLIED: n/a

## Rebuttals



**Author (claude), round 3 responses.** All findings verified against the source; all accepted, none
rebutted.

1. **Critical 1 — accepted. The premise was false and I misread the rule.** Verified `AGENTS.md:231`
   reads "Every **model** exposed in the API must be registered in `src/openapi.rs`" — model, not
   endpoint. Routes (`src/api.rs:212-283`) and `ApiDoc` (`src/openapi.rs:29-80`) are maintained
   independently, so an unpublished route evades the derived set without shrinking it. Rather than
   invent a fourth mechanism, the guarantee is now bounded to what the mechanism delivers: the spec
   scopes the requirement to published operations, adds a scenario stating that an unpublished route is
   explicitly outside it, and drops "cannot ship undetected". design.md records the misreading and why
   route-to-`ApiDoc` completeness is a separate concern (wider than JSON, wider than this error
   contract) rather than folding it in. proposal.md's enforcement bullet is corrected to match.

2. **Critical 2 — accepted, and it found a layer I had not considered at all.** Verified `require_auth`
   is layered over the entire API router (`src/api.rs:291-295`) and returns before `next.run` on every
   rejection path: `401` at `src/middleware.rs:206,234,239`, `403` at `:178,188,216`, and `403` for an
   auth-managed path under `no_auth` at `:173`. The spec's universal-`400` scenario was therefore false
   for the running service. Fixed by adding a leading requirement that states §11's precedence
   explicitly, with scenarios for the `401` and `403` outcomes, and by conditioning every body-rejection
   scenario on an admitted caller. Also verified the exempt list (`:40-45`) still origin-checks
   state-changing `login`/`setup` (`:183-190`), which the coverage test must account for.
   The "before any side effect" claim is withdrawn as the finding requires: bearer auth calls
   `lookup_token` (`:198-205`) before extraction, so the accurate statement is that rejection precedes
   the *handler*, not all work.

3. **Moderate 1 — accepted; this was drift I introduced by patching design and spec each round and
   letting proposal.md fall behind.** All three remnants removed: `IntoResponse` is now `Json<T>` only,
   the "non-`String` extractors only" sentence is replaced by the verified invalid-UTF-8 case, and the
   test plan calls for the OpenAPI-derived coverage test instead of the abandoned source guard. I then
   ran a cross-document sweep on every load-bearing claim to check the three files agree, rather than
   fixing only the three instances named.

4. **Moderate 2 — accepted.** The spec kept the overbroad "no endpoint changes the reason it reports"
   after proposal.md had been corrected. It now carries the same four-versus-fifteen distinction.

5. **Suggestion 1 — accepted.** "Nineteen of twenty-three sites" was wrong; corrected to fifteen of the
   nineteen JSON extraction sites.

6. **Suggestion 2 — accepted.** `ui-colour-palette` added to the capability inventory; it landed on
   `main` in `9200835` after this branch was first cut.

Verdict was REVISE, so this is not a re-check: a full round 4 runs in a fresh context.
