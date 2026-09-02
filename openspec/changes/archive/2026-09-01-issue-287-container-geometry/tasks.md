## 1. The resolver classifies what is fixed by the template

- [x] 1.1 Add `fixed_by_template: bool` to `AxisSpec` in `src/resolver.rs`, documented in the style of
      the neighbouring `written_as_to`: recorded by the classifier so no later rule has to read
      `Extent` again, and named for the property rather than for a stage.
- [x] 1.2 Set it in `source_of` and nowhere else: true for an `Author` extent from a number or from a
      `to` whose corners are both non-negative or both sign-negative; false for a `"{param}"`
      reference, a `ShrinkingTo`, `Content` and `Frame`.
- [x] 1.3 Unit-test one case per row of the delta's spelling table, asserting the flag for a number, a
      constant `to` of each sign pairing, a parameter reference, a shrinking `to`, `content` and
      `fill`.

## 2. The geometry field, through the three layers

- [x] 2.1 Add the `shape` key to the container in `src/raw.rs` as an optional value, leaving
      `deny_unknown_fields` to reject it on every other item type.
- [x] 2.2 Add the geometry enum to `src/models.rs` with variants for `rect`, `ellipse` and `circle`,
      defaulting to `rect` when the key is absent.
- [x] 2.3 Classify the value in the container's `TryFrom` in `src/convert.rs`, refusing an unknown one
      with a `serde_path_to_error` path that names the value and the accepted set.
- [x] 2.4 Refuse `rounded` alongside `shape: ellipse` or `shape: circle` in `src/convert.rs`, naming
      `rounded` on that container, as the radius requirement now scopes that key to a geometry with
      corners.
- [x] 2.5 Register the new model in `src/openapi.rs`.
- [x] 2.6 Unit-test in `src/convert.rs`: the default is `rect`; each accepted value parses; `polygon`
      and `Rect` are refused naming the value and the set; `shape` on a `text`, `qr`, `image` or
      `line` is refused naming the field and the item; `rounded` on each round geometry is refused.

## 3. The load-time squareness check

- [x] 3.1 In `src/templates.rs`, check a `circle` container's resolved box for squareness only when
      `source_of` reports `fixed_by_template` on both axes, branching on that flag and never on the
      spelling.
- [x] 3.2 Compare with `resolver::BOUNDS_EPSILON` rather than exact equality or a new constant:
      square means `(w - h).abs() <= BOUNDS_EPSILON`.
- [x] 3.3 Report a non-square box as ordinary structural validation, so the template is quarantined
      and reported as `TemplateInvalid` with the reason template validation already carries, never as
      `circle_box_not_square`, and the server still starts and still serves every other template.
- [x] 3.4 Unit-test that `size: [14, 12]` quarantines naming the container's `size`; that
      `size: [content, content]`, `size: ["{w}", 12]` and a shrinking `to` all load without
      quarantine; that a `circle` fixed by the template is refused whatever `when:` it carries; and
      that `at: [0.2, 0.0]` with `to: [0.3, 0.1]` loads while a 0.001 difference does not.

## 4. The render-time refusal

- [x] 4.1 Add one variant to `src/reason.rs` for the slug `circle_box_not_square`.
- [x] 4.2 Check every active `circle`'s resolved box at render, on the emission walk where the
      final `place` frame is known (so a gated-off item never reaches the check via
      `is_item_active` filtering in `render_items` before dispatch); use the same
      `BOUNDS_EPSILON` comparison as the load check.
- [x] 4.3 Raise it as `422 UnsupportedLayoutItem` with `details.reason` `circle_box_not_square` and a
      message naming the JSON path of the container, adding no new envelope: the single-label path
      carries it at the top level and the batch path carries it as an existing
      `details.failures` entry.

## 5. Geometry-aware emission

- [x] 5.1 For `shape: rect`, collapse the placed `#rect` overlay in `src/render/mod.rs` into the
      `#box` that already holds the children and already carries `clip: true`, moving `fill`,
      `stroke` and `radius` onto it, so one element carries the container.
- [x] 5.2 For `shape: ellipse` and `shape: circle`, place an `#ellipse(width, height, fill, stroke)`
      first and the child `#box` second, keeping the background-then-stroke-then-children order and
      leaving that child box unstroked and unrounded so its clip stays the whole rectangle.
- [x] 5.3 Emit `#ellipse` for `circle` too, never Typst's `#circle`, on a box the service has already
      proven square.
- [x] 5.4 Keep the existing radius clamp to half the shorter side, and keep placement, padding and
      rotation unchanged at every geometry, so a child's coordinates and extents do not depend on
      `shape`.

## 6. HTTP-level tests

- [x] 6.1 Add fixture templates under `tests/fixtures/templates/` covering the geometries the
      scenarios exercise: a default-`rect` container, an ellipse with a text child and padding, a
      square-box ellipse, a `content`-sized circle, a parameter-sized circle with a declared default,
      a gated-off parameter-sized circle, a rounded rectangle with a child in the corner, a stroked
      rectangle with a child reaching the edge, and a stroked ellipse with a child crossing the curve.
- [x] 6.2 Assert through the render endpoint that a `content`-sized circle resolving non-square
      returns `422` with `error.code` `UnsupportedLayoutItem` and `error.details.reason`
      `circle_box_not_square`, and that the same template resolving square renders.
- [x] 6.3 Assert through the batch endpoint that the same failure returns `422 BatchInvalid` with a
      `details.failures` entry carrying that label's `index`, code `UnsupportedLayoutItem` and that
      reason.
- [x] 6.4 Assert through the render endpoint that `size: ["{w}", 12]` against `default: w = 12`
      renders with no `w` supplied and is refused with that reason when the request supplies `w: 14`.
- [x] 6.5 Assert through the render endpoint that the same parameter-sized circle under a false
      `when:` succeeds with `w: 14` supplied and raises no failure, and is refused when the predicate
      is true.
- [x] 6.6 Assert that a default-`rect` container defaults to `rect` (the explicit `shape: rect`
      renders identically to the omitted default) and that an unknown `shape` value leaves the
      template quarantined while the server still serves the others.

## 7. Gates

- [x] 7.1 Run `cargo fmt`.
- [x] 7.2 Run `cargo clippy --all-targets --all-features` and fix the root cause of anything it
      flags, never silencing it with an `allow`.
- [x] 7.3 Run `cargo test`.

The clipping changes are visual and their evidence is an image no later reader can retrieve, so no
task above claims them. Before calling the change done, run the render-inspect loop by hand on a
stroked container with a child reaching the edge and on a rounded one with a child in the corner.
