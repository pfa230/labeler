## Purpose

Defines what a caller receives when a request fails by panicking rather than by returning an error:
the same error envelope every other failure returns, with nothing of the panic in it. This is the
counterpart to `request-error-envelope`, which covers failures the request layer can name before a
handler runs; a panic is the failure the service cannot name at all. It also states, once, the
boundary the guarantee holds within, because a panic is not catchable everywhere.

## ADDED Requirements

### Requirement: What counts as a covered panic

This capability guarantees an answer for a bounded set of panics, and states the bound here so that
every requirement below can name it rather than restate it.

A **covered panic** is an unwinding panic that propagates out of the service's request-handling
pipeline either while the request is being dispatched into it, or while the response future is being
driven, at any point **before that future yields a response**. Within that window the service holds
the whole response, so it can discard whatever was in progress and substitute one of its own. A panic
raised in a handler body, in an extractor, or in middleware executing inside that window SHALL be
treated as covered.

Three kinds of panic SHALL NOT be treated as covered, and this capability makes no promise about
them:

- **A panic raised while the response body is being produced**, after the response head has been
  returned. By then the status and headers are settled and may already be on the wire, so no fresh
  JSON `500` can replace them. This includes a panic inside a body callback installed by logging or
  tracing middleware, which runs as the body is polled and not before.
- **A panic in a task the service detached from the request.** Such a panic does not unwind through
  the request's own future, so nothing on the request path observes it.
- **A termination that does not unwind**: a build configured to abort on panic, an explicit abort, a
  stack overflow, or the process being killed. Nothing running in the process can answer those.

Naming the bound is the point. A guarantee stated wider than the mechanism reaches would be read by
the next person as covering the cases it silently drops.

#### Scenario: A panic before the response future yields is covered

- **WHEN** a panic unwinds out of a handler, an extractor, or middleware, before any response for that
  request has been produced
- **THEN** it is a covered panic and every requirement below applies to it

#### Scenario: A panic while the response body is produced is not covered

- **WHEN** a panic is raised after the response head has been returned, while the body is being
  produced
- **THEN** it is not a covered panic and this capability specifies nothing about the outcome

### Requirement: A covered panic answers with the error envelope

The service SHALL answer every request that ends in a covered panic. It SHALL NOT close the
connection without a response, and SHALL NOT leave the caller with no response.

