## 1. Parse and domain model

- [x] 1.1 Add a `Ref(String)` variant to `RawLineSpacing` (`src/raw.rs:214-244`) and extend its
  hand-written `Deserialize`: a YAML number stays `Float`; a string becomes `Ref` only when it opens
  with exactly one `{`, closes with exactly one `}` and carries no inner brace, with the inner text
  trimmed so `"{ pitch }"` and `"{pitch}"` name the same parameter; every other scalar stays `Invalid`
  and keeps its existing message. `"1.2"`, `"1.2mm"`, `"1.2em"` and `"{{ pitch }}"` must all still
  reach `Invalid`. Do not swap the field to `DynamicValue<f32>`'s `Deserialize`, which would accept
  all four (design, "The raw field keeps its own deserializer").
- [x] 1.2 Change `LayoutItem::Text.line_spacing` in `src/models.rs:1070` to
  `Option<DynamicValue<f32>>`, keeping `skip_serializing_if = "Option::is_none"` so an item declaring
  nothing still omits the key and a reference reads back as `"{name}"` through `DynamicValue`'s
  existing `Serialize`.
- [x] 1.3 Carry the reference through the `TryFrom` in `src/convert.rs:411-437`: a `Float` keeps the
  existing finite-and-greater-than-zero refusal and becomes `DynamicValue::Literal`, a `Ref` becomes
  `DynamicValue::Ref` unchecked, `Some(None)` keeps the null refusal, and `Invalid` keeps its message.
- [x] 1.4 Fix the mechanical fallout of the type change at every `LayoutItem::Text` construction site
  the compiler flags (`src/batch.rs:237`, the fixtures from `src/templates.rs:3827` on, and
  `src/render/helpers.rs:1435`). All are `line_spacing: None` and none changes meaning.

## 2. Load-time validation, reference checking and input discovery

- [x] 2.1 Narrow `validate_line_spacing` (`src/templates.rs:2322`) to bounds-check literals only, so a
  `DynamicValue::Ref` passes it exactly as `validate_font_weight` passes a non-literal.
- [x] 2.2 Leave the reference unsubstituted in `instantiate_item_defaults`
  (`src/templates.rs:1822-1826`): clone the `DynamicValue` through rather than resolving it to the
  parameter's declared default the way `font_weight` is. A template whose pitch parameter declares
  `default: 0` must load (design, "A reference is left unsubstituted by `instantiate_with_defaults`").
- [x] 2.3 Add a `check_param_ref` call for `line_spacing` to the `Text` arm of
  `validate_item_references` (`src/templates.rs:1540-1560`) with allowed types `["number", "integer"]`
  — narrower than the sibling numeric sites on purpose — and a context string that puts
  `line_spacing` in the message alongside the layout path.
- [x] 2.4 Collect the reference in the input walker's `Text` arm (`src/templates.rs:272-290`) with
  `record_ref(r, false, false)`, the same call `font_weight` and `color` make. `interpolated` must be
  false: pitch is a layout attribute and nothing prints it. Add nothing to the `when:` gate, which
  already skips inactive items before the arm runs.

## 3. Error surface

- [x] 3.1 Add the `LineSpacingParamInvalid => "line_spacing_param_invalid"` variant to
  `src/reason.rs` alongside `ColorParamInvalid`.
- [x] 3.2 Add an `AppError` constructor for it in `src/errors.rs`, built on `invalid_request` like
  `color_param_invalid` (`src/errors.rs:278`), so the refusal is `400 InvalidRequest` with the slug in
  `details.reason` and a message naming the item's layout path and the parameter.
- [x] 3.3 Confirm `spec_documents_every_reason_and_invents_none` (`src/errors.rs:664`) passes on the
  new slug, which the active delta documents in backticks; no `docs/SPEC.md` edit (it is frozen).

## 4. Render resolution and emission

- [x] 4.1 Resolve the reference in `Renderer::intrinsic`'s `Text` arm (`src/render/mod.rs:1663-1704`),
  before `layout_text` is called, using `resolve_dynamic_value_f32` (`src/render/helpers.rs:187-225`).
  `TextLayoutItem.line_spacing` keeps its `Option<f32>` type and receives the already-resolved value.
