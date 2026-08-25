## Review Metadata

- **Round**: 5
- **Prior round**: round 4 by codex returned REVISE (2 Critical, 2 Moderate, 1 Suggestion); all were accepted by the author and the artifacts were revised

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/request-error-envelope/spec.md, design.md, review-round-1.md through review-round-4.md, AGENTS.md, docs/SPEC.md §§10–11, src/errors.rs, src/api.rs, src/reason.rs, src/lib.rs, src/openapi.rs, src/middleware.rs, src/main.rs, Cargo.toml, Cargo.lock, docs/DEPLOY.md, docs/adr/README.md, ui/src/api/client.ts, issue-197/src/lib.rs, axum 0.8.9 JSON/path/rejection sources, axum-core 0.5.6 rejection macros, utoipa-gen 5.5.0 axum inference sources, serde_json 1.0.151 error tests, and the current main/eight-worktree ADR inventory
- **Issue**: #225

## Findings

### Critical (blocking)

none

### Moderate

1. **The obligation/guarantee split is nearly coherent, but one normative scenario still exempts the routes the requirement says remain obligated.** The requirement first says every JSON endpoint owes the mapping and explicitly says an unpublished route still owes it even though enforcement cannot prove it (`spec.md:139-145`). Its unpublished-route scenario then says “this requirement makes no claim about it” (`spec.md:184-189`), which contradicts that obligation rather than merely bounding the test. Replace this with language saying the *mechanical guarantee/test* makes no claim about the route while the route still owes the mapping. The broader obligation remains useful once that distinction is stated consistently.

2. **Proposal and design still overstate the published-operation guarantee as covering path extraction.** The capability and goal claim that handlers reading either JSON bodies or path parameters receive the mapping by construction and that this is mechanically proven for published operations (`proposal.md:88-93`; `design.md:21-25`). The actual derived test selects only operations with JSON request bodies (`proposal.md:62-67`; `design.md:103-109`), and only the JSON requirement contains the published-operation enforcement contract (`spec.md:137-198`); the path requirements have no equivalent coverage mechanism (`spec.md:205-280`). Import shadowing and the planned path endpoint cases cover current code, but they do not mechanically detect a newly published handler that explicitly uses `axum::Path`. Narrow the mechanical-guarantee claims to published JSON-body operations, unless path-operation-derived coverage is deliberately added.

3. **Routing all auth-body failures through the existing converter promotes potentially secret-bearing parser text from framework TRACE logging to application WARN logging.** The plan sends all fifteen bare JSON sites through `From<JsonRejection>` (`proposal.md:24-35`), which logs the complete `body_text()` at WARN (`src/errors.rs:405-406`). That includes login/setup/user/password handlers whose models carry passwords (`src/api.rs:2474-2478,2545-2592,2736-2750,2817-2839`), and serde JSON diagnostics can reproduce an unexpected string verbatim (`serde_json-1.0.151/tests/test.rs:1221-1229`). Application WARN is enabled by the default filter (`src/main.rs:160-164`), whereas axum logs rejection bodies at TRACE (`axum-core-0.5.6/src/macros.rs:5-19`). The response does not teach an unauthenticated caller more than today’s plain-text rejection, but malformed credential payloads can newly persist in ordinary logs. Amend the “do not rewrite `From<JsonRejection>`” non-goal (`design.md:34-37`) to preserve its wire mapping while sanitizing its log event to record only classification/status, not the parser message.

### Suggestions

1. Correct the scope explanation that `POST /api/import/csv` “reads multipart” (`proposal.md:79-82`). It accepts a raw `String` documented as `text/csv` (`src/api.rs:2325-2349`); no multipart extractor exists in `src/`.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Change the unpublished-route scenario to distinguish the unproven enforcement guarantee from the still-applicable endpoint obligation.
2. Limit claims of OpenAPI-derived mechanical coverage to published JSON-body operations, or explicitly add equivalent derived path coverage.
3. Update the design to sanitize JSON-rejection logging while leaving the specified wire mapping unchanged.

CHANGES_APPLIED: yes

## Rebuttals

<!-- Leave empty. The author fills this in. -->

**Author (claude), round 5 responses.** All three Required Changes applied, plus the Suggestion.

1. **RC1 — applied.** `spec.md`'s unpublished-route scenario no longer says the requirement makes no
   claim about the route. It now says the route still owes the mapping exactly as a published one does,
   and that it is the *mechanical guarantee* which makes no claim about whether it keeps it. The
   scenario is retitled "obligated but not proven" to match.

