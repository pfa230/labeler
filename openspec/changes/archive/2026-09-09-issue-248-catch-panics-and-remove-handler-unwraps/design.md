## Context

See `proposal.md` — Why. The facts below were verified by reading the tree at `760a547`; several of
them differ from what issue #248 assumes, and the design turns on those differences.

**Where the unwraps are.** `src/api.rs` holds seven `.unwrap()` calls outside `#[cfg(test)]`: `704`,
`711`, `712`, `1034`, `1129`, `1133`, `2116`. `src/api.rs:1090` is already an `ok_or_else`, as the
issue notes.

**The `2116` race is not reachable through the HTTP API.** `update_connection_h` takes
`state.write_lock` at `src/api.rs:2097` and holds it past the read-back at `2116`;
`delete_connection_h` takes the same lock at `2133`. `delete_connection_and_default`
(`src/store.rs:761`) is the only code path that removes a connection row, and that handler is its
only caller. So the interleaving the issue describes cannot happen in a single process, and the
`None` branch is a defensive branch rather than a live bug. This does not change the fix: the lock is
an implementation detail no requirement publishes, and the handler must answer for the state it
observes. It does change what the test has to be — see D5.

**A non-UTF-8 filename cannot reach the registry.** `src/templates.rs:726` refuses a file whose name
is not valid UTF-8, and `template-registry`'s "A template's id is its filename stem" restricts an id
to `^[a-zA-Z0-9_-]+$`. Every path the registry hands out therefore has an ASCII filename. So `1133`,
`704`, `711` and `712` cannot fire on a filename's encoding either; like `1034` and `1129`, they are
registry invariants. They are still removed, because the point of #248 is that no panic path survives
in a request handler, not that these particular ones are reachable.

**`src/api.rs:1133` is not a display string.** The issue's table reads it as one. It is not: the
`filename` it produces is the argument to `fs_safe::unlink_file` at `src/api.rs:1155`, which is the
delete's actual effect. See D6.

**What already exists.** `tower-http` 0.7.0 is in `Cargo.lock`; only the `catch-panic` feature is
missing. `AppError::internal` (`src/errors.rs:410`) yields status 500, `code` `Internal`, and no
`details`, which `ErrorBody` omits when `None` (`src/models.rs:15`). `AppError::into_response`
already emits a `tracing::error!` for any 5xx carrying `message` (`src/errors.rs:465-472`). No
`spawn_blocking` or `thread::spawn` exists in `src/`, so no request-path work is detached from the
request's own task and every panic in it unwinds through that task. `Cargo.toml` declares no `panic` setting, so unwinding is in effect.

## Goals / Non-Goals

**Goals**

- One place decides what a panic looks like on the wire, and it is not a handler.
- Every `.unwrap()` on the request path in `src/api.rs` is replaced with a named outcome, so the
  layer is a backstop rather than the mechanism.
- The evidence for both halves is HTTP-level and reproducible: a status code and a body, obtained
  through the router.

**Non-Goals**

- Fixing any particular panic, including #235.
- Auditing `.unwrap()` outside `src/api.rs`. The issue scopes criterion 3 to that file.
- Making the connection-update read-back atomic. See D5's rejected alternative.
- Repairing state a panic corrupts. See Risks.
- Answering a panic the chosen middleware cannot reach: one raised while the response body is
  produced, one in a detached task, or a termination that does not unwind. D1 states the boundary and
  the spec publishes it.

## Decisions

### D1. `CatchPanicLayer` at the outermost position of `app()`, and the boundary that gives

The layer goes on the top-level router built in `src/api.rs:310-322`, applied after
`TraceLayer::new_for_http()` so that it wraps it: `.layer(TraceLayer::…).layer(CatchPanicLayer::…)`
puts catch-panic outside, so the `/api` auth middleware, extraction, handler bodies, `ServeDir`'s and
the SPA fallback's own invocation, and the trace layer's `call` are all inside it.

**What that reaches, exactly.** `CatchPanic` wraps two things and no more
(`tower-http-0.7.0/src/catch_panic.rs:180-216, 240-279`): `catch_unwind` around
`self.inner.call(req)`, and `catch_unwind` around each poll of the resulting future. When that future
yields `Ok(res)` the layer only re-boxes the body — `res.map(|body| UnsyncBoxBody::from_inner(…))` —
and does **not** wrap the body's own polling. So the covered window closes the moment the response
future returns a response, and this is the boundary the spec's first requirement publishes.

