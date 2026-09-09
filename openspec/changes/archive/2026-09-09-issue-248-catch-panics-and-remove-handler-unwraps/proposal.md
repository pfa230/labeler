## Why

Implements GitHub issue **#248**.

A panicking handler produces no HTTP response at all: the caller gets a dropped connection instead of
the `{ "error": { code, message, details } }` envelope every endpoint is documented to return. There
is no `CatchPanicLayer` in the stack, and `tower-http` is declared without the `catch-panic` feature
(`Cargo.toml:22`), so the crate that would produce the envelope is not even compiled in. Seven
`.unwrap()` calls sit in `src/api.rs` outside `#[cfg(test)]`, six of them on the request path, and a
render panic is already filed separately (#235). Fixing #235 removes one trigger; it does not change
what happens the next time something panics.

## What Changes

- The six request-path `.unwrap()` calls in `src/api.rs` are replaced by explicit outcomes:
  `src/api.rs:2116` answers `404` when the connection it just updated is gone by the time it reads it
  back; `1034`, `1129` and `1133` answer `500 Internal` when a registry invariant does not hold; and
  `704`, `711`, `712` render their filename with `to_string_lossy()`, falling back to the whole path,
  because they are decoration on an error message about something else.
- `tower_http::catch_panic::CatchPanicLayer` is added to the router with a responder that emits the
  standard envelope through `AppError::internal`, so a panic that unwinds out of the wrapped service
  becomes a `500` with `code` `Internal`. `catch-panic` is added to the `tower-http` features.
- The guarantee is bounded by what that middleware reaches: a panic that unwinds out of the request's
  dispatch or out of its response future **before that future yields a response**. Handlers,
  extractors and middleware inside that window are covered. A panic raised while the response body is
  being produced — including inside a body callback the trace layer installs — is not, because the
  status and headers are already settled; nor is a panic in a detached task, nor a termination that
  does not unwind. The new capability states that bound rather than promising past it.
- The panic payload is logged at error level: in full when it is a `String` or a `&'static str`, and
  as a fixed marker for any other payload type, which is all `Box<dyn Any>` yields. It is never sent.
  No new `Reason` slug is introduced.
- **BREAKING** for any client that treats a dropped connection as a distinguishable outcome: a panic
  in that window that used to close the connection now returns a well-formed `500`. Nothing else
  changes shape.

### What this change does not do

Fixing the panics #235 names, or any other individual panic, is out of scope. This change makes a
panic answerable; it does not enumerate what still panics.

Nor does it extend the answer to the panics the middleware cannot reach, listed above. Covering a
panic during body production would mean buffering every response before sending its head, which is a
different change with a real cost on the PDF and ZIP paths and no issue asking for it.

## Capabilities

### New Capabilities

- `panic-envelope`: what the service returns when a request fails by panicking rather than by
  returning an error. Sibling to `request-error-envelope`, which covers failures *before* a handler
  runs; this one covers a failure the service cannot name at all.

### Modified Capabilities

- `connections`: "Updating a connection" gains the outcome for a connection that is absent when the
  handler reads back the row it just wrote. Today that path panics; it becomes the `404` the
  requirement already specifies for an unknown id.

## Impact

- `src/api.rs`: six unwrap sites, the router in `app()`, a new panic responder, and one test-only
  seam on the connection-update path (the shape `mid_write_hook` / `pre_publish_hook` already use).
- `Cargo.toml:22`: `tower-http` gains the `catch-panic` feature. No new dependency.
- `src/errors.rs`: unchanged. `AppError::internal` already exists and carries no `Reason`, so the
  §10.1 registry gate (`src/errors.rs:677-711`) is untouched.
- `src/openapi.rs`: unchanged. No new model and no new `code` string; `Internal` is already published
  by `request-error-envelope`.
- No `ui/` change.
