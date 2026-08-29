## MODIFIED Requirements

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
Every other row of the table, and every other code, is unchanged by *this* capability and remains
authoritative until another names one. `param-resolution` names the `TemplateInvalid` row; the two
supersessions are disjoint.

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