Three things fall outside it, and the design accepts all three rather than working around them:

- **A panic while the body is polled.** The status line and headers are already produced and may be
  on the wire, so substituting a JSON `500` is not available at any price short of buffering every
  response before sending its head. That would defeat streaming on the PDF, ZIP and `ServeDir` paths
  for a case nothing has been observed to hit. Notably this includes `TraceLayer`'s `on_body_chunk`
  and `on_eos` callbacks, which run during body polling: putting catch-panic outside the trace layer
  covers the trace layer's `call`, not its body wrapper.
- **A panic in a detached task.** `src/` spawns none on the request path today — there is no
  `spawn_blocking` or `thread::spawn` in it — so this bound costs nothing now. It is stated because
  adding one later would move work outside the guarantee silently.
- **A termination that does not unwind**: `panic = "abort"`, an explicit abort, a stack overflow.
  `catch_unwind` sees nothing in any of those cases.

An earlier draft of this design claimed the layer covered `TraceLayer` and `ServeDir` outright. It
covers their invocation; it does not cover the bodies they return. The spec is written to the narrower
claim.

*Alternatives.* Wrapping each handler body in `std::panic::catch_unwind` — rejected: it cannot cover
extractors or middleware, and it is a rule every future handler has to remember, which is what the
issue is complaining about. Writing the Tower service by hand — rejected: it reimplements a
maintained layer and would have to re-derive the `Response`/`ResponseFuture` unwind handling
(`tower-http-0.7.0/src/catch_panic.rs:180-280`) to no benefit. A `std::panic::set_hook` — rejected:
a panic hook runs during the unwind and has no way to produce a response.

*Body plumbing.* `CatchPanic`'s response type is `Response<UnsyncBoxBody<Bytes, BoxError>>`, not
`Response<axum::body::Body>`. axum accepts it: `Router::layer` requires only that the layered
service's response implement `IntoResponse`, and axum implements that for any
`Response<B: http_body::Body<Data = Bytes> + Send + 'static>`. `app()`'s signature does not change,
so the test helpers around it (`with_auth`, `build_app*`) do not either. The responder is passed as a
`fn` pointer so the layer is unambiguously `Clone + Send + Sync + 'static`.

### D2. The responder logs the payload and sends a fixed message

The responder downcasts the payload to `String` and then to `&'static str` — the two shapes
`panic!`, `assert!` and `unwrap()` produce — emits a `tracing::error!` carrying whichever matched in
full, and returns `AppError::internal(<fixed string>).into_response()`.

A payload of any other type, which only `std::panic::panic_any` produces, cannot be read back out of
`Box<dyn Any>` at all. It is logged as one fixed marker string, and the spec says so rather than
promising a payload the code cannot obtain: an error-level record still fires, so the panic is not
invisible to an operator, and it is the same record whatever the payload's type was.

The payload must not go into `AppError::internal`'s message: that message is serialised into
`error.message` and sent, which is exactly what the spec forbids. Logging it separately is what keeps
the two apart.

The message is fixed rather than derived, so a caller cannot mine `error.message` for internal detail
by provoking different panics, and so the "same string for every covered panic" scenario is checkable.
It is the same string for the unreadable-payload case too: the caller learns nothing from the payload's
type either.

### D3. `catch-panic` is added to `tower-http`'s features

`Cargo.toml:22` becomes `features = ["trace", "fs", "catch-panic"]`. No new dependency and no
`Cargo.lock` churn beyond the feature: `tower-http` 0.7.0 is already resolved.

### D4. The registry invariants answer `500 Internal`, they do not crash

`1034` (`registry.detail`), `1129` (`registry.path`) and `1133` (see D6) become `AppError::internal`
on `None`. The invariant is real — `registry.get(&id)` succeeded a few lines above, against the same
snapshot — but defending it by crashing costs the caller their response, while defending it with an
error costs one line. Same observable status as the layer would produce; the difference is that the
handler names the failure, and the log record comes from `AppError::into_response` with a message
that says which invariant broke.

