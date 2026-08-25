## Purpose

Defines what the service returns when the request layer rejects a request before any handler runs:
a body it cannot deserialize, a missing or wrong `Content-Type`, an oversized body, or a path segment
it cannot deserialize. These failures happen outside handler code, which is why they are specified
once for the whole API rather than per endpoint.

## ADDED Requirements

### Requirement: Request admission precedes body and path mapping

Everything in this capability describes what happens **after** the service's authentication and
origin checks have admitted a request. Those checks run as middleware outside every API handler and
therefore outside extraction: a request they reject never reaches the body or path extractor at all.

`docs/SPEC.md` §11 remains authoritative and takes precedence over every mapping below. This
capability supersedes nothing in §11 and narrows nothing in it. Specifically, a rejected request SHALL
keep §11's outcome regardless of what its body contains: `401` when authentication is required and
absent or invalid, and `403` when the origin check fails or when an authentication-managed route is
called while authentication is disabled.

A malformed body therefore does not produce `400` on a request that was never admitted, and a caller
cannot use a malformed body to learn anything about an endpoint it is not entitled to reach.

#### Scenario: An unauthenticated request is rejected before its body is read

- **WHEN** a client sends a syntactically invalid JSON body to a protected endpoint with no
  credentials
- **THEN** the response status is `401`
- **AND** `error.code` is `Unauthorized`
- **AND** the response does not report `json_malformed`

#### Scenario: A cross-origin request is rejected before its body is read

- **WHEN** a browser client sends a syntactically invalid JSON body to a state-changing endpoint with
  a session cookie and a mismatched `Origin`
- **THEN** the response status is `403`
- **AND** the response does not report `json_malformed`

#### Scenario: An admitted request reaches the mapping

- **WHEN** a client sends a syntactically invalid JSON body to a protected endpoint with valid
  credentials and an acceptable origin
- **THEN** the mapping in the requirements below applies
- **AND** the response status is `400`

### Requirement: A rejected request body returns the error envelope

Every endpoint that reads a JSON request body SHALL, when that body cannot be accepted **on a request
that was admitted**, respond with
the error envelope defined by `docs/SPEC.md` §10 — `Content-Type: application/json` and a body of
`{ "error": { "code": ..., "message": ..., "details": ... } }` — and never with a `text/plain`
diagnostic from the web framework.

This supersedes the blanket "All errors return JSON" claim of `docs/SPEC.md` §10 as it applies to
request-body rejections, and it supersedes §10.1's `json_malformed` and `request_body_invalid` rows,
by stating the complete mapping below.

**`json_malformed` is redefined and widened.** §10.1 defines it as "The request body is not parseable
JSON". It SHALL now mean **the request body could not be deserialized into the type the endpoint
declares**, which covers both a syntax error and a body that is syntactically valid JSON of the wrong
shape. The wider definition matches what the service has always emitted on the four endpoints that already
return the envelope, so those four report the same reason before and after and only the published
definition changes to match them. The other fifteen report no reason at all today, because they return
no envelope at all, and begin reporting `json_malformed`. A client cannot distinguish a syntax error from a shape mismatch by `reason`, and
`details.error` is the only thing that separates them.

The mapping is:

| Condition | Status | `code` | `details.reason` |
| --- | --- | --- | --- |
| Body is not syntactically valid JSON | 400 | `InvalidRequest` | `json_malformed` |
| Body is valid JSON but does not deserialize into the endpoint's type | 400 | `InvalidRequest` | `json_malformed` |
| Body could not be read from the connection | 400 | `InvalidRequest` | `request_body_invalid` |
| Body exceeds the endpoint's size limit | 413 | `PayloadTooLarge` | *(none)* |
| `Content-Type` is absent, unparseable, or not a JSON media type | 415 | `UnsupportedMediaType` | *(none)* |
| Any other body rejection | 400 | `InvalidRequest` | `request_body_invalid` |

A **JSON media type** is `application/json` or any `application/<subtype>+json`. Both SHALL be
accepted; a vendor or profile media type such as `application/problem+json` SHALL NOT be rejected
merely for carrying a suffix.

For the two `json_malformed` rows, `details.error` SHALL additionally carry the parser's own message,
so a caller can locate the fault without guessing.

