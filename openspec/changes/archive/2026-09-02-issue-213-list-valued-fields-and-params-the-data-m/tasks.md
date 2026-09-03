## 1. The type in the two-stage parser

- [x] 1.1 Add `RawParamType::List` in `src/raw.rs`, leaving `RawParamSpec`'s presence-preserving fields (`min`, `max`, `multiline`, `values`, `time`) as they are, since `list` refuses them on presence exactly as `datetime` does.
- [x] 1.2 Add `ParamType::List` and `ParamValue::List(Vec<String>)` in `src/models.rs`. Check the `#[serde(untagged)]` order on `ParamValue`: only `List` deserializes a JSON array, so it is unambiguous.
- [x] 1.3 Add the `List` branch to `TryFrom<RawParamSpec> for ParamSpec` in `src/convert.rs`: refuse `min`, `max`, `multiline`, `values` and `time` when the key is written, an explicit YAML null included, each naming the parameter and the attribute. `format` is already refused on every type.
- [x] 1.4 Convert a `list` `default:` in `src/convert.rs`: a YAML sequence of string scalars becomes `ParamValue::List`; a non-null scalar or a mapping is refused naming the parameter; a non-string element (number, boolean, null, sequence, mapping) is refused naming the parameter and the first offending element's position; an explicit YAML null is an absent default; `[]` is a present empty list.
- [x] 1.5 Refuse a sequence `default:` on every type but `list` in `src/convert.rs`, naming the parameter, rather than letting `convert_raw_default`'s wildcard hold it as `format!("{other:?}")`.
- [x] 1.6 Register the new `ParamType` and `ParamValue` variants in `src/openapi.rs`.
- [x] 1.7 Unit-test the conversions: a `list` with a default and a description loads; each forbidden attribute is refused, including one written as an explicit null; a scalar default, a mapping default and each non-string element kind are refused with the parameter named; `default: []` and `default:` written empty are distinguished; a sequence default on a `string` is refused.

## 2. The token grammar

- [x] 2.1 Replace `Token.format: Option<&str>` with `Token.reader: Option<Reader>` in `src/interpolation.rs`, where `Reader` is `Format(&str) | Join(&str)`, and fix the resulting compile errors at every consumer.
- [x] 2.2 Make `parse` structural rather than colon-counting: split at the first `:`, then read the reader as either a bare name matching `^[a-zA-Z0-9_-]+$` or a `join('<sep>')` call. `{x:a:b}` must still be refused, now because `a:b` is neither.
- [x] 2.3 Parse the separator literal: it runs from the first `'` after the `(` to the next `'`, may be empty, may contain `:`, and may not contain `'`. Refuse a further `'` before the `)`, whitespace outside the quotes, and anything after the closing `)`, each naming the token.
- [x] 2.4 Leave `scan_tokens` unchanged, and add a test asserting its current behaviour on `{tags:join('}')}`, `{tags:join('{')}` and `{tags:join('{{')}`, since the first two must yield a malformed token and the third must yield none.
- [x] 2.5 Unit-test `parse`: `{tags:join(', ')}` and `{tags:join('')}` and `{tags:join(' : ')}` parse; `{sys.now:join}` parses as an ordinary format name, because a bare reader is a format name whatever it spells; `{tags:join}`, `{sys.now:long_date(', ')}`, `{tags:join(''')}`, `{tags:join( ', ' )}`, `{tags:join(a)}` and `{tags:join(', ')x}` are refused.

## 3. Load-time validation

- [x] 3.1 Thread a layout path through `validate_item_references` in `src/templates.rs`, extending it for each child, and pass it to `validate_when_references`. Change no existing message: only the two new refusals below consume it.
- [x] 3.2 Add the `List` arm to `check_param_ref` so a `list` is refused as a numeric, dimension, `font_weight` or colour reference, naming the parameter and the context.
- [x] 3.3 Refuse an `image` item's `name:` that names a declared `list`, naming the parameter and the item's layout path.
- [x] 3.4 Extend `validate_interpolated_string`: a bare token naming a declared `list` is refused; a bare reader on a declared `list` is refused with a message saying a list is read through `join('<separator>')`; a join on anything but a bare token naming a declared `list` is refused; a bare reader name other than `join` carrying an argument is refused. Each names the token.
- [x] 3.5 Refuse a `when:` key naming a declared `list` in `validate_when_references`, naming the key and the item's layout path, and leave the undeclared-key refusal's message exactly as it is.
- [x] 3.6 Test the load refusals through the registry so each quarantines its file while the server still starts, and test that the same content through `PUT /api/templates/{id}` is `422 TemplateInvalid` with reason `template_validation_failed`.
- [x] 3.7 Pin the behaviour these rules must not change: `when:` written with an explicit YAML null leaves the item unconditional and does not reach the empty-map refusal; `when: {}`, a blank key and a blank value stay refused; an undeclared `when:` key stays refused with today's message and no layout path.

