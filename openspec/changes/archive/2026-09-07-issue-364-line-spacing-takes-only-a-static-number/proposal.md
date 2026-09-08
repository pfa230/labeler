## Why

Implements GitHub issue **#364**: `line_spacing` takes only a static number, so pitch cannot vary per
render.

#363 added `line_spacing` to `text` items as a bare number, deliberately holding the change to one
new field with one value shape. Every neighbouring numeric field on a `text` item already varies per
render: `font_weight` is a `DynamicValue<u16>`, `color` is a `DynamicValue<Color>`, and a dynamic
width or height is a `DynamicValue<f32>`. Pitch is the odd one out, so a template that wants a tighter
pitch for a long name and a looser one for a short one cannot express it, and neither can one driven by
a caller-supplied style parameter.

## What Changes

- `line_spacing` accepts a `"{param}"` reference in addition to a bare number, resolved per render
  from the request's parameter values. The domain field becomes `Option<DynamicValue<f32>>`.
- The reference spelling is the single-brace `"{pitch}"` form that `font_weight`, `color` and every
  dynamic dimension use. `"{{ pitch }}"` stays refused because it is outside that grammar: doubled
  braces are not a second reference spelling, and `interpolation-tokens` gives them as literal braces
  rather than as a token, whose own spelling is single-braced.
- A referenced parameter must be declared `number` or `integer`. This deliberately narrows the
  `["length", "number", "integer"]` set the sibling numeric ref sites accept: pitch is a unitless
  multiple of the font size, so a `length` parameter carrying mm or in has no meaning here and is
  refused at load.
- The reference is resolved **before** the measurement pass, and the resolved value is what both the
  fitter and the emitter use. This is the part that is not a copy of `font_weight`: pitch feeds the
  auto-shrink search and the block-height arithmetic directly, so measuring at 1.2 and emitting at the
  resolved value would break the published requirement that the fitter reserve and the emitter emit
  against the same pitch.
- A resolved value that is zero or negative refuses the render with a new `Reason`,
  `line_spacing_param_invalid`, naming the item's layout path and the parameter. It is distinct from
  the type failure numeric parameter resolution already reports as `request_body_invalid`, so a client
  can tell "you sent a pitch of 0" from "you sent a string". On the batch path a refused pitch is a
  failing label, so the request is `422 BatchInvalid` and produces nothing, per `batch-validation`,
  with each failure entry carrying that label's own code and a reason only when that code carries one.
- A parameter named only by a `line_spacing` reference gets an input control, with `interpolated`
  false, like every other parameter read by a layout attribute. This is a requirement `template-inputs`
  already publishes, and it needs the reference added to the input walker alongside `font_weight` and
  `color`; without that the form would offer no control for a value the render demands.
- **Departs from one bullet of #364's acceptance list:** the issue asks that a supplied NaN or
  infinity also carry the new slug. It cannot without changing how *every* numeric parameter is
  coerced, which is outside this issue. Parameter coercion converts a `number` to
  `serde_json::json!(f32)`, and serde_json maps a non-finite float to null, so a non-finite pitch
  arrives having already lost its value and is refused as a value that is not a number
  (`request_body_invalid`). Nothing the issue protects is lost — no non-finite value renders, reaches
  the fitter, clamps or panics — only which of two 400s the caller reads. `design.md` records the two
  alternatives and why each was rejected.
- The two accepted parameter types differ on an oversized value, and the contract states each outcome
  rather than promising a uniformity that does not exist. A `number` loses it to coercion and is
  refused as not-a-number. An `integer` saturates, so a large positive JSON number becomes the largest
  integer it holds — a legal, enormous pitch whose block does not fit, answered by the item's existing
  `overflow` rule — while a large negative one becomes the smallest and is refused by the pitch bound
  as negative. An oversized value written as a *string* fails integer parsing outright and is refused
  as not a value of its type. No magnitude ceiling is added; the only bounds on a pitch are finiteness
  and positivity.
- An absent parameter refuses the render with `422 MissingField`. Resolution never falls back to 1.2:
  `params[].default` is the mechanism for a value a caller omits (#145), and 1.2 is the default for an
  item that declared no `line_spacing` at all, not for one whose reference did not resolve.
- A declared `default` is answered where it fails, and `param-resolution`'s existing rule is untouched:
  a `.nan` default fails resolution as the template's fault (`422 TemplateInvalid`,
  `param_default_unresolvable`), while a `0` default coerces, resolves, and meets the pitch bound as a
  `400`. The bound cannot see provenance; `design.md` records why neither way of hiding that is worth
  its cost.
- Not breaking. A bare number keeps every behavior #363 published, load refusals included, and no
  existing template alters appearance.

Out of scope, named here so the boundary is explicit: `font_weight`'s own measure/emit divergence
(`src/render/mod.rs:1672-1683` resolves a ref with a silent `.unwrap_or(400)` for measuring) is a
separate defect tracked as **#382** and is not touched.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `text-line-spacing`: both published requirements change. The first, "A text item's line pitch is
  authored as `line_spacing`", currently states that the value is a bare number and that this "makes
  the field static-only, since a `{{ param }}` interpolation is a string and is refused as one"; that
  sentence is what this change overturns, and the requirement gains the reference spelling, its
  load-time declaration and type checks, its render-time bound and reason, and its read-back shape.
  The second, "Pitch is the authored multiple, or 1.2, everywhere", gains the resolved value as a
  third source of the multiple alongside the authored literal and the 1.2 default, and states that the
  single resolution feeds fitter and emitter alike.

`MODIFIED` is valid for both: each requirement already lives in `openspec/specs/text-line-spacing/spec.md`,
so the tooling has something to resolve against. Each delta carries the complete post-change
requirement rather than the difference.

## Impact

- `src/raw.rs`: `RawLineSpacing` gains a reference variant. The strict discrimination stays: a YAML
  number is a literal, a single-brace string is a reference, and every other scalar (including
  `"1.2"`, `"1.2em"` and `"{{ pitch }}"`) keeps its existing refusal.
- `src/models.rs`: `LayoutItem::Text.line_spacing` becomes `Option<DynamicValue<f32>>`, which changes
  the published OpenAPI schema for the field from a number to the literal-or-reference shape
  `DynamicValue<f32>` already publishes elsewhere.
- `src/convert.rs`: the `TryFrom` carries the reference through and keeps the literal bound.
- `src/templates.rs`: `validate_line_spacing` bounds-checks literals only; `validate_item_references`
  gains a `check_param_ref` call for the field; and the input walker's `Text` arm gains a `record_ref`
  call so a pitch-only parameter reaches the input list.
- `src/render/mod.rs`, `src/render/helpers.rs`: resolution before the layout pass, the resolved value
  carried on `TextFit` so the emitter cannot read a different one, and the bounds refusal.
- `src/errors.rs`, `src/reason.rs`: one new `Reason` and its `AppError` constructor.
- `docs/AUTHORING.md`: the existing worked example gains the reference spelling.
- No UI surface: `ui/` does not reference `line_spacing`.
