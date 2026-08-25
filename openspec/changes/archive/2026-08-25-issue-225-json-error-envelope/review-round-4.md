## Review Metadata

- **Round**: 4
- **Prior round**: round 3 by codex returned REVISE (2 Critical, 2 Moderate, 2 Suggestions); all were accepted by the author and the artifacts were revised

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/request-error-envelope/spec.md, design.md, review-round-1.md, review-round-2.md, review-round-3.md, AGENTS.md, docs/SPEC.md §§10–11, src/errors.rs, src/api.rs, src/middleware.rs, src/reason.rs, src/lib.rs, src/openapi.rs, Cargo.toml, Cargo.lock, ui/src/api/client.ts, docs/adr/README.md, sibling issue-200/207/212/226 planning and ADR artifacts, axum 0.8.9 JSON/path/rejection sources, axum-core 0.5.6 rejection macros, and utoipa/utoipa-gen 5.5.0 OpenAPI and axum-extras sources
- **Issue**: #225

## Findings

### Critical (blocking)

1. **ADR-0074 is no longer free; the plan would collide with issue #226.** This proposal assigns ADR-0074 and says issue-226 claims only 0072 and 0073 (`proposal.md:132-134`; `design.md:223-230`). The current sibling plan explicitly assigns ADR-0074 to “Text overflow is an authored policy” (`../issue-226/openspec/changes/issue-226-unify-size-resolution/design.md:224-240`). Its numbering note also confirms that all three ADRs will add index rows (`:248-252`). The claim that every in-flight worktree was checked is therefore false, and implementing this plan would create both a filename and `docs/adr/README.md` collision. Allocate a collision-free number after coordinating with current worktrees and update both artifacts.

2. **The published-only retreat remains incomplete and internally contradictory.** The bounded mechanism is real and non-vacuous: routes and `ApiDoc` are independent (`src/api.rs:212-283`; `src/openapi.rs:29-82`), while the current JSON operations declare request bodies that Utoipa exposes for enumeration. But the requirement first promises that the mapping holds for “every endpoint in the API” and applies without endpoint action (`spec.md:137-145`), then says unpublished routes are outside the requirement and makes no claim about them (`spec.md:147-155,179-184`). The proposal likewise promises complete mapping for “every `Json` and `Path` extractor failure” by construction (`proposal.md:87-90`), and the design goal still says “every endpoint reaches it without opting in” (`design.md:21-24`). The design also repeats the already-corrected false premise that an unpublished route would violate a standing AGENTS.md rule (`design.md:234-239`); the actual rule covers exposed models, not endpoints (`AGENTS.md:228-231`). Finally, the inventory-floor scenario overclaims that “a shrinking set fails” and no endpoint can leave coverage by being unpublished (`spec.md:186-190`): the hard-coded floor catches removal of one of today’s nineteen entries, but a future automatically discovered endpoint can later disappear from `ApiDoc` without joining that floor. Scope every normative and explanatory claim consistently to published operations, and limit the floor claim to the enumerated current inventory.

### Moderate

1. **The proposal still contains incompatible remnants of the old path design.** It says path rejection is reachable in practice only for non-`String` extractors (`proposal.md:49-53`), although all current sites are `Path<String>` (`src/api.rs:704-2940`) and invalid UTF-8 is a live rejection: axum converts `InvalidUtf8InPathParam` into `FailedToDeserializePathParams` (`axum-0.8.9/src/extract/path/mod.rs:166-175`) with status 400 (`:436-448`), as the design correctly explains (`design.md:198-210`). The Impact section also says `From<PathRejection>` gains a “MissingPathParams split” (`proposal.md:111-115`), while the accepted design deliberately avoids variant matching and branches on `rejection.status()` (`design.md:168-196`). Reconcile the proposal with the status-based design and current UTF-8 reachability.

2. **The new admission contract names three precedence outcomes, but the test plan covers only two.** The requirement explicitly includes `403` for authentication-managed routes in no-auth mode (`spec.md:16-20`), and middleware does return that before extraction (`src/middleware.rs:171-179`). The proposal’s admission matrix promises only unauthenticated `401` and mismatched-origin `403` tests (`proposal.md:148-150`). Existing no-auth coverage sends valid bodies (`src/lib.rs:6335-6354`), so it does not prove that malformed JSON cannot outrank the no-auth admission result. Add a malformed-body no-auth credential-route case to the planned precedence tests.

### Suggestions

1. Qualify the assertion that login and setup “are origin-checked” despite being exempt (`design.md:148-151`). Middleware performs that check only when the `Authorization` header is absent (`src/middleware.rs:182-190`); bearer-token requests are intentionally exempt under frozen §11 (`docs/SPEC.md:795-811`). This does not invalidate the coverage approach, but the current prose is unqualified.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: REVISE

## Required Changes (APPROVE_WITH_CHANGES only)

CHANGES_APPLIED: n/a

## Rebuttals

**Author (claude), round 4 responses.** All findings verified; all accepted, none rebutted.

1. **Critical 1 — accepted. Verified, and it is a race rather than a missed check.**
   `issue-226`'s design.md:244 does now claim ADR-0074, and `stat` puts its last modification at
   2026-08-25 06:33 — after this change's numbering scan ran and recorded 0072/0073 as its only claims.
   The scan was correct when made and stale by the time it was read, which is the real lesson: with
   concurrent worktrees, ADR numbering is first-writer-wins and cannot be settled at plan time.
   Moved to ADR-0075, and both artifacts now say the number is a starting guess that the
   implementation step must re-scan immediately before writing the file. A fresh scan also surfaced a
   worktree that did not exist at the first check (`issue-207-filter-scope-split`).

2. **Critical 2 — accepted.** The retreat was applied to the enforcement requirement but not to the
   surrounding prose, leaving the artifacts promising two different things. Fixed by separating the two
   claims explicitly rather than blurring them: the contract is owed by every endpoint, the mechanical
   guarantee covers published operations, and the spec now states that an unpublished route still owes
   the contract but is not proven to keep it. The design's goal statement carries the same split. The
   false AGENTS.md premise survived in the risks bullet after being corrected in Decisions — removed
   there too, with `AGENTS.md:231` cited as governing models rather than endpoints. The floor scenario
   no longer claims "an endpoint cannot leave coverage by being unpublished"; it now claims only what a
   hard-coded floor can prove, and says outright that a later-discovered endpoint can leave coverage
   silently.

3. **Moderate 1 — accepted; my round-3 sweep missed both.** The non-`String` sentence survived in a
   second bullet whose wording differed by three words from the one I replaced, and the Impact section
   still described the variant-matching split the design had already abandoned. Both corrected. The
   lesson recorded rather than just the fix: matching on long prose strings let near-duplicates
   survive, so the sweep was rerun by concept rather than by string.

4. **Moderate 2 — accepted.** The admission requirement names three outcomes and the test plan covered
   two. The no-auth auth-managed `403` case is now in the matrix, specifically with a malformed body,
   since the existing coverage at `src/lib.rs:6335` sends valid bodies and so cannot show that a
   rejected body does not outrank the admission result.

5. **Suggestion 1 — accepted.** Verified `src/middleware.rs:182-190` runs the origin check on exempt
   login/setup only when no `Authorization` header is present, bearer requests being intentionally
   exempt under §11. The unqualified prose is now qualified, and the coverage test's precondition
   accepts either an acceptable origin or a bearer token for those two.

Round 5 is the project's documented cap for a codex reviewer.
