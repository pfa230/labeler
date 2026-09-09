# Plan review

AUTHOR: claude
REVIEWER: codex
VERDICT: APPROVE_WITH_CHANGES
ROUNDS: 1

## Required changes

The author must apply these edits; **NO further review follows**.

1. **[P1] Limit the panic guarantee to the boundary the middleware actually covers.** [panic-envelope/spec.md:12](/home/pfa/projects/labeler/.worktrees/issue-248/openspec/changes/issue-248-catch-panics-and-remove-handler-unwraps/specs/panic-envelope/spec.md:12) promises a JSON response for every request panic, including middleware failures. [design.md:58](/home/pfa/projects/labeler/.worktrees/issue-248/openspec/changes/issue-248-catch-panics-and-remove-handler-unwraps/design.md:58) claims coverage of `TraceLayer` and `ServeDir`. However, `CatchPanicLayer` catches service invocation and response-future polling; it only boxes the returned body. A panic during subsequent body polling—including trace body callbacks—escapes, and a response already being transmitted cannot become a fresh JSON 500. This follows from the [upstream implementation, lines 202–216 and 269–279](https://docs.rs/tower-http/latest/src/tower_http/catch_panic.rs.html#202-279).

   **Required edit:** Treat the selected middleware as the intended boundary. Qualify the proposal, D1, and all panic-envelope guarantees as applying to unwinding panics propagated through the wrapped service invocation or response future before it returns a response. Explicitly exclude subsequent response-body polling, detached tasks, and non-unwinding termination. Scope the survival requirement to those same covered panics. Preserve coverage of handlers, extractors, and middleware executing within that boundary.

2. **[P2] Reconcile the payload-logging contract with the planned non-string fallback.** [panic-envelope/spec.md:54](/home/pfa/projects/labeler/.worktrees/issue-248/openspec/changes/issue-248-catch-panics-and-remove-handler-unwraps/specs/panic-envelope/spec.md:54) requires logging the payload, and line 63 explicitly promises it “in full.” But [design.md:85](/home/pfa/projects/labeler/.worktrees/issue-248/openspec/changes/issue-248-catch-panics-and-remove-handler-unwraps/design.md:85) logs only a placeholder for non-string payloads. That implementation cannot satisfy the published requirement.

   **Required edit:** Specify full error-level logging for `String` and `&'static str` payloads, and an error-level record containing a fixed non-string-payload marker for other payload types. Qualify the “logged in full” rationale accordingly. Extend D8’s test plan to cover both string representations and `panic_any(123_u32)`, asserting the appropriate log content, the same fixed public message, status `500`, code `Internal`, and no reason slug.

CHANGES_APPLIED: yes
SPECS_SHA256: 044afeb2c651959dfc6a9c9222c92c11ae36225dde2d311ef12b6010a97feace
