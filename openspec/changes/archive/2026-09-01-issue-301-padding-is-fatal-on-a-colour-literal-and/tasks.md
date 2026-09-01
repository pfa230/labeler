## 1. The parser strips, and the declared string survives it

- [x] 1.1 In `Color::from_str` (`src/models.rs:893`), capture the argument before trimming:
  `let spelling = s.to_owned();` then a separate `let value = s.trim();`. Do not shadow `s` with the
  trimmed value — after that the declared string is unreachable and the read-back loses its padding.
- [x] 1.2 Point every step that judges the value at the trimmed binding: the empty check, the `#`
  test and hex-digit walk, the name table, and every error message the function formats. `" octarine "`
  is then reported as `unknown colour 'octarine'`.
- [x] 1.3 Order the existing `is_empty()` guard after the trim, so `"   "` is refused with the
  existing empty-colour message. No new error variant, and no other refusal changes.
- [x] 1.4 Construct both success paths from the captured `spelling` rather than from the argument
  (`src/models.rs:951` in the hex branch, `:977` in the name branch), so the padding reaches the
  serializer at `src/models.rs:992`.

## 2. Render-time resolution

- [x] 2.1 In `resolve_dynamic_value_color` (`src/render/helpers.rs:223-251`), bind the trimmed
  resolved string once, immediately after the value is known to be a string, and read that binding in
  the `{...}` chained-reference test, in the `parse::<Color>()` call and in the
  `unrecognised colour '{s}'` message. `" {other} "` must reach the chained-reference refusal, not
  the unrecognised-colour one.

## 3. The comment that argues the opposite

- [x] 3.1 Rewrite the doc comment on `RawColor`'s deliberately-failing `FromStr`
  (`src/raw.rs:34-44`). The code stays. Its stated reason — letting `convert.rs` and `Color::from_str`
  "strictly reject whitespace at load time" — is gone, and the surviving one takes its place: the
  failing `FromStr` keeps `DynamicValueVisitor`'s `trimmed.parse::<T>()` fast path from firing, so the
  untrimmed declared string reaches the model and the read-back keeps its padding.

## 4. Tests

- [x] 4.1 Conversion layer: a template writing `color: " red "`, `background: " #F0F "` and
  `stroke.color: " navy "` loads, and the converted model carries `#ff0000`, `#ff00ff` and `#000080`
  respectively.
- [x] 4.2 Render layer: drive that same template's layout items through the source-emitting path the
  colour tests already use (`render_test_items`, `src/render/mod.rs:2349`, as
  `emitted_typst_source_color_fill_and_omission` at `:8797` does) and assert the emitted markup
  carries `fill: rgb("#ff0000ff")` for the padded `text.color`, `fill: rgb("#ff00ffff")` for the
  padded `background` and `rgb("#000080ff")` for the padded `stroke.color`.
- [x] 4.3 Reference at load: `color: " {brand} "` still loads as a reference to `brand` and still
  renders.
- [x] 4.4 Resolved parameter at render: a render supplying `brand: " navy "` for a referenced colour
  succeeds and paints `#000080`. This replaces
  `color_param_with_whitespace_is_rejected_at_render_time` (`src/render/mod.rs:8951`), which asserts
  the refusal this change removes.
- [x] 4.5 Chained reference at render: a render supplying `brand: " {other} "` fails with
  `color_param_invalid` carrying the chained-reference message, asserted on the message and not only
  on the reason.
- [x] 4.6 HTTP read-back, one test over a template carrying both forms: a `text` item written
  `color: " red "` reports `" red "` with its padding, and a container written
  `background: " {brand} "` reports `"{brand}"`. Include a colour whose YAML scalar reaches the same
  characters by another spelling (an escape, or another quoting or scalar style) and assert the
  response reports the decoded content, since that is the boundary of what the read-back preserves.
- [x] 4.7 Refusals: `" red "` and `" #ff0000 "` leave `invalid_colour_strings_are_rejected`
  (`src/models.rs:1430`); `"   "`, `"re d"` and `"# f0f"` join it. Add a load-level test that a
  template writing `color: "   "` is quarantined, with the error naming the file, the item's layout
  path and the field.

## 5. Documentation

- [x] 5.1 Add one clause to the colour paragraph of `docs/AUTHORING.md` (`docs/AUTHORING.md:500-506`)
  saying surrounding whitespace is ignored in a colour.

## 6. Gates

- [x] 6.1 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test`, and fix what
  they report.
