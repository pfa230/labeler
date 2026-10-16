## ADDED Requirements

### Requirement: Service error codes

Each `code` SHALL be returned with exactly the status below, and with the listed `details` keys when it has any. `InvalidRequest`, `TemplateInvalid`, and `UnsupportedLayoutItem` SHALL always carry `details.reason`; no other code carries one.

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
| `UnsupportedLayoutItem` | 422 | This label's data cannot be laid out or rendered. | `reason` |
| `BatchInvalid` | 422 | One or more labels of a batch failed. | `failures` |
| `Internal` | 500 | The service failed for a reason not attributable to the request; the cause goes to the log. | |

`NotFound`'s `details.kind` SHALL be one of `route`, `template`, `printer`, `setting`, `user` and `token`, and `details.id` the identifier as requested (the path for `route`).

#### Scenario: Unknown API route

- **WHEN** a client requests `GET /api/nonexistent`
- **THEN** the response is `404` with `error.code` `NotFound`, `error.details.kind` `route` and `error.details.id` `/api/nonexistent`

## MODIFIED Requirements

### Requirement: Reason slugs

`details.reason` SHALL be exactly one of the slugs below, raised only with the code shown.

| Slug | Code | Meaning |
|---|---|---|
| `template_validation_failed` | TemplateInvalid | The template did not parse or failed validation; the message names the path of the offending key. |
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
| `chain_and_continue_unsupported` | InvalidRequest | Explicit chain-and-continue printing was requested without the printer advertising the required finishing template support. |
| `variable_key_invalid` | InvalidRequest | A variable key is empty or contains disallowed characters. |
| `setting_value_invalid` | InvalidRequest | A setting's value fails its type or range rule. |
| `datetime_pattern_invalid` | InvalidRequest | A datetime format pattern is not valid. |
| `username_empty` | InvalidRequest | The username is empty. |
| `password_empty` | InvalidRequest | The password is empty. |

#### Scenario: Reason accompanies its code

- **WHEN** a label supplies `2.7` for an `integer` parameter `copies`
- **THEN** the response is `400` with `error.code` `InvalidRequest`, `error.details.reason` `param_value_invalid` and `error.details.param` `copies`

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

- **WHEN** a client sends `{"value":42}` to `PUT /api/variables/example`, where `value` is a string
- **THEN** the response is `400 InvalidRequest` with `details.reason` `json_malformed` and a non-empty `details.error`

#### Scenario: Suffixed JSON media type

- **WHEN** a client sends a valid body with `Content-Type: application/problem+json`
- **THEN** the body is accepted as JSON

#### Scenario: Non-JSON media type

- **WHEN** a client sends a body to a JSON endpoint with `Content-Type: text/plain`
- **THEN** the response is `415 UnsupportedMediaType`

## REMOVED Requirements

### Requirement: Error codes

**Reason**: Replaced by the service error vocabulary.
**Migration**: Use Service error codes.
