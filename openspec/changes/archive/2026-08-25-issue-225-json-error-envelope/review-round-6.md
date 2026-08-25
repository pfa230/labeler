## Review Metadata

- **Round**: 6
- **Prior round**: round 5 returned APPROVE_WITH_CHANGES; all changes were applied and re-checked ALL_APPLIED, then the author deliberately VOIDED that verdict by simplifying the artifacts

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/request-error-envelope/spec.md, design.md, tasks.md, review-round-1.md through review-round-5.md, review.md, AGENTS.md, docs/SPEC.md §§10–11, src/errors.rs, src/api.rs, src/reason.rs, src/lib.rs, src/openapi.rs, src/middleware.rs, src/main.rs, Cargo.toml, Cargo.lock, ui/src/api/client.ts, docs/adr/README.md, sibling-worktree ADR plans, axum 0.8.9 JSON/path/rejection/router sources, axum-core 0.5.6 rejection macros, and serde_json 1.0.151 tests
- **Issue**: #225

## Findings

### Critical (blocking)

none

### Moderate

1. **The logging design sanitizes only the first of two log events, so implementing task 3.1 as written still violates the new security requirement.** `From<JsonRejection>` obtains the parser diagnostic and stores it under `details.error` through `AppError::malformed_json` (`src/errors.rs:405-412`; `src/errors.rs:109-119`). Independently of the explicit warning at `src/errors.rs:406`, `AppError::into_response` logs the entire `self.details` value for every client error (`src/errors.rs:368-396`, especially `:380-385`). Therefore removing or sanitizing only the warning at `:406`, as specified by `design.md:108-138` and `tasks.md:24-30`, still emits the diagnostic—including `12345` from the required credential scenario—through the generic `"request rejected"` record. This contradicts the requirement that no rejection log contain the body or parser diagnostic (`spec.md:169-189`). Task 3.2 would expose the defect if it captures all records, but the planned production edit must explicitly address both emitters rather than leave the test to discover an unstated implementation change.

### Suggestions

1. Removing the OpenAPI-derived test is honest about the absence of a structural guarantee, but it does lose useful defense-in-depth. The design concedes that derivation from `ApiDoc` is workable (`design.md:88-90`), while the replacement inventory is a fixed nineteen-endpoint list (`spec.md:150-161`) and the source check only pins the central imports (`tasks.md:69-72`). Neither automatically detects a newly published endpoint that explicitly names `axum::Json`. Retaining the derived test without claiming completeness would cover that narrower regression, though it is not required for the now-explicit convention-based contract.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Revise the proposal, design, and task 3.1 to account for `AppError::into_response` logging `details.error`. Specify a single coherent sanitization strategy ensuring every log record for a JSON rejection contains only safe classification/status information while the response still carries `details.error`; keep task 3.2 capturing the complete request trace so it verifies all emitted records.

CHANGES_APPLIED: yes

## Rebuttals

**Author (claude), round 6 responses.**

**Required Change 1 — accepted, verified, applied.** The finding is correct and my fix was incomplete.
Verified `AppError::into_response` logs `details = ?self.details` at WARN for every client error
(`src/errors.rs:380-385`), and `malformed_json` stores the parser message under `details.error`
(`:109-119`). Sanitizing only the explicit `warn!` at `:406` would have left the second emitter writing
`12345` from the very scenario the new requirement adds. I had found one emitter and stopped looking.

Applied as one mechanism rather than two patches: `AppError` gains a set of response-only detail keys,
`malformed_json` marks its `error` key, `into_response` omits marked keys from the log, and the
converter drops its own `warn!`. `details.error` still reaches the caller as §10.1 requires;
`details.reason` still reaches the log, because classification is what a log should carry. Key-level
exclusion rather than a blunt "never log details" flag so `reason` survives; marked at construction
rather than filtered by key name at the log site so a future constructor embedding request-derived text
opts in explicitly. `proposal.md`, `design.md` and tasks 3.1/3.2 all state this; 3.2 now captures every
record for the request, not just the converter's. `specs/` is unchanged: the requirement was already
correct, and editing it would widen beyond the Required Change.

Worth noting the blast radius: the four already-enveloped endpoints log the parser message through
`into_response` today, so this closes a live leak as well as preventing the new one.

**Suggestion 1 — declined, with reasoning.** Keeping the `ApiDoc`-derived test as unclaimed
defense-in-depth would catch a newly published endpoint naming `axum::Json`. I am leaving it out: that
test is what produced four re-checks of a single item and the published-versus-owed apparatus, and
reintroducing it without a requirement to anchor it invites the same drift back into the artifacts. The
narrower regression it covers is real but small, and the change is already carrying a security fix that
matters more. If it is wanted later it is additive and needs no spec change. Declining a Suggestion is
the author's call; flagging it here rather than silently dropping it.

**Re-check (codex, read-only): RECHECK_RESULT: ALL_APPLIED.** The single Required Change accepted;
the Suggestion 1 declination judged defensible. Verdict APPROVE_WITH_CHANGES discharged.