The response SHALL have status `500`, `Content-Type: application/json`, and the body defined by
`docs/SPEC.md` §10 — `{ "error": { "code": ..., "message": ..., "details": ... } }` — with `code`
`Internal`. `Internal` is not a new code: `request-error-envelope` already added its row (500, "the
service failed for a reason not attributable to the request") to §10's code table, and this
requirement neither widens nor narrows that row.

This SHALL hold wherever inside the covered window the panic is raised: in the handler, in an
extractor, or in middleware. It SHALL hold for every route the service serves, both under `/api` and
outside it.

This supersedes the frozen `docs/SPEC.md` §10 sentence "All errors return JSON" **as it applies to a
covered panic**, which today produces no response and therefore no JSON. `request-error-envelope`
supersedes the same sentence for request-body rejections; the two supersessions are disjoint and every
other part of §10 remains authoritative.

A covered panic SHALL NOT be reported as a client error. The service SHALL NOT answer one with any
`4xx` status, and in particular SHALL NOT convert it into a `404` or a `400`: the caller's request may
be entirely correct, and telling them otherwise sends them to fix something that is not broken.

#### Scenario: A handler on an API route panics

- **WHEN** an admitted caller reaches a handler under `/api` that panics before returning a response
- **THEN** the response status is `500`
- **AND** the response `Content-Type` is `application/json`
- **AND** the body parses as JSON with a top-level `error` object carrying `code` and `message`
- **AND** `error.code` is `Internal`

#### Scenario: A route outside `/api` panics

- **WHEN** a caller reaches a route outside `/api` that panics before returning a response
- **THEN** the response status is `500`
- **AND** `error.code` is `Internal`

#### Scenario: A covered panic is never answered with a client error status

- **WHEN** any request ends in a covered panic
- **THEN** the response status is `500` and is not in the `4xx` range

### Requirement: The panic payload reaches the log and never the caller

The service SHALL write an error-level log record for every covered panic.

What that record carries depends on what the panic carried, because a Rust panic payload is an opaque
value and only two shapes can be read back:

- A payload that is a `String` or a `&'static str` — which is what `panic!`, `assert!` and
  `unwrap()` produce — SHALL be logged in full.
- A payload of any other type SHALL be logged as a fixed marker naming it as a payload the service
  could not read. There is nothing else to be had from it, and the marker SHALL be the same string
  whatever the payload's type or value was.

The response body SHALL NOT carry the panic payload, nor any part of it, for either shape. A panic
message is an internal string: it is written for whoever reads the source, and it can name filesystem
paths, template contents, or values taken from other requests. The `message` the caller receives SHALL
be a fixed string that carries nothing derived from the panic or from the request, and SHALL be the
same string for both shapes.

This is what makes catching panics safe rather than merely convenient. The objection that a caught
panic hides an invariant violation does not hold for the shapes an operator can act on, because those
are logged in full; a payload of any other type was already unreadable before this capability existed,
and the marker at least records that one occurred.

#### Scenario: A formatted panic message is logged

- **WHEN** a request panics with a distinctive interpolated message, so the payload is a `String`
- **THEN** an error-level log record emitted for that request contains that message in full

#### Scenario: A literal panic message is logged

- **WHEN** a request panics with a distinctive string literal, so the payload is a `&'static str`
- **THEN** an error-level log record emitted for that request contains that message in full

#### Scenario: A payload that is not a string is logged as a marker

- **WHEN** a request panics with a payload that is neither a `String` nor a `&'static str`
- **THEN** an error-level log record emitted for that request contains the fixed unreadable-payload
  marker
- **AND** the response is `500` with `error.code` `Internal`, carrying the same fixed
  `error.message` a string payload would have produced

#### Scenario: The panic message is not in the response

- **WHEN** a request panics with a distinctive payload string
- **THEN** the response body does not contain that string
- **AND** `error.message` is the same fixed string for every covered panic, whatever the payload was

### Requirement: A covered panic carries no reason slug

`error.details` on the response to a covered panic SHALL NOT carry a `reason`. `docs/SPEC.md` §10.1
names the complete set of codes that carry one, `Internal` is not among them, and a panic is by
definition a cause the service has not classified.

No new `reason` slug SHALL be added for panics. The registry gate that pins `Reason` against §10.1
and against `openspec/specs/` is therefore unaffected by this capability.

#### Scenario: A panic response has no reason

- **WHEN** a request ends in a covered panic, with a payload of any type
- **THEN** `error.details` is absent, or is present and carries no `reason` key

### Requirement: The service survives a covered panic

A covered panic SHALL NOT end the process. The service SHALL keep accepting requests after one, and
SHALL keep answering them.

This is a live constraint on how the service is built, not a restatement of what the language does by
default: a build configured to abort on panic turns every covered panic into a process death, and so
violates this requirement rather than satisfying the ones above vacuously. A termination that does not
unwind for some other reason is outside this capability, as the covered-panic requirement states.

The guarantee is about answering, not about repair. A panic raised while the service holds a lock over
shared state can leave that state unusable, and a later request that depends on it can fail for that
reason. What this requirement forbids is such a failure going unanswered: it SHALL produce a response,
either the envelope this capability defines or whatever error the service raises in its own right.

#### Scenario: A later request is served normally

- **WHEN** a request ends in a covered panic and is answered with `500`
- **AND** the caller then issues an unrelated request that does not depend on any state the panicked
  request held
- **THEN** that request is answered normally, with the status and body it would have had if no panic
  had occurred
