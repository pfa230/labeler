# Diff review

AUTHORS: agy
REVIEWER: claude
VERDICT: APPROVE
ROUNDS: 1
TREE_SHA256: bf9ab7d86c181fff5b0a12f65e5063a8aa222c8bd8fa25333b8f185b6f4e0526
SPECS_SHA256: 044afeb2c651959dfc6a9c9222c92c11ae36225dde2d311ef12b6010a97feace

## Diff review: issue-248-catch-panics-and-remove-handler-unwraps

### What I verified

Gates, run in this worktree [verified]: `cargo fmt --check` clean; `cargo clippy --all-targets --all-features -- -D warnings` clean after touching both changed sources (not a cache replay); `cargo test` green (906 unit + 3 integration binaries, 0 failed). `openspec validate <change> --strict` reports valid. `Cargo.lock` is unmodified and no new dependency is pulled by the `catch-panic` feature.

Correctness of the two halves against the artifacts [verified]:

- Layer ordering is right. `src/api.rs:405-406` applies `TraceLayer` then `CatchPanicLayer`, and axum's `Router::layer` makes the later call the outer one, so catch-panic wraps the trace layer, the `/api` nest and its auth middleware, `nest_service("/assets")` and the SPA fallback. `panic_outside_api_returns_internal_envelope` (`src/lib.rs:9556`) proves it empirically for a non-`/api` route.
- D1's boundary claim matches upstream. `tower-http-0.7.0/src/catch_panic.rs:203-216` and `269-279` wrap `inner.call` and each future poll, and on `Ok(res)` only re-box the body. The spec's exclusion of body-polling panics is accurate, not a hedge. Upstream's `CatchPanic` itself logs nothing, so the log assertions in `panic_envelope_and_logging_for_payload_shapes` can only be satisfied by `handle_panic` (`src/api.rs:379-387`).
- The new tests are genuinely falsifiable. Without the layer, `app.oneshot(...)` unwinds into the test thread; without `ok_or_else` at `src/api.rs:2217-2221`, the panic yields 500, and the test asserts 404. Neither can pass against the pre-change tree.
- The `connections` MODIFIED delta is a pure superset of the published requirement: `diff` of `openspec/specs/connections/spec.md:78-172` against the delta shows only the two added paragraphs and the added scenario, nothing dropped or reworded.
- `src/api.rs` holds no `.unwrap()` outside `#[cfg(test)]`. The test module starts at `src/api.rs:3603`; the first remaining hit is `3611`. Issue criterion 3 is met.
- The mutex guard in `AppState::after_connection_update` (`src/api.rs:223-233`) is a statement temporary and is dropped before the `.await`, which is what keeps the handler future `Send`; the code compiling as an axum handler is the proof.
- `AppError::internal` carries `details: None` (`src/errors.rs:410-417`) and `ErrorBody` omits `None`, so the no-reason requirement holds structurally, not just by assertion.

### Findings (all non-blocking)

**1. `src/api.rs:356-357` — the two panic constants are `pub`, widening the library's public API for a test-only need.** `UNREADABLE_PANIC_MARKER` is read only by `src/lib.rs:9548`, and `PANIC_RESPONSE_MESSAGE` has no reader outside `handle_panic` at all. `pub(crate)` covers both. Nothing in the plan asked for them to be exported. Cosmetic, but it publishes two strings the crate now has to keep.

**2. `openspec/.../tasks.md:1.1` and `5.2` are checked over a `Cargo.lock` check that could not have been performed as written.** 1.1 says "confirm `Cargo.lock` records only the feature change"; Cargo.lock records no feature selections at all, and 5.2 lists `Cargo.lock` among the files the diff touches while `git status` shows it unmodified. The substance behind both (no new dependency, `tower-http` still 0.7.0) is true and I verified it independently, so this is a wording defect in the task text rather than an unearned claim about behaviour. Worth correcting only because AGENTS.md treats a checked box as a claim the next reader trusts.

**3. `src/api.rs:786-794` — the new `winner_rel` fallback skips the `\` to `/` normalisation every other element of the same `files` vector gets** (`796-800` maps `refused` through `.replace('\\', "/")`). The branch is unreachable per the design's own Context finding, and the pre-change code was equally un-normalised on the `file_name()` path, so this changes nothing observable. Noted for consistency only.

**4. Coverage gap the plan chose deliberately, recorded so it is not mistaken for an oversight.** The panic-envelope requirement's prose says the guarantee holds "in the handler, in an extractor, or in middleware", and only the handler case has a test. That is consistent with the capability's own scenarios, which name routes rather than pipeline positions, and D8 states the choice. No task claims extractor or middleware coverage, so nothing is over-claimed.

Nothing here changes a status code, a body, or a contract, and none of it must be fixed before the change lands.

