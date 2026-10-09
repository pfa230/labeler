# Errors

## Purpose

Defines how every failure is reported: the JSON error envelope, the `code` vocabulary and HTTP statuses, the `details.reason` slugs, per-label batch failures, which fault a request reports, request body and path rejections, and panics.

## Requirements

### Requirement: Error envelope

Every error response SHALL have `Content-Type: application/json` and the body `{ "error": { "code", "message", "details"? } }`. `code` and `details.reason` are the contract and SHALL stay stable; `message` is prose for people and MUST NOT be relied on. `details` is an object, omitted when the error carries none.

#### Scenario: Unknown template

- **WHEN** a client requests a template id that does not exist
- **THEN** the response is `404` with body `{ "error": { "code": "NotFound", "message": "…", "details": { "kind": "template", "id": "<id>" } } }`

### Requirement: Error codes

Each `code` SHALL be returned with exactly the status below, and with the listed `details` keys when it has any. `InvalidRequest`, `TemplateInvalid`, `UnsupportedLayoutItem` and `Upstream` SHALL always carry `details.reason`; no other code carries one.

| Code | Status | When | `details` |
|---|---|---|---|
| `InvalidRequest` | 400 | The request is malformed or carries or names something invalid. | `reason` |
| `Unauthorized` | 401 | Authentication is required and absent or invalid. | |
| `Forbidden` | 403 | The origin check failed, or an authentication route was called while authentication is disabled. | |
| `NotFound` | 404 | Unknown `/api/*` route, or a named record that does not exist. | `kind`, `id` |
| `Conflict` | 409 | The request conflicts with stored state (a template or printer id already taken, setup already done, existing username, deleting the last user or one's own account). | |
| `PayloadTooLarge` | 413 | The request body exceeds the endpoint's size limit, or a batch exceeds the label cap. | |
| `UnsupportedMediaType` | 415 | `Content-Type` is absent or not a JSON media type on a JSON endpoint. | |
| `TemplateInvalid` | 422 | The template, not the request, is at fault. | `reason` |
| `UnsupportedLayoutItem` | 422 | This label's data cannot be laid out or rendered, a value it needs being absent included. | `reason` |
| `BatchInvalid` | 422 | One or more labels of a batch failed. | `failures` |
| `Internal` | 500 | The service failed for a reason not attributable to the request; the cause goes to the log. | |
| `Upstream` | 502 | A connection's upstream failed. | `reason` |

`NotFound`'s `details.kind` SHALL be one of `route`, `template`, `printer`, `connection`, `setting`, `user` and `token`, and `details.id` the identifier as requested (the path for `route`).

#### Scenario: Unknown API route

- **WHEN** a client requests `GET /api/nonexistent`
- **THEN** the response is `404` with `error.code` `NotFound`, `error.details.kind` `route` and `error.details.id` `/api/nonexistent`

#### Scenario: Upstream rate limit

- **WHEN** a connection's upstream answers a browse with `429`
- **THEN** the response is `502` with `error.code` `Upstream` and `error.details.reason` `rate_limited`

### Requirement: Reason slugs

`details.reason` SHALL be exactly one of the slugs below, raised only with the code shown.

| Slug | Code | Meaning |
|---|---|---|
| `template_validation_failed` | TemplateInvalid | The template did not parse or failed validation; the message names the path of the offending key. |
| `missing_field` | UnsupportedLayoutItem | A parameter an active item reads has no value and no default; `details.field` names it. |
| `reference_unresolved` | TemplateInvalid | A part of the template that does not come from a label does not resolve against the request's snapshot: a `vars` key the store lacks, a datetime format name the `datetime_formats` setting lacks, or a tokened default whose value its parameter's type refuses (`parameters`); `details.field` names the key, the format name or the parameter. |
| `qr_payload_invalid` | UnsupportedLayoutItem | A `qr` value cannot be encoded, for example because it is too long. |
| `coord_out_of_frame` | UnsupportedLayoutItem | A resolved coordinate lies below or left of the frame. |
| `item_out_of_frame` | UnsupportedLayoutItem | A resolved item box extends beyond the frame. |
| `line_endpoint_out_of_frame` | UnsupportedLayoutItem | A resolved line endpoint lies beyond the frame. |
| `line_degenerate` | UnsupportedLayoutItem | A line's start and end resolve to the same point. |
| `edge_rect_inverted` | UnsupportedLayoutItem | At render, a `to` resolves below or left of its `at`. |
| `size_invalid` | UnsupportedLayoutItem | At render, a parameter-supplied extent is not greater than 0, or a resolved extent is negative. |
| `text_does_not_fit` | UnsupportedLayoutItem | A `text` does not fit its box under its `overflow` policy. |
| `image_format_unsupported` | UnsupportedLayoutItem | The image's MIME type or file extension is not supported. |
| `image_data_invalid` | UnsupportedLayoutItem | Inline image data is not a usable base64 `data:` URI. |
| `image_asset_missing` | UnsupportedLayoutItem | The referenced asset file does not exist. |
| `image_asset_unreadable` | UnsupportedLayoutItem | The asset file exists but cannot be read. |
| `image_asset_path_escapes` | UnsupportedLayoutItem | The asset path resolves outside the assets directory. |
| `assets_dir_unavailable` | UnsupportedLayoutItem | The server's assets directory cannot be resolved. |
| `dimension_exceeds_limit` | UnsupportedLayoutItem | A resolved label dimension is not finite, not positive, or exceeds 1000 mm. |
| `json_malformed` | InvalidRequest | The body cannot be deserialized into the endpoint's type; `details.error` carries the parser's message. |
| `request_body_invalid` | InvalidRequest | The body is unreadable, or is rejected for a reason no other body slug names. |
| `path_param_invalid` | InvalidRequest | A path segment has malformed percent-encoding, is not UTF-8, or does not deserialize into the declared type. |
| `param_value_invalid` | InvalidRequest | A parameter value a label supplies is not a form its type accepts, lies outside its `min`/`max`, is not among an `enum`'s `values`, or is not a `font_weight` an author could write; `details.param` names the parameter and, for a `list` element, `details.element` its zero-based position. |
| `data_key_unknown` | InvalidRequest | A `data` key names no declared parameter. |
| `field_not_applicable` | InvalidRequest | A batch field was sent where it does not apply: `start_slot` for a `single` template, or `format` for a `sheet` template. |
| `format_unknown` | InvalidRequest | `format` is not `png` or `pdf`. |
| `format_unsupported` | InvalidRequest | The endpoint does not serve the template's format: a `sheet` template sent to `POST /api/render/label`. |
| `batch_empty` | InvalidRequest | The batch contains no labels. |
| `start_slot_out_of_range` | InvalidRequest | `start_slot` is not below the sheet's slot count. |
| `color_mode_unknown` | InvalidRequest | `color_mode` is not `color` or `bilevel`. |
| `resolution_invalid` | InvalidRequest | `resolution` is not a positive integer in range. |
| `bilevel_requires_png` | InvalidRequest | `bilevel` was requested for output other than PNG. |
| `width_bounds_inverted` | InvalidRequest | The resolved `format.width.max` is below `format.width.min`. |
| `template_id_invalid` | InvalidRequest | A template id is empty or contains characters other than letters, digits, `-` and `_`. |
| `printer_id_invalid` | InvalidRequest | A printer id is empty or contains disallowed characters. |
| `printer_invalid` | InvalidRequest | A printer record has a blank name or a field that breaks its rule. |
| `variable_key_invalid` | InvalidRequest | A variable key is empty or contains disallowed characters. |
| `setting_value_invalid` | InvalidRequest | A setting's value fails its type or range rule. |
| `datetime_pattern_invalid` | InvalidRequest | A datetime format pattern is not valid. |
| `connector_unknown` | InvalidRequest | The request names a connector that does not exist. |
| `connection_connector_missing` | InvalidRequest | A stored connection references a connector that is no longer registered. |
| `credential_required` | InvalidRequest | The connector requires a credential and none, or `""`, was supplied. |
| `base_url_invalid` | InvalidRequest | `base_url` is not a valid `http` or `https` URL. |
| `public_url_invalid` | InvalidRequest | `public_url` is not a valid `http` or `https` URL. |
| `filter_invalid` | InvalidRequest | A connector browse filter is invalid. |
| `row_key_invalid` | InvalidRequest | A row key to materialize is empty, contains `/` or starts with `.`. |
| `row_limit_exceeded` | InvalidRequest | A materialize request asks for more than 200 rows. |
| `username_empty` | InvalidRequest | The username is empty. |
| `password_empty` | InvalidRequest | The password is empty. |
| `auth` | Upstream | The upstream refused the stored credential or access. |
| `unreachable` | Upstream | The upstream could not be reached. |
| `rate_limited` | Upstream | The upstream rate-limited the request. |
| `bad_response` | Upstream | The upstream answered with an unexpected status or shape, or a response over the size cap. |

#### Scenario: Reason accompanies its code

- **WHEN** a label supplies `2.7` for an `integer` parameter `copies`
- **THEN** the response is `400` with `error.code` `InvalidRequest`, `error.details.reason` `param_value_invalid` and `error.details.param` `copies`

### Requirement: Per-label batch failures

A request that renders several labels and has any failing label SHALL return `422 BatchInvalid` with `details.failures`: one entry per failing label, in ascending `index` order, each the error object `{ code, message, details? }` plus `index`, the zero-based label index.

#### Scenario: Two labels fail differently

- **WHEN** a batch's label 0 has an unknown `data` key and label 2 omits a parameter an active item reads that has no default
- **THEN** `details.failures` is `[{ "index": 0, "code": "InvalidRequest", "message": "…", "details": { "reason": "data_key_unknown" } }, { "index": 2, "code": "UnsupportedLayoutItem", "message": "…", "details": { "reason": "missing_field", "field": "<name>" } }]`

### Requirement: One fault per answer

A request with several faults SHALL report one of them, and which one is unspecified, except that authentication and origin checks come first (see "Admission before body and path mapping") and request-level faults (unknown template or printer, label cap, empty batch, a field that does not apply, a template whose snapshot does not resolve) SHALL be decided before any label of a batch is judged. A failing label SHALL get one failure entry reporting one of its faults.

#### Scenario: A request fault outranks label faults

- **WHEN** a batch names an unknown printer and one of its labels carries an undeclared key
- **THEN** the response is `404 NotFound` with `details.kind` `printer`, not `BatchInvalid`

#### Scenario: A template fault outranks label faults

- **WHEN** a batch's template reads `{vars.base}`, the store holds no `base`, and one of its labels carries an undeclared key
- **THEN** the response is `422 TemplateInvalid` with reason `reference_unresolved`, not `BatchInvalid`

### Requirement: Admission before body and path mapping

Authentication and origin checks SHALL run before the request body or path is read, so a request they reject gets their `401` or `403` whatever its body contains.

#### Scenario: Malformed body without credentials

- **WHEN** a client sends invalid JSON to a protected endpoint with no credentials
- **THEN** the response is `401 Unauthorized`, not `json_malformed`

### Requirement: Request body rejection

Every endpoint that reads a JSON body SHALL map a body it cannot accept as follows, always in the error envelope:

| Condition | Status | Code | Reason |
|---|---|---|---|
| Not valid JSON, or valid JSON that does not deserialize into the endpoint's type (wrong type, missing key, unlisted key, a key written as `null`) | 400 | `InvalidRequest` | `json_malformed` |
| Body exceeds the endpoint's limit (about 2 MiB) | 413 | `PayloadTooLarge` | none |
| `Content-Type` absent, unparseable, or not a JSON media type | 415 | `UnsupportedMediaType` | none |
| Body unreadable, or any other rejection | 400 | `InvalidRequest` | `request_body_invalid` |

A JSON media type is `application/json` or any `application/<subtype>+json`. Values inside a label's `data` are parameter values, judged by `parameters`, so `null` there is not a key written as `null`.

#### Scenario: Wrong-shaped body

- **WHEN** a client sends `{"connector":42,"name":"home","base_url":"http://hb.lan:7745"}` to `POST /api/connections`, where `connector` is a string
- **THEN** the response is `400 InvalidRequest` with `details.reason` `json_malformed` and a non-empty `details.error`

#### Scenario: Suffixed JSON media type

- **WHEN** a client sends a valid body with `Content-Type: application/problem+json`
- **THEN** the body is accepted as JSON

#### Scenario: Non-JSON media type

- **WHEN** a client sends a body to a JSON endpoint with `Content-Type: text/plain`
- **THEN** the response is `415 UnsupportedMediaType`

### Requirement: Rejected body stays out of the log

When the service rejects a body it SHALL NOT log the body or the parser message quoting it; it logs the classification and status only. The response's `details.error` still carries the parser message.

#### Scenario: Malformed login body

- **WHEN** a client sends `{"username":"admin","password":12345}` to `POST /api/auth/login`
- **THEN** no log record for the rejection contains `12345`
- **AND** the response carries `json_malformed` and `details.error`

### Requirement: Path parameter rejection

A path segment that cannot be decoded or deserialized into the declared type SHALL return `400 InvalidRequest` with `details.reason` `path_param_invalid`. A path rejection that indicates a service defect (handler and route disagree on parameters, or parameters never reached the handler) SHALL return `500 Internal` and MUST NOT be reported as `path_param_invalid`.

#### Scenario: Segment is not UTF-8

- **WHEN** a client requests `GET /api/templates/%FF/source`
- **THEN** the response is `400 InvalidRequest` with `details.reason` `path_param_invalid`

#### Scenario: Handler and route disagree

- **WHEN** a handler declaring a different number of path parameters than its route is reached
- **THEN** the response is `500 Internal`

### Requirement: Panics

A panic that unwinds out of a handler, extractor or middleware before the response is produced, on any route inside or outside `/api`, SHALL be answered with `500`, `error.code` `Internal`, `error.message` `internal server error` and no `details`. The response MUST NOT carry any part of the panic payload. The service SHALL log the payload at error level when it is a string, or the fixed marker `unreadable panic payload` otherwise, and SHALL keep serving requests. A panic while the response body streams, in a detached task, or that aborts the process is outside this guarantee.

#### Scenario: Handler panics

- **WHEN** a handler panics with message `boom 42`
- **THEN** the response is `500` with `error.code` `Internal` and `error.message` `internal server error`
- **AND** the body does not contain `boom 42`, and an error-level log record does

#### Scenario: Service survives

- **WHEN** a request ends in a panic
- **THEN** a later unrelated request is answered normally
