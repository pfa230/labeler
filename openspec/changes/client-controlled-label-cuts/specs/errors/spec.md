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

### Requirement: One fault per answer

A request with several faults SHALL report one of them, and which one is unspecified, except that authentication and origin checks come first (see "Admission before body and path mapping") and request-level faults (unknown template or printer, label cap, empty batch, a field that does not apply, unsupported continuation (`printing`), a template whose snapshot does not resolve) SHALL be decided before any label of a batch is judged. A failing label SHALL get one failure entry reporting one of its faults.

#### Scenario: A request fault outranks label faults

- **WHEN** a batch names an unknown printer and one of its labels carries an undeclared key
- **THEN** the response is `404 NotFound` with `details.kind` `printer`, not `BatchInvalid`

#### Scenario: A template fault outranks label faults

- **WHEN** a batch's template reads `{vars.base}`, the store holds no `base`, and one of its labels carries an undeclared key
- **THEN** the response is `422 TemplateInvalid` with reason `reference_unresolved`, not `BatchInvalid`