## 4. Request values, resolution and render

- [x] 4.1 Add `field_value_not_scalar` to `src/reason.rs` and an `AppError` path for it under `UnsupportedLayoutItem` in `src/errors.rs`.
- [x] 4.2 Add the `List` arm to `coerce_param_value` in `src/render/mod.rs`: a JSON array of strings is accepted; a non-array, and an array with a non-string element, are `400 InvalidRequest` with reason `request_body_invalid` naming the parameter and, for an element, its position. Treat JSON `null` as an omission.
- [x] 4.3 Change `coerce_param_value`'s `String` arm to refuse a JSON array with the same `400 InvalidRequest` and `request_body_invalid` shape its numeric siblings use, and add tests pinning that `boolean`, `integer`, `number`, `length`, `enum` and `datetime` keep exactly the code, reason and message they refuse an array with today.
- [x] 4.4 Render a join in `interpolate` (`src/render/helpers.rs`): concatenate the resolved list's elements with the separator between consecutive ones, so one element renders itself and zero elements render the empty string.
- [x] 4.5 Refuse a JSON array reaching a scalar slot in `interpolate` with `422 UnsupportedLayoutItem` and reason `field_value_not_scalar`, naming the field, and leave a JSON object stringifying to its JSON text as it does today.
- [x] 4.6 Apply the same refusal to both `image` `name:` bindings (`src/render/mod.rs`, the measure pass and the render pass), decided before `parse_image_data_uri` so the failure reports the value's shape rather than a malformed data URI.
- [x] 4.7 HTTP-test the render path: a supplied list joins; `[]` joins to nothing; `null` falls back to the declared default; a non-array and a non-string element are `400`; an absent list an active item joins is `422 MissingField`; an undeclared array printed by a token, and one bound by an `image` `name:`, are `422 UnsupportedLayoutItem` with `field_value_not_scalar`; an undeclared array nothing reads renders.
- [x] 4.8 HTTP-test the batch path: one label carrying a refused list value fails the whole batch as `422 BatchInvalid`, with a `details.failures` entry naming that label's index, code and reason, and no PDF, ZIP or print job produced.

## 5. Reported inputs, defaults and previews

- [x] 5.1 Add `InputControl::List` in `src/models.rs`, register it in `src/openapi.rs`, and add the `ParamType::List` arm to the control decision in `src/templates.rs` so a declared `list` reports control `list` with no `values`, `min`, `max` or `unit`.
- [x] 5.2 Add the `InputControl::List` arm to `placeholder_data` in `src/templates.rs`, filling a one-element list holding the entry's own name.
- [x] 5.3 Add an explicit `serde_json::Value::Array` arm to `json_to_param_value` in `src/render/mod.rs` mapping to `ParamValue::List`, since its wildcard arm would otherwise publish a list default as a string and would not fail to compile.
- [x] 5.4 HTTP-test `GET /api/templates/{id}` on the **serialized** body: `params.tags.type` is `"list"`, and `param_defaults.tags.resolved` is the JSON array `["CONSUMABLE"]` rather than a string. An assertion below the serializer would pass against the defect 5.3 fixes.
- [x] 5.5 Test the input list: a declared `list` appears with control `list`, its resolved array as `default` and `required: false`; one declaring no default is `required: true` with no `default`; an undeclared name is never `list`.
- [x] 5.6 Test the thumbnail: a template joining a `list` with no default renders and reads the parameter's own name; with `default: [CONSUMABLE, KIDS]` it reads `CONSUMABLE, KIDS`; with `default: []` it renders that text empty.

## 6. The UI's declared types and its tolerance for a control it cannot draw

- [x] 6.1 Correct `ui/src/api/types.ts`: `ParamValue` admits `string[]`, `ParamSpec["type"]` admits `"list"`, and the `InputControl` union admits `"list"`.
- [x] 6.2 Return no control from `ui/src/components/ParamInput.tsx` for `control === "list"`, before the final text-input fall-through, so the print form omits the control instead of collecting text the service will refuse.
- [x] 6.3 Skip a `list` entry when building columns in the CSV import grid and the connector grid (`ui/src/pages/Import.tsx`, `ui/src/pages/Connect.tsx`, and the cell rendering in `ui/src/components/LabelGrid.tsx`), on the same rule and for the same reason.
- [x] 6.4 Test that a reported `list` input renders no control and breaks neither the print form nor either grid, and that every other entry in the same form still renders.

## 7. Gates

- [x] 7.1 Run `cargo fmt`.
- [x] 7.2 Run `cargo clippy --all-targets --all-features` and fix the root cause of anything it reports, never silencing it with an `allow`.
- [x] 7.3 Run `cargo test`.
- [x] 7.4 Run `npm run lint`, `npm run test` and `npm run build` in `ui/`.