2. **RC2 — applied.** Narrowed rather than extended: no path-operation-derived coverage is added.
   `spec.md` now scopes the enforcement contract to published operations **that declare a JSON request
   body**, and states that path extraction has no equivalent derived coverage and is held by the
   enumerated endpoint scenarios. `proposal.md`'s capability description and `design.md`'s goal carry
   the same narrowing, and the mechanism section in `design.md` says outright that a newly published
   handler using `axum::Path` is not detected by the derived test.

3. **RC3 — applied, and verified before applying.** Confirmed `src/errors.rs:405-406` logs the full
   `body_text()` at WARN; `Credentials { username, password }` (`src/api.rs:2475-2478`) and
   `PasswordChange { current_password, new_password }` (`:2818-2821`); default filter `labeler=info`
   (`src/main.rs:160-164`), so the line ships. The finding is right that this is a regression the fix
   would introduce rather than a pre-existing condition, because the endpoints that would newly log are
   precisely the credential-bearing ones. The non-goal is amended to cover the *mapping* only, and a
   new decision reduces the log event to classification and status. The wire mapping is untouched:
   `spec.md:83` still requires `details.error` to carry the parser message, on the reasoning that the
   body's author may see a complaint about their own payload while the operator's log needs no copy.

4. **Suggestion 1 — applied**, disclosed here because it is an edit beyond the Required Changes list.
   Verified: `POST /api/import/csv` takes a raw `text/csv` body (`src/api.rs:2334`) and no `Multipart`
   extractor exists anywhere in `src/`. The word "multipart" was wrong and is replaced. It is a
   one-clause factual correction to a scope aside, touching no requirement.

`openspec validate --strict` passes. Re-check is scoped to items 1-3 above, plus item 4 as disclosure.

**Author (claude), re-check response.** Item 2 was correctly rejected. I had narrowed the explanatory
paragraph but left the normative sentence and the bounding bullet saying "every operation published"
and "published operations only", which claims more than the derived set delivers. Now corrected in
three places: the SHALL sentence is scoped to operations the API description publishes as declaring a
JSON request body; the bounding bullets split the limit into its two real causes (unpublished routes,
and published operations without a declared JSON body) and add that path extraction has no derived
coverage at all; and the newly-published scenario keys on the declaration rather than on what the
handler reads. `design.md:297` carried the same loose phrasing and is aligned. Items 1, 3 and 4 were
accepted and are untouched.

**Author (claude), re-check 2 response.** Item 2 rejected again, again correctly. I had fixed
`spec.md` and one `design.md` line while `proposal.md:64-66` and `design.md:269-275` still described
coverage as applying to "published operations" and named the unpublished route as the only residual
hole. Twice now I have corrected the instances I searched for rather than every place the claim lives,
so this pass was done by auditing every line in all three files mentioning coverage, enforcement,
guarantee, `ApiDoc` or the derived set, rather than by grepping remembered phrasings.

Corrected: `proposal.md`'s enforcement bullet now names all three bounds (unpublished route; published
without a declared JSON body; path extraction). `design.md`'s risk bullet lists the same three instead
of one. `design.md:129-135` no longer says the spec scopes to "published operations" or speaks of a
single remaining hole. The requirement's own title was the last overclaim — "Every JSON endpoint is
covered, without per-endpoint opt-in" — and is now "Every JSON endpoint owes the mapping; published
JSON-body operations are proven to keep it", matching the obligation/guarantee split in its body.

`openspec validate --strict` passes. Items 1, 3 and 4 remain untouched.

**Author (claude), re-check 3 response.** Item 2 rejected a third time. The remaining instance was the
floor scenario's final bullet (`spec.md:205-206`), which said a later-discovered endpoint "is covered
while it is published" without the JSON-body condition, contradicting the bounded derived set fifty
lines above it. It now reads "covered only while the API description publishes it as declaring a JSON
request body, and silently leaves coverage if that declaration is later removed or the endpoint is
unpublished".

Worth recording that my re-check 2 audit did list this line and I read past it: the audit enumerated
the right lines but I checked them for the phrasing I had been fixing rather than against the bound
itself. The re-audit was therefore run as an explicit per-line verdict against the bound, which is
what surfaced it.

`openspec validate --strict` passes. Items 1, 3 and 4 remain untouched.

**Re-check 4 (codex, read-only): RECHECK_RESULT: ALL_APPLIED.** All three Required Changes accepted,
plus the disclosed Suggestion. Verdict APPROVE_WITH_CHANGES is discharged; `CHANGES_APPLIED` set above.

Re-check history for the record: item 2 took four passes. Each rejection was narrower than the last —
a whole-paragraph overclaim, then two files, then one bullet — and every one was correct. Items 1, 3
and 4 were accepted on the first pass and were untouched thereafter.