### D5. `2116` answers `404`, and a test-only seam is what proves it

`state.store().get_connection(&id).await?` returning `None` becomes `AppError::not_found(&id)`, which
is what the endpoint two lines above already returns when `update_connection` reports the id unknown.

Because the write lock makes the interleaving unreachable through two concurrent requests (see
Context), the test cannot stage it with a second HTTP call. It needs a seam, which is the shape
`mid_write_hook` and `pre_publish_hook` already have (`src/api.rs:79-86, 161-191`): a `#[cfg(test)]`
field on `AppState`, a `set_*` installer, and a private fire point compiled out of the shipped binary.
The new one fires between the update and the read-back, and the test's hook deletes the connection
through the store.

It differs from the two existing hooks in one way: those run synchronous filesystem calls, and this
one has to reach `Store`, whose methods are `async fn`. So the hook returns a boxed future and the
fire point awaits it. The installed closure is cloned out of its `std::sync::Mutex` and the guard
dropped before the await — holding a `std::sync::MutexGuard` across an await makes the handler future
non-`Send`, which axum rejects. The hook is handed `&Store` by the fire point rather than capturing
one, so the test does not have to build a second handle to the same in-memory database.

The test asserts `404` and the envelope. It is red against today's code: the `unwrap()` panics, which
before D1 lands drops the connection and after it lands yields `500` — neither is `404`. The task list
should keep the two halves separable enough to see that.

*Alternative rejected.* Have `Store::update_connection` return the updated row from inside its own
transaction, removing the second read and the race with it. That is the stronger fix and it deletes
the `Option` rather than handling it. It is not taken here: #248 settles the outcome as "return
`AppError::not_found` on `None`", and criterion 1 asks for the interleaving to be demonstrated, which
an atomic update leaves nothing to demonstrate. Recorded so a later reader knows it was weighed.

### D6. `1133` answers `500`, not a lossy filename — a deliberate deviation from the issue

Issue #248 groups `1133` with `704`, `711` and `712` and prescribes `to_string_lossy()` with a
whole-path fallback. That is right for the other three and wrong for this one. The value at `1133` is
`filename`, and its only use is `fs_safe::unlink_file(parent_resolved.target_fd.as_fd(), &filename)`
at `src/api.rs:1155`. A lossy spelling there does not decorate a message, it selects a file to
unlink: it either names nothing, in which case `unlink_file` maps `ENOENT` to `Ok(())`
(`src/fs_safe.rs:789`) and the endpoint returns `204` having deleted nothing, or it names a different
existing file and deletes that. The first is a silent fallback over a real failure; the second is
worse.

So `1133` follows D4 and returns `AppError::internal` when `file_name()` and `to_str()` do not both
succeed. Given the Context finding that no such path can be in the registry, this branch costs
nothing and hides nothing.

*Alternative considered.* Widen `fs_safe::unlink_file` to take `&OsStr`, which `rustix`'s `Arg`
accepts, removing the `to_str()` step entirely. Rejected as scope: it changes a shared helper's
signature and its error formatting for a case that cannot arise, and `AppError::internal` covers it in
one line inside the handler that has the problem.

### D7. `704`, `711`, `712` render lossily, exactly as the issue says

These three build the prose of a duplicate-file report. `to_string_lossy()` with the whole path as the
fallback when `file_name()` is `None` matches the pattern already used twelve lines above at
`src/api.rs:679`, in the same function, for the same purpose. Nothing is masked: the message still
names the path.

### D8. The panic tests need routes that panic, and they are `#[cfg(test)]` routes

Criterion 2 is an HTTP-level assertion, so something reachable through the router has to panic.
Provoking a real panic is not an option: the only one on record is #235's, which another change is
about to remove, and a test keyed to it would then be testing nothing.

Two `#[cfg(test)]` routes, one registered inside `api_router()` and one on the top-level router
outside `/api`, cover the capability's route scenarios and prove the layer sits outside the `/api`
nest rather than inside it. They compile out of the shipped binary on the same terms as the hooks.

The `/api` one takes the payload shape as a path segment, because the payload requirement now
distinguishes three of them and each needs its own assertion:

| Segment | Panic raised | Payload type | Log assertion |
| --- | --- | --- | --- |
| formatted | `panic!("… {interpolated}")` | `String` | the whole interpolated message appears |
| literal | `panic!("<distinctive literal>")` | `&'static str` | the whole literal appears |
| any | `std::panic::panic_any(123_u32)` | neither | the fixed unreadable-payload marker appears |

All three assert the same public outcome: status `500`, `error.code` `Internal`, `error.message`
equal to the one fixed string, and no `reason` under `error.details`. The `formatted` and `literal`
cases additionally assert their payload is absent from the response body. Asserting `error.message` is
byte-equal across all three is what holds the "same string whatever the payload was" scenario; a test
that only checked each response in isolation would pass with three different messages.

`panic_any(123_u32)` is chosen over a custom type so the test needs no fixture: it is the shortest
value that is neither of the two readable shapes.

Log capture reuses the existing `init_test_tracing` / `TEST_LOG_BUFFER` machinery
(`src/lib.rs:9290-9327`). That buffer is a `thread_local`, so these tests must stay on the
default current-thread `#[tokio::test]` runtime; a `multi_thread` flavour would run the handler on a
worker thread and capture nothing, and the assertion would pass or fail for the wrong reason.

Nothing tests the three uncovered cases from D1, because there is nothing to assert: the spec states
no outcome for them, and a test asserting today's incidental behaviour would pin something no
requirement promises.

### D9. Criterion 3 is checked by reading, not by a lint

`src/api.rs` holding no `.unwrap()` outside `#[cfg(test)]` is verified by grep during the diff review.
`#![cfg_attr(not(test), deny(clippy::unwrap_used))]` would make it a gate, but the lint is a
restriction lint that would then bind every future edit to `src/api.rs` and nothing else in the crate,
which is a project-wide policy decision and not this issue's. Not taken; recorded so the option is not
rediscovered as novel.

### D10. Nothing is added to the error contract

No new `Reason`, so `src/errors.rs` and the §10.1 registry gate (`src/errors.rs:677-711`) are
untouched. No new `code` string: `Internal` is already published by `request-error-envelope`. No new
model, so `src/openapi.rs` is untouched. The `500 Internal` responses the OpenAPI document already
describes per-endpoint are unaffected; documenting a panic per-endpoint would be noise, since it is a
property of the stack rather than of any route.

## Risks / Trade-offs

- **A panic taken while a lock is held leaves that lock poisoned, and the service answers 500 for
  every later request that needs it.** `Store` guards its SQLite connection with a
  `std::sync::Mutex` and unwraps the guard (`src/store.rs:93`, `.expect("store lock")`), so a panic
  inside a store call poisons it permanently. → Not fixed here, and the spec's "survives a covered
  panic" requirement is written to say what actually holds: every such failure is *answered*, not that
  the service is repaired. Today those requests get a dropped connection instead, so this change is
  strictly an improvement on the same state. Recovering from a poisoned store is its own problem and
  its own issue if anyone wants it.
- **Catching panics can hide an invariant violation that a single-user deployment would rather see
  loudly.** → A `String` or `&'static str` payload — which is every panic the crate's own code raises
  — is logged in full at error level, and the default panic hook still writes its backtrace line to
  stderr for every panic, the unreadable ones included. Nothing becomes less visible to the operator;
  one thing becomes visible to the caller.
- **Setting `panic = "abort"` in a future profile would defeat the whole change silently.** → The
  "service survives a covered panic" scenario is the tripwire: under `abort` the test process dies and
  the suite fails rather than passing vacuously. The spec puts non-unwinding termination outside the
  guarantee, which is honest about what the middleware can do, and this scenario is what stops that
  exclusion from becoming a way to satisfy the capability by never unwinding at all.
- **Every response body is now boxed by the layer.** → One allocation wrapping an
  already-materialised body; the PDF and PNG paths hand over a `Vec<u8>` that is moved, not copied.
- **A `#[cfg(test)]` route and a third `#[cfg(test)]` hook add test-only surface to production
  source.** → The precedent is set and documented in place (`src/api.rs:79-86`), and the alternative
  for each is no coverage at all: the service cannot stage either condition itself.
