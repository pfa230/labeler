## 1. The panic layer and its test surface

- [x] 1.1 Add `catch-panic` to the `tower-http` feature list at `Cargo.toml:22`, leaving `trace` and
      `fs` in place. Confirm `cargo tree -p tower-http` still resolves 0.7.0 and that `Cargo.lock`
      records only the feature change (D3).
- [x] 1.2 Add the `#[cfg(test)]` panic routes: one under `api_router()` taking the payload shape as a
      path segment (`formatted` raising `panic!` with an interpolated value, `literal` raising
      `panic!` with a string literal, `any` raising `std::panic::panic_any(123_u32)`), and one on the
      top-level router in `app()` outside the `/api` nest. Give neither a `#[utoipa::path]`, so
      neither enters the OpenAPI document or the doc-derived sweeps (D8).
- [x] 1.3 Write the HTTP tests for the panic scenarios against those routes, run them, and record that
      they are red before the layer exists: with no `CatchPanicLayer` the panic unwinds out of the
      `oneshot` call and the test aborts rather than reading a status (D8).
- [x] 1.4 Add the panic responder: downcast the payload to `String`, then to `&'static str`, emit a
      `tracing::error!` carrying whichever matched in full, and for any other payload type emit the
      same error-level record carrying one fixed unreadable-payload marker. Return
      `AppError::internal(<fixed message>).into_response()`, with the payload kept out of that message
      (D2).
- [x] 1.5 Apply `CatchPanicLayer::custom` to the top-level router in `app()` after
      `TraceLayer::new_for_http()`, so catch-panic is the outermost layer, passing the responder as a
      `fn` pointer. Confirm `app()`'s signature is unchanged and `with_auth` / `build_app*` need no
      edit (D1).
- [x] 1.6 Re-run the tests from 1.3 and confirm they now pass.

## 2. What the panic tests assert

- [x] 2.1 For each of the three payload shapes, assert status `500`, `Content-Type: application/json`,
      a top-level `error` object with `code` `Internal`, and `error.details` absent or carrying no
      `reason` key.
- [x] 2.2 Assert the log: the whole interpolated message for `formatted`, the whole literal for
      `literal`, and the fixed unreadable-payload marker for `any`, using the existing
      `init_test_tracing` / `TEST_LOG_BUFFER` machinery (`src/lib.rs:9290-9327`). Keep every panic
      test on the default current-thread `#[tokio::test]` runtime, since that buffer is a
      `thread_local` (D8).
- [x] 2.3 Assert that the `formatted` and `literal` payloads do not appear anywhere in their response
      bodies.
- [x] 2.4 Assert `error.message` is byte-equal across all three shapes, in one place, so three
      different fixed messages cannot pass.
- [x] 2.5 Assert the route outside `/api` also answers `500` with `error.code` `Internal`, proving the
      layer sits outside the `/api` nest.
- [x] 2.6 Assert the service survives: after a panic answered with `500`, a request to a route that
      touches none of the panicked request's state is answered normally on the same app.

## 3. The connection read-back answers 404

- [x] 3.1 Add the `#[cfg(test)]` seam on `AppState` alongside `mid_write_hook` and `pre_publish_hook`
      (`src/api.rs:79-86, 161-191`): the field, its `set_*` installer, and a private fire point that
      runs between `update_connection` and the read-back in `update_connection_h`. The hook returns a
      boxed future and is handed `&Store`; clone it out of its `std::sync::Mutex` and drop the guard
      before awaiting, so the handler future stays `Send` (D5).
- [x] 3.2 Write the HTTP test: install a hook that deletes the connection through the store, send
      `PUT /api/connections/{id}` for a connection that exists, and assert `404` with the standard
      envelope. Run it and record the red result against the current `unwrap()` (D5).
- [x] 3.3 Replace the `.unwrap()` at `src/api.rs:2116` with `AppError::not_found(&id)` on `None`,
      matching the `404` the same handler already returns when `update_connection` reports the id
      unknown (D5).
- [x] 3.4 Re-run the test from 3.2 and confirm it passes.

## 4. The remaining request-path unwraps

- [x] 4.1 Replace the `.unwrap()` at `src/api.rs:1034` (`registry.detail`) and `src/api.rs:1129`
      (`registry.path`) with `AppError::internal` on `None`, each message naming the invariant that
      did not hold (D4).
- [x] 4.2 Replace the `.unwrap()` at `src/api.rs:1133` with `AppError::internal` when `file_name()`
      and `to_str()` do not both succeed. Do not make it lossy: `filename` is the argument to
      `fs_safe::unlink_file` at `src/api.rs:1155`, and a lossy spelling would either delete nothing
      while returning `204` or delete a different file (D6).
- [x] 4.3 Replace the `.unwrap()` calls at `src/api.rs:704`, `711` and `712` with `to_string_lossy()`,
      falling back to the whole path when `file_name()` is `None`, matching the pattern already used
      at `src/api.rs:679` in the same function (D7).

## 5. Verification and gates

- [x] 5.1 `rg '\.unwrap\(\)' src/api.rs` and confirm every remaining hit is inside `#[cfg(test)]`
      code, satisfying the issue's third criterion (D9).
- [x] 5.2 `git diff --stat` and confirm the diff touches only `Cargo.toml`, `Cargo.lock` and
      `src/api.rs` plus the test module in `src/lib.rs`: no `src/errors.rs`, no `src/openapi.rs`, no
      new `Reason`, no new `code` string, no `ui/` file (D10).
- [x] 5.3 `cargo fmt --check`
- [x] 5.4 `cargo clippy --all-targets --all-features`
- [x] 5.5 `cargo test`
