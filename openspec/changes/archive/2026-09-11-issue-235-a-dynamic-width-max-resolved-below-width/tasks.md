## 1. HTTP tests first, observed red

- [x] 1.1 In `src/lib.rs`, beside the `line_spacing_param_invalid` HTTP tests, add a test that writes the issue's repro template (`params: [{ name: max_width, type: length, default: 120.0 }]`, `format: { type: single, width: { min: 10.0, max: "{max_width}" }, height: 18.1 }`, a small text item) to the test config dir and sends `POST /api/render/label?format=png` with `data.max_width: 5`; assert status `400`, `error.code` `InvalidRequest`, `error.details.reason` `width_bounds_inverted`, and that the message contains `max_width`, `5` and `10`. Then send `GET /api/health` on the same app and assert `200`.
- [x] 1.2 In the same test module, add the ordered cases against the repro template: `max_width: 10` asserts `200` (equal bounds render), and `max_width: 0` asserts `422`, `UnsupportedLayoutItem`, `dimension_exceeds_limit` (dimension check precedes the ordering check).
- [x] 1.3 Add a test for the other reference shapes: a template with `width: { min: "{min_width}", max: 60.0 }` rendered with `min_width: 80` asserts `400 width_bounds_inverted` with a message containing `min_width`, `80` and `60`; a template with `width: { min: "{lo}", max: "{hi}" }` rendered with `lo: 30, hi: 20` asserts `400 width_bounds_inverted` with a message containing `lo`, `hi`, `30` and `20`.
- [x] 1.4 Add a test for defaults: a template with `width: { min: 10.0, max: "{max_width}" }` where `max_width` declares `default: 5.0` and the layout fits a 5-wide frame; a render omitting `max_width` asserts `400 width_bounds_inverted` naming `max_width`, `5` and `10`; a render supplying `max_width: 20` asserts `200`.
- [x] 1.5 Add a test for precedence over measurement: a template with `width: { min: 10.0, max: "{max_width}" }` and a `text` item whose width is `"{w}"` on a `length` parameter `w` with a positive default; `max_width: 5, w: 0` asserts `400 width_bounds_inverted` (not `size_invalid`); `max_width: 40, w: 0` asserts `422`, `UnsupportedLayoutItem`, `size_invalid`.
- [x] 1.6 Add a batch test: `POST /api/batch` with two labels of the repro template, `max_width: 40` then `max_width: 5`; assert status `422`, a JSON content type, `error.code` `BatchInvalid`, and `error.details.failures` holding exactly one entry with `index` `1`, `code` `InvalidRequest`, `reason` `width_bounds_inverted`.
- [x] 1.7 Run `cargo test` and record, in the test's doc comment or the run log, that the inverted-bounds assertions fail against the unchanged tree with status `500` (the panic envelope) and that the precedence test's inverted case fails with `422 size_invalid`. Do not tick this task on a run where the new tests pass before the guard exists.

## 2. Reason and constructor

- [x] 2.1 In `src/reason.rs`, add `WidthBoundsInverted => "width_bounds_inverted"` under the `InvalidRequest` group, after `LineSpacingParamInvalid`.
- [x] 2.2 In `src/errors.rs`, add `AppError::width_bounds_inverted(min: f32, max: f32, unit: &str, min_param: Option<&str>, max_param: Option<&str>)` next to `line_spacing_param_invalid`, building `Self::invalid_request(Reason::WidthBoundsInverted, message)` where the message names `format.width.max <max> <unit>` and `format.width.min <min> <unit>`, each followed by `(parameter '<name>')` exactly when that bound was a reference, and states that `max` is below `min`. No structured `details` beyond `reason`.
- [x] 2.3 Run `cargo test reason` and confirm the reason-completeness test passes with the new slug documented by this change's delta at `openspec/changes/issue-235-a-dynamic-width-max-resolved-below-width/specs/layout-sizing/spec.md`.

## 3. The guard

- [x] 3.1 In `src/render/mod.rs` `compile_label_source`, immediately after the two `check_dimension_limit` calls on `min_w` and `max_w` and before `RenderContext::new` / `measure_items`, return `Err(AppError::width_bounds_inverted(...))` when `min_w > max_w` (strict), passing the unit and, for each of `min` and `max`, the parameter name when the `DynamicValue` is a `Ref` and `None` when it is a `Literal`. Change nothing else on that path: `resolve_dynamic_value_f32`, `check_dimension_limit`, the measure pass and the clamp stay as they are.
- [x] 3.2 Run `cargo test` and confirm every test from section 1 passes and no existing test changed outcome.

## 4. Gates

- [x] 4.1 `cargo fmt --check`
- [x] 4.2 `cargo clippy --all-targets --all-features`
- [x] 4.3 `cargo test`