#### Scenario: Body is not parseable JSON

- **WHEN** a client sends `{"connector":"nope",` to `PUT /api/connections/{id}` with
  `Content-Type: application/json`
- **THEN** the response status is `400`
- **AND** the response `Content-Type` is `application/json`
- **AND** `error.code` is `InvalidRequest`
- **AND** `error.details.reason` is `json_malformed`
- **AND** `error.details.error` is a non-empty string carrying the parser's message

#### Scenario: Body is valid JSON of the wrong shape

- **WHEN** a client sends `{"connector":42,"name":"home","base_url":"http://hb.lan:7745"}` to
  `PUT /api/connections/{id}`, where `connector` is declared as a string
- **THEN** the response status is `400`, not `422`
- **AND** `error.code` is `InvalidRequest`
- **AND** `error.details.reason` is `json_malformed`

#### Scenario: Body omits a required key

- **WHEN** a client sends `{"connector":"nope","base_url":"http://hb.lan:7745"}` to
  `PUT /api/connections/{id}`, omitting a required key
- **THEN** the response status is `400`
- **AND** `error.code` is `InvalidRequest`
- **AND** `error.details.reason` is `json_malformed`

#### Scenario: Content-Type is missing

- **WHEN** a client sends a syntactically valid JSON body to a JSON endpoint with no `Content-Type`
  header
- **THEN** the response status is `415`
- **AND** `error.code` is `UnsupportedMediaType`

#### Scenario: A suffixed JSON media type is accepted

- **WHEN** a client sends a valid body to a JSON endpoint with `Content-Type: application/problem+json`
- **THEN** the response is not `415`
- **AND** the body is deserialized as JSON, so the request reaches the handler

#### Scenario: A non-JSON media type is rejected

- **WHEN** a client sends a body to a JSON endpoint with `Content-Type: text/plain`
- **THEN** the response status is `415`
- **AND** `error.code` is `UnsupportedMediaType`

#### Scenario: Body exceeds the endpoint's limit

- **WHEN** a client sends a body larger than `POST /api/print`'s 64 KiB limit to that endpoint
- **THEN** the response status is `413`
- **AND** `error.code` is `PayloadTooLarge`

### Requirement: Every JSON endpoint returns the mapping

The mapping above SHALL hold for every endpoint in the API that reads a JSON request body. No endpoint
is exempt, and an endpoint added later is bound by it.

The service SHALL define its JSON and path extractors in a single module, and handlers SHALL use those
rather than the web framework's own. This is what makes the mapping the default a handler gets by
writing the obvious thing, instead of something each handler opts into.

This is a convention backed by a distinct type, not a structural guarantee: no mechanism in the
language or the framework can prevent a handler from naming the framework's extractor directly. The
requirement is held by the scenario below, which enumerates the endpoints that exist, and by review.

#### Scenario: Every JSON endpoint rejects a malformed body identically

- **WHEN** an admitted caller — authenticated as required by §11, with an acceptable origin — sends a
  syntactically invalid JSON body to each endpoint in the API that reads a JSON body, including
  `POST /api/printers`, `POST /api/printers/probe`, `PUT /api/printers/{id}`,
  `PUT /api/variables/{key}`, `PUT /api/settings/{key}`, `POST /api/datetime-formats/preview`,
  `POST /api/connections`, `PUT /api/connections/{id}`, `POST /api/connections/{id}/browse`,
  `POST /api/connections/{id}/materialize`, `POST /api/auth/setup`, `POST /api/auth/login`,
  `POST /api/auth/password`, `POST /api/users`, `POST /api/tokens`,
  `PUT /api/templates/{id}/group`, `POST /api/batch`, `POST /api/print` and `POST /api/render/label`
- **THEN** every response has status `400`, `error.code` `InvalidRequest` and
  `error.details.reason` `json_malformed`

#### Scenario: A body rejection is never plain text

- **WHEN** any of the endpoints above rejects a body for any reason in the mapping table
- **THEN** the response `Content-Type` is `application/json`
- **AND** the body parses as JSON with a top-level `error` object carrying `code` and `message`

### Requirement: A rejected body is not echoed into the service log

