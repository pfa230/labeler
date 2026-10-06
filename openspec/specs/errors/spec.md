# Errors

## Purpose

Defines how every failure is reported: the JSON error envelope, the `code` vocabulary and HTTP statuses, the `details.reason` slugs, per-label batch failures, request body and path rejections, and panics.

## Requirements

### Requirement: Error envelope

Every error response SHALL have `Content-Type: application/json` and the body `{ "error": { "code", "message", "details"? } }`. `code` and `details.reason` are the contract and SHALL stay stable; `message` is prose for people and MUST NOT be relied on. `details` is an object, omitted when the error carries none.

#### Scenario: Unknown template

- **WHEN** a client requests a template id that does not exist
- **THEN** the response is `404` with body `{ "error": { "code": "TemplateNotFound", "message": "…", "details": { "template": "<id>" } } }`

### Requirement: Error codes

Each `code` SHALL be returned with exactly the status below, and with the listed `details` keys when it has any. `InvalidRequest`, `UnsupportedLayoutItem`, `TemplateInvalid` and `RenderFailed` SHALL always carry `details.reason`; no other code carries one.

| Code | Status | When | `details` |
|---|---|---|---|
| `InvalidRequest` | 400 | The request is malformed or names something invalid. | `reason` |
| `InvalidFilter` | 400 | A connector browse filter is invalid. | |
| `BudgetExceeded` | 400 | A connector request asks for too many rows. | |
| `Unauthorized` | 401 | Authentication is required and absent or invalid. | |
| `Forbidden` | 403 | The origin check failed, or an authentication route was called while authentication is disabled. | |
| `TemplateNotFound` | 404 | Unknown template id. | `template` |
| `PrinterNotFound` | 404 | Unknown printer id. | `printer` |
| `SettingNotFound` | 404 | Unknown setting key. | `setting` |
| `NotFound` | 404 | Unknown `/api/*` route, or a named group or connection that does not exist. | `path` |
| `Conflict` | 409 | The request conflicts with stored state (non-empty group, occupied name, setup already done, existing username, deleting the last user or one's own account). | |
| `PrinterExists` | 409 | A printer with that id already exists. | `printer` |
| `TemplateIdCollision` | 409 | More than one file declares the template id. | `template`, `files` (bare filenames) |
| `MediaMismatch` | 409 | The template's media width differs from the printer's loaded media by more than 1 mm. | |
| `PreconditionFailed` | 412 | A template write sent `If-None-Match: *` and the id or destination file already exists. | |
| `BatchTooLarge` | 413 | A batch exceeds the label cap. | `count`, `max` |
| `PayloadTooLarge` | 413 | The request body exceeds the endpoint's size limit. | |
| `UnsupportedMediaType` | 415 | `Content-Type` is absent or not a JSON media type on a JSON endpoint. | |
| `InvalidEnumValue` | 422 | A value is not among an `enum` parameter's allowed values. | `selection`, `allowed` |
| `MissingField` | 422 | A value the render needs is absent: an omitted parameter with no default, an unknown variable, or an unknown datetime format name. | `field` |
| `UnsupportedLayoutItem` | 422 | A layout item cannot be resolved or rendered for this request. | `reason` |
| `TemplateInvalid` | 422 | The template, not the request, is at fault. | `reason` |
| `UnsupportedFormat` | 422 | The endpoint does not fit the template's format, or a format is incomplete or uses an unknown unit. | |
| `BatchInvalid` | 422 | One or more labels of a batch failed. | `failures` |
| `PrinterInvalid` | 422 | A printer's driver kind or configuration is invalid. | |
| `RateLimited` | 429 | The connector's upstream rate-limited the request. | |
| `RenderFailed` | 500 | The service failed to render, encode or store something. | `reason` |
| `Internal` | 500 | The service failed for a reason not attributable to the request. | |
| `ConnectorAuthFailed` | 502 | Upstream authentication failed. | |
| `ConnectorForbidden` | 502 | Upstream refused access. | |
| `ConnectorUnreachable` | 502 | The upstream could not be reached. | |
| `UpstreamSchemaMismatch` | 502 | The upstream answered in an unexpected shape. | |
| `Upstream` | 502 | Any other upstream failure. | |

#### Scenario: Unknown API route

- **WHEN** a client requests `GET /api/nonexistent`
- **THEN** the response is `404` with `error.code` `NotFound` and `error.details.path` `/api/nonexistent`

#### Scenario: Unreasoned code has no reason

- **WHEN** a render fails with `MissingField`
- **THEN** `error.details` carries `field` and no `reason`

### Requirement: Reason slugs

`details.reason` SHALL be exactly one of the slugs below, raised only with the code shown. Where a slug lists several codes, the meaning says which applies.

| Slug | Code | Meaning |
|---|---|---|
| `template_parse_failed` | TemplateInvalid | The template YAML did not parse. |
| `template_validation_failed` | TemplateInvalid | The template parsed but failed validation, including every load-time layout refusal. |
| `template_duplicate_id` | TemplateInvalid | Two templates on disk declare the same id. |
| `template_group_invalid` | TemplateInvalid, InvalidRequest | A group name or path is not a legal group: `InvalidRequest` for the group named in a group endpoint's URL, `TemplateInvalid` otherwise. |
| `template_group_case_conflict` | TemplateInvalid | A group differs from an existing group only by letter case. |
| `template_group_unsafe_path` | TemplateInvalid, InvalidRequest, RenderFailed | A directory on a group's path is a symbolic link or not a directory: `TemplateInvalid` for a group the request supplies on a template write, `InvalidRequest` for the group in a group endpoint's URL, `RenderFailed` when the request did not name the group. |
| `param_default_unresolvable` | TemplateInvalid | A declared parameter default cannot be resolved for this request; `details` also carries `param` and, when known, `token` and `value`. |
| `coord_out_of_frame` | UnsupportedLayoutItem | A resolved coordinate lies below or left of the frame. |
| `item_out_of_frame` | UnsupportedLayoutItem | A resolved item box extends beyond the frame. |
| `line_endpoint_out_of_frame` | UnsupportedLayoutItem | A resolved line endpoint lies beyond the frame. |
| `line_degenerate` | UnsupportedLayoutItem | A line's start and end resolve to the same point. |
| `edge_rect_inverted` | UnsupportedLayoutItem | At render, a `to` resolves below or left of its `at`. |
| `size_invalid` | UnsupportedLayoutItem | At render, a parameter-supplied extent is not greater than 0, or a resolved extent is negative. |
| `max_size_invalid` | UnsupportedLayoutItem | A `max_w` or `max_h` is not greater than 0. No code path raises it. |
| `intrinsic_size_undefined` | UnsupportedLayoutItem | An intrinsic size was needed at render and the content declares no extent on that axis, or its image dimensions cannot be read. |
| `text_does_not_fit` | UnsupportedLayoutItem | A `text` does not fit its box under its `overflow` policy. |
| `image_source_missing` | UnsupportedLayoutItem | An image item resolves neither `src` nor `name`. |
| `image_format_unsupported` | UnsupportedLayoutItem | The image's MIME type or file extension is not supported. |
| `image_data_invalid` | UnsupportedLayoutItem | Inline image data is not a usable base64 `data:` URI. |
| `image_asset_missing` | UnsupportedLayoutItem | The referenced asset file does not exist. |
| `image_asset_unreadable` | UnsupportedLayoutItem | The asset file exists but cannot be read. |
| `image_asset_path_escapes` | UnsupportedLayoutItem | The asset path resolves outside the assets directory. |
| `assets_dir_unavailable` | UnsupportedLayoutItem | The server's assets directory cannot be resolved. |
| `qr_error_correction_invalid` | UnsupportedLayoutItem | `error_correction` is not one of `L`, `M`, `Q`, `H`. |
| `dimension_exceeds_limit` | UnsupportedLayoutItem | A resolved label dimension is not finite, not positive, or exceeds the `max_label_dimension_mm` setting. |
| `circle_box_not_square` | UnsupportedLayoutItem | An active `shape: circle` container resolves a box that is not square. |
| `field_value_not_scalar` | UnsupportedLayoutItem | A value bound where only a scalar renders is an array; `details.field` names it. |
| `json_malformed` | InvalidRequest | The body cannot be deserialized into the endpoint's type; `details.error` carries the parser's message. |
| `request_body_invalid` | InvalidRequest | The body cannot be read, or a supplied parameter value cannot be coerced to its declared type. |
| `path_param_invalid` | InvalidRequest | A path segment has malformed percent-encoding, is not UTF-8, or does not deserialize into the declared type. |
| `start_slot_out_of_range` | InvalidRequest | `start_slot` is not below the sheet's slot count. |
| `start_slot_not_applicable` | InvalidRequest | `start_slot` was sent for a non-sheet template. |
| `batch_empty` | InvalidRequest | The batch contains no labels. |
| `format_unknown` | InvalidRequest | `format` is not `png` or `pdf`. |
| `format_not_applicable` | InvalidRequest | `format` was sent with `mode=print`. |
| `interpolation_syntax` | InvalidRequest | An interpolated string has an unterminated `{`, an unmatched `}`, or an invalid token. |
| `template_id_invalid` | InvalidRequest | A template id is empty or contains characters other than letters, digits, `-` and `_`. |
| `template_id_mismatch` | InvalidRequest | The template id in the body disagrees with the path. No code path raises it. |
| `template_group_mismatch` | InvalidRequest | A template replace sent a `group` other than the template's current group. |
| `unsupported_precondition` | InvalidRequest | A template write sent `If-None-Match` with a value other than `*`. |
| `printer_id_invalid` | InvalidRequest | A printer id is empty or contains disallowed characters. |
| `printer_id_mismatch` | InvalidRequest | The printer id in the body disagrees with the path. |
| `variable_key_invalid` | InvalidRequest | A variable key is empty or contains disallowed characters. |
| `setting_value_invalid` | InvalidRequest | A setting's value fails its type or range rule. |
| `datetime_pattern_invalid` | InvalidRequest | A datetime format pattern is not valid. |
| `datetime_param_invalid` | InvalidRequest | A supplied `datetime` parameter value cannot be parsed. |
| `color_param_invalid` | InvalidRequest | A parameter supplying a colour does not resolve to a valid colour. |
| `line_spacing_param_invalid` | InvalidRequest | A parameter-supplied `line_spacing` does not resolve to a finite value greater than 0. |
| `width_bounds_inverted` | InvalidRequest | The resolved `format.width.max` is below `format.width.min`. |
| `connector_unknown` | InvalidRequest | The request names a connector that does not exist. |
| `connector_immutable` | InvalidRequest | A connection update changes its `connector`. |
| `connection_connector_missing` | InvalidRequest | A stored connection references a connector that is no longer registered. |
| `connection_transform_invalid` | InvalidRequest | A connection's field transform is invalid. |
| `credential_required` | InvalidRequest | The connector requires a credential and none was supplied. |
| `base_url_invalid` | InvalidRequest | `base_url` is not a valid `http` or `https` URL. |
| `public_url_invalid` | InvalidRequest | `public_url` is not a valid `http` or `https` URL. |
| `csv_header_invalid` | InvalidRequest | The CSV header row is unparsable, or has empty or duplicate column names. |
| `csv_row_invalid` | InvalidRequest | A CSV data row cannot be parsed. |
| `csv_empty` | InvalidRequest | The CSV has a header but no data rows. |
| `csv_data_column_unknown` | InvalidRequest | A CSV data column names no declared parameter. |
| `data_key_unknown` | InvalidRequest | A `data` key names no declared parameter. |
| `mode_unknown` | InvalidRequest | `mode` is not `download` or `print`. |
| `printer_required` | InvalidRequest | `mode=print` was requested without a printer. |
| `copies_invalid` | InvalidRequest | `copies` is outside the allowed range. |
| `color_mode_unknown` | InvalidRequest | `color_mode` is not `color` or `bilevel`. |
| `resolution_invalid` | InvalidRequest | `resolution` is not a positive integer in range. |
| `bilevel_requires_png` | InvalidRequest | `bilevel` was requested for output other than PNG. |
| `username_empty` | InvalidRequest | The username is empty. |
| `password_empty` | InvalidRequest | The password is empty. |
| `typst_compile_failed` | RenderFailed | Typst failed to compile the generated source. |
| `typst_source_build_failed` | RenderFailed | Building the generated Typst source failed. |
| `typst_no_pages` | RenderFailed | Typst compiled but produced no pages. |
| `png_encode_failed` | RenderFailed | Encoding the rendered image to PNG failed. |
| `pdf_encode_failed` | RenderFailed | Encoding the document to PDF failed. |
| `item_has_no_source` | RenderFailed | An item reached rendering with no data source. No code path raises it. |
| `qr_generation_failed` | RenderFailed | The QR payload cannot be encoded. |
| `font_read_failed` | RenderFailed | The measurement font file cannot be read. |
| `font_parse_failed` | RenderFailed | The measurement font cannot be parsed. |
| `font_axis_missing` | RenderFailed | The measurement font lacks the `wght` or `opsz` variation axis. |
| `template_path_invalid` | RenderFailed | The resolved template file path is not usable. No code path raises it. |
| `template_write_failed` | RenderFailed | Writing a template file failed. |
| `template_missing_after_write` | RenderFailed | A template was written but is absent from the reloaded registry. |
| `template_delete_failed` | RenderFailed | Deleting a template file failed. |
| `template_registry_io` | RenderFailed | Reading the templates directory failed. |
| `zip_write_failed` | RenderFailed | Building the ZIP of rendered labels failed. |

#### Scenario: Reason accompanies its code

- **WHEN** a QR item sets `error_correction: X`
- **THEN** the render fails `422` with `error.code` `UnsupportedLayoutItem` and `error.details.reason` `qr_error_correction_invalid`

### Requirement: Per-label batch failures

A request that renders several labels and has any failing label SHALL return `422 BatchInvalid` with `details.failures`: one entry `{ index, code, reason?, message }` per failing label, in ascending `index` order, where `index` is the zero-based label index. `reason` MUST be present exactly when that entry's `code` carries one.

#### Scenario: Two labels fail differently

- **WHEN** a batch's label 0 has an unknown `data` key and label 2 omits a required parameter
- **THEN** `details.failures` is `[{ "index": 0, "code": "InvalidRequest", "reason": "data_key_unknown", … }, { "index": 2, "code": "MissingField", "message": "…" }]`, the second without `reason`

### Requirement: Admission before body and path mapping

Authentication and origin checks SHALL run before the request body or path is read, so a request they reject gets their `401` or `403` whatever its body contains.

#### Scenario: Malformed body without credentials

- **WHEN** a client sends invalid JSON to a protected endpoint with no credentials
- **THEN** the response is `401 Unauthorized`, not `json_malformed`

### Requirement: Request body rejection

Every endpoint that reads a JSON body SHALL map a body it cannot accept as follows, always in the error envelope:

| Condition | Status | Code | Reason |
|---|---|---|---|
| Not valid JSON, or valid JSON that does not deserialize into the endpoint's type (wrong type, missing key) | 400 | `InvalidRequest` | `json_malformed` |
| Body exceeds the endpoint's limit (64 KiB on `POST /api/print`, about 2 MiB elsewhere) | 413 | `PayloadTooLarge` | none |
| `Content-Type` absent, unparseable, or not a JSON media type | 415 | `UnsupportedMediaType` | none |
| Body unreadable, or any other rejection | 400 | `InvalidRequest` | `request_body_invalid` |

A JSON media type is `application/json` or any `application/<subtype>+json`.

#### Scenario: Wrong-shaped body

- **WHEN** a client sends `{"connector":42,"name":"home","base_url":"http://hb.lan:7745"}` to `PUT /api/connections/{id}`, where `connector` is a string
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