- [x] 4.2 Bound the resolved value at that same point: finite and greater than zero, else return the
  new error naming the layout path and the parameter. Write the check against the resolved value, not
  against the reference. Do not clamp, do not fall back to 1.2, do not panic.
- [x] 4.3 Record the resolved value on `TextFit` (`src/render/helpers.rs:718-723`) from inside
  `layout_text`, so the fitter's own output is the emitter's only access path.
- [x] 4.4 Read the pitch from `args.text_fit` in `render_text_item` (`src/render/mod.rs:2203-2204`),
  drop `TextRenderArgs.line_spacing` (`src/render/mod.rs:1312`), and stop reading the item's
  `line_spacing` in `render_single_item` (`src/render/mod.rs:2072-2101`). The fitter and the emitter
  must be unable to see different pitches.
- [x] 4.5 Change nothing shared by every numeric parameter: leave `coerce_param_value`
  (`src/render/mod.rs:108-138`) and `resolve_dynamic_value_f32`'s string handling untouched, so a
  non-finite `number`, a saturating `integer` and the `mm`/`in` suffix keep behaving as they do for
  every other reference.

## 5. Load, read-back and input-list tests

- [x] 5.1 Extend the quarantine test (`src/templates.rs:9166`) so `"1.2"` and `"1.2mm"` are refused
  alongside `"1.2em"`, `"{{ pitch }}"`, `true`, `[1.2]` and an explicit null, each naming the file and
  the `line_spacing` key, with a valid template still served and startup intact.
- [x] 5.2 Test that a reference to an undeclared parameter quarantines the template with an error
  naming the layout path and the `line_spacing` key, the service still starting and serving a valid
  template.
- [x] 5.3 Test that a reference to a `length`, `string`, `boolean`, `enum`, `datetime` and `list`
  parameter each quarantines with the same message shape, and that a `number` and an `integer`
  parameter each load and are served.
- [x] 5.4 Test that a reference whose parameter declares `default: 0` loads and is served, its value
  checked at neither bound at load.
- [x] 5.5 Extend the HTTP read-back test (`src/lib.rs:3382`) so `GET /templates/{id}` reports
  `"{pitch}"` for a declared reference, `0.99` for a declared literal, and no key at all for an item
  declaring nothing.
- [x] 5.6 Test that a `number` parameter whose only use is `line_spacing: "{pitch}"` on an active item
  appears in the template's input list with its numeric control and `interpolated` false.
- [x] 5.7 Test that the same reference inside an item whose `when:` gate is false contributes no input
  entry, and that the same template with the gate true contributes one.
- [x] 5.8 Verify the published OpenAPI document still builds and reports the field as the
  literal-or-reference shape `DynamicValue<f32>` already publishes, the way `font_weight` does.

## 6. Single-label render tests

Every refusal below is exercised through `POST /api/render/label`, never through the numeric helper: a
helper test would hand the bound an intact `f32::NAN` and prove a behaviour no request can produce
(design, "This is why verification runs through the endpoints").

- [x] 6.1 Render one template declaring `line_spacing: "{pitch}"` twice, supplying 0.99 and then 1.5,
  and measure the distance between the corresponding ink bands of `"Hxy\nHxy"` on each render: 0.99
  and 1.5 font sizes. Measured on the render, not asserted from the fitter's arithmetic.
- [x] 6.2 Extend the fitter/Typst agreement test (`block_height_matches_typst_layout`,
  `src/render/mod.rs:6223`) to cover a *supplied* pitch at 0.5, 0.99, 1.2 and 1.5 over one to three
  lines, within the existing 1% tolerance.
- [x] 6.3 Render one height-bound two-line item with a `{ min, max }` `font_size` and
  `line_spacing: "{pitch}"` twice, supplying 0.99 and then 1.5, and assert the first settles at a
  larger font size than the second — proof the fitter saw the resolved value.
- [x] 6.4 Test that a supplied `0` and a supplied `-0.5` are each refused with `400 InvalidRequest`,
  `details.reason` of `line_spacing_param_invalid`, and a message naming the layout path and `pitch`,
  producing no image.
- [x] 6.5 Test that a supplied value numeric parameter resolution cannot read as a number is refused
  with `400 InvalidRequest` and `request_body_invalid` naming `pitch`, and not with
  `line_spacing_param_invalid`.