The service SHALL NOT write the request body, or a parser diagnostic quoting any part of it, to its
application log when it rejects a body. It SHALL log the rejection's classification and status only.

This is not a diagnostic preference. Deserializer messages quote the offending value verbatim, and
four of the endpoints covered by this capability accept credentials, so echoing the diagnostic writes
passwords into ordinary logs (CWE-532). Reducing the log level is not sufficient: a deployment may
enable a lower level, and log collectors gather every level.

`details.error` on the **response** is unaffected and SHALL continue to carry the parser message. It
reaches only the caller that sent the body, which is the party entitled to know why their own payload
was rejected.

#### Scenario: A malformed credential body is not echoed to the log

- **WHEN** a client sends `{"username":"admin","password":12345}` to `POST /api/auth/login`, where
  `password` is declared as a string
- **THEN** no log record emitted for the rejection contains `12345`
- **AND** the response still carries `error.details.reason` `json_malformed`
- **AND** the response still carries `error.details.error` with the parser's message

### Requirement: A rejected path parameter returns the error envelope

When a path segment cannot be deserialized into the type an endpoint declares, and the failure is
attributable to the request, the service SHALL respond with the error envelope, status `400`, `code`
`InvalidRequest`, and `details.reason` `path_param_invalid`.

This supersedes §10.1's `path_param_invalid` row, which describes a reason no endpoint currently
emits.

#### Scenario: A path segment is not valid UTF-8

- **WHEN** a client requests `GET /api/templates/%FF/source`, where `%FF` percent-decodes to a byte
  sequence that is not valid UTF-8
- **THEN** the response status is `400`
- **AND** the response `Content-Type` is `application/json`
- **AND** `error.code` is `InvalidRequest`
- **AND** `error.details.reason` is `path_param_invalid`

#### Scenario: A path segment does not parse as the declared type

- **WHEN** a client requests an endpoint declaring a numeric path parameter with a segment that is
  not a number
- **THEN** the response status is `400`
- **AND** `error.code` is `InvalidRequest`
- **AND** `error.details.reason` is `path_param_invalid`

#### Scenario: A string path parameter accepts any valid segment

- **WHEN** a client requests an endpoint whose path parameter is a string, with any segment that
  percent-decodes to valid UTF-8
- **THEN** the request reaches the handler
- **AND** the response is whatever that handler returns, including its own `404` or validation error
  for an id it does not recognise

### Requirement: A defect in the service is not reported as a client error

Some path-parameter failures indicate a defect in the service rather than a fault in the request: the
parameter set a handler declares does not match the parameters its route defines, the declared type
cannot be deserialized from path parameters at all, or the request reached the handler without the
router's path parameters attached. In each case the caller's URL may be entirely correct.

The service SHALL respond to these with status `500` and `code` `Internal`, and SHALL NOT report them
as `400 InvalidRequest` with `path_param_invalid`. The service SHALL NOT downgrade to `400` any path
rejection that the web framework itself classifies as a server error.

`path_param_invalid` therefore means "your URL", and `500 Internal` means "our bug"; conflating them
sends a caller to fix a URL that is correct.

This requirement exposes `code: Internal`, which is absent from `docs/SPEC.md` §10's code table. It
therefore supersedes that table **for the addition of `Internal` (500) only**, adding the row below.
Every other row of the table, and every other code, is unchanged and remains authoritative.

| Code | Status | When |
| --- | --- | --- |
| `Internal` | 500 | The service failed for a reason not attributable to the request. |

#### Scenario: Handler and route disagree on path parameters

- **WHEN** a handler declaring a different number of path parameters than its route defines is
  reached
- **THEN** the response status is `500`
- **AND** `error.code` is `Internal`
- **AND** `error.details.reason` is absent or is not `path_param_invalid`

#### Scenario: Path parameters never reached the handler

- **WHEN** a handler declaring a path parameter is reached without the router's path parameters
  attached to the request
- **THEN** the response status is `500`
- **AND** `error.code` is `Internal`

#### Scenario: A server-classified path rejection is never downgraded

- **WHEN** the web framework classifies a path rejection as a server error
- **THEN** the response status is in the `5xx` range
- **AND** `error.details.reason` is not `path_param_invalid`