- [x] 6.6 Test that a supplied NaN, and a supplied magnitude beyond the range a `number` carries, are
  each refused with `400 InvalidRequest` and `request_body_invalid` naming `pitch`, producing no image
  and never reaching the fitter.
- [x] 6.7 Test that an `integer`-typed `pitch` supplied as `2` renders with the ink bands of two
  identical lines 2 font sizes apart, measured on the render.
- [x] 6.8 Test that an `integer`-typed `pitch` supplied as the JSON number `1e300` saturates to a
  legal enormous pitch, so a two-line height-bound item declaring `overflow: fail` is refused with
  `422` and `details.reason` of `text_does_not_fit`, not with `line_spacing_param_invalid` or
  `request_body_invalid`.
- [x] 6.9 Test that an `integer`-typed `pitch` supplied as the JSON number `-1e300` saturates negative
  and is refused with `400 InvalidRequest` and `line_spacing_param_invalid`, naming the layout path
  and `pitch`.
- [x] 6.10 Test that an `integer`-typed `pitch` supplied as the string `"9223372036854775808"` fails
  integer parsing and is refused with `400 InvalidRequest` and `request_body_invalid` naming `pitch`,
  not with `line_spacing_param_invalid`.
- [x] 6.11 Test that omitting a `pitch` parameter that declares no `default` is refused with
  `422 MissingField` naming the parameter, and that no label is produced at 1.2 or any other pitch.
- [x] 6.12 Test that a `pitch` parameter declaring `default: 0.99`, with `pitch` omitted, renders with
  the ink bands 0.99 font sizes apart, measured on the render.
- [x] 6.13 Test that a `pitch` parameter declaring `default: 0` loads and is served, and that omitting
  `pitch` is refused with `400 InvalidRequest` and `line_spacing_param_invalid` naming the layout path
  and `pitch`.
- [x] 6.14 Test that a `pitch` parameter declaring `default: .nan`, with `pitch` omitted, is refused
  with `422 TemplateInvalid` and `details.reason` of `param_default_unresolvable` naming `pitch`, and
  not with `line_spacing_param_invalid` — the existing `param-resolution` contract, unchanged.

## 7. Batch tests

- [x] 7.1 Test through `POST /api/batch` that three labels from a template declaring
  `line_spacing: "{pitch}"`, with the first and third supplying a usable pitch and the second
  supplying `0`, refuse the whole request with `422` and `error.code` `BatchInvalid`, carry exactly
  one `error.details.failures` entry at `index` 1 with `code` `InvalidRequest` and `reason`
  `line_spacing_param_invalid` naming the layout path and `pitch`, and return no ZIP and no PDF.
- [x] 7.2 Test that a two-label batch whose second label omits a `pitch` parameter with no `default`
  refuses with `422 BatchInvalid`, produces nothing, and gives that entry `code` `MissingField` with
  no `reason` key at all.
- [x] 7.3 Test that the same batch against a template whose `pitch` parameter declares a `.nan`
  default gives the entry `code` `TemplateInvalid` and `reason` `param_default_unresolvable`.

## 8. Documentation

- [x] 8.1 Extend the `line_spacing` section of `docs/AUTHORING.md:399-414` so its worked `text`
  example shows the `"{param}"` reference spelling alongside the bare number, naming the parameter
  types it accepts (`number` and `integer`). Do not touch `docs/SPEC.md` or `docs/adr/`, both frozen.

## 9. Gates

- [x] 9.1 Run `cargo fmt --check`. The gate is read-only, so repair any mis-formatting as an authored
  edit and re-run it.
- [x] 9.2 Run `cargo clippy --all-targets --all-features` and fix the root cause of anything it flags;
  never silence a lint with `#[allow(clippy::...)]`.
- [x] 9.3 Run `cargo test` and confirm the whole suite passes, including the #363 tests this change
  must not disturb: `line_spacing_load_refusals_quarantine_files`,
  `render_measured_line_pitch_band_distances_and_default_equivalence`,
  `tighter_pitch_allows_larger_font_size_and_single_line_is_invariant`,
  `block_height_matches_typst_layout` and `template_line_spacing_readback_and_refusals`.
- [x] 9.4 Skip the `ui/` gates: `ui/` does not reference `line_spacing` and this diff does not touch
  it.
