## 1. One colour type, one parser, one table

- [x] 1.1 In `src/models.rs`, make `Color` the merged type `{ spelling, rgba }`: `FromStr` over the
  sixteen CSS Level 1 names at the values in `colour-vocabulary` matched case-insensitively, and hex
  in 3, 4, 6 or 8 digits with digit doubling; refusing an unknown name (including `eastern` and
  `orange`), a hex string without `#`, any other digit count, a non-hex character and the empty
  string. Keep `hex()` as the canonical formatter the Typst emitter needs.
- [x] 1.2 Give `Color` `Serialize` writing the authored spelling, `Deserialize` through `FromStr`, and
  a utoipa string schema describing the accepted literal forms (name, `#`-hex).
- [x] 1.3 Delete `Ink` from `src/models.rs` and everything that referenced it, including its tests.
- [x] 1.4 Replace the `Color::BLACK` const with a constructor and fix every site the loss of `Copy`
  breaks (`src/convert.rs` stroke default, render and test call sites).
- [x] 1.5 In `src/raw.rs`, delete `parse_color` and `deserialize_dynamic_ink` in favour of one
  `DynamicValue<Color>` deserializer shared by `text.color`, `stroke.color` and `background`, keeping
  the paint keys' `Option<Option<_>>` presence encoding and reading `text.color: null` as absence.
- [x] 1.6 Unit-test the parser: each of the sixteen names denotes its stated value; `red`, `Red` and
  `RED` are one name; `"#f0f"`, `"#F0F8"`, `"#ff00ff"`, `"#FF00FF80"` parse with digit doubling and
  alpha; `chartreuse`, `eastern`, `orange`, `"ff00ff"`, `"#ff00f"`, `"#gg0000"`, `""`, `16711680` and
  `true` are refused.

## 2. The text field is `color`

- [x] 2.1 Rename the field to `color` through `src/raw.rs` (`TextRaw`), `src/models.rs`
  (`LayoutItem::Text`, `Option<DynamicValue<Color>>`) and `src/convert.rs`.
- [x] 2.2 Test that `ink:` on a `text` item is an unknown field: the template is quarantined at load
  with an error naming `ink` and the item's layout path, and the server still starts and serves every
  other template.
- [x] 2.3 Test that `color` on a `qr`, `image`, `line` or `container` item is refused as an unknown
  field naming that item's layout path.
- [x] 2.4 Test that an absent `color` and an explicit `color: null` both render black, while
  `background: null` and `stroke: { thickness: 0.2, color: null }` are still refused naming the field.
- [x] 2.5 Test that two otherwise identical `text` items differing only in `color` resolve to the same
  box, the same fitted font size and the same line breaks.

## 3. `{param}` on `stroke.color` and `background`

- [x] 3.1 Make `Stroke.color` a `DynamicValue<Color>` and the container's `background` an
  `Option<DynamicValue<Color>>` through `src/raw.rs`, `src/models.rs` and `src/convert.rs`, keeping
  the `black` default for an omitted `stroke.color`.
- [x] 3.2 In `src/templates.rs`, extend `validate_item_references` to check a reference on
  `background` and `stroke.color` with `check_param_ref` and the `["string", "enum"]` allowlist, on
  `container` and `line` alike, naming the item's layout path, the field and the parameter.
- [x] 3.3 In `src/templates.rs`, extend the input walk so an active shape's colour reference is
  recorded as not interpolated, inheriting the existing `when` gating.
- [x] 3.4 Rename `resolve_dynamic_value_ink` to `resolve_dynamic_value_color` in
  `src/render/helpers.rs` and call it at all three emission sites in `src/render/mod.rs` (text fill,
  line stroke, container fill and stroke) before formatting.
- [x] 3.5 Test the load-time refusals per field: `background: "{missing}"` and
  `stroke: { thickness: 0.2, color: "{missing}" }` against an undeclared parameter, and
  `background: "{width}"` against a `length`, `number`, `integer`, `boolean` or `datetime` parameter.
- [x] 3.6 Test that a supplied parameter renders on each field: `background: "{brand}"` fills,
  `stroke.color: "{brand}"` outlines `navy`, `color: "{brand}"` paints glyphs, and an `enum`
  parameter of colour names drives each of its values.
- [x] 3.7 Test the input list: an ungated container's `background` reference and an ungated text
  item's `color` reference are asked for as not interpolated; a `when`-gated item contributes none
  while its `when` parameters still appear; a parameter both referenced and interpolated is reported
  once, as interpolated.

## 4. The failure reason is `color_param_invalid`

- [x] 4.1 Rename `Reason::InkParamInvalid` to `ColorParamInvalid` with the wire string
  `color_param_invalid` in `src/reason.rs`, and rename its `AppError` constructor and message in
  `src/errors.rs` so the message names the parameter rather than an ink.
- [x] 4.2 Test that a resolved value that is not a colour, and a resolved value that is itself a
  `"{other}"` reference, fail `POST /render/label` with `400`, code `InvalidRequest`,
  `details.reason` `color_param_invalid` and a message naming the parameter, producing no label.
- [x] 4.3 Test that one bad colour in a two-label batch fails the whole batch: `422`, code
  `BatchInvalid`, one `details.failures` entry with `index` 1, `code` `InvalidRequest`, `reason`
  `color_param_invalid`, and no PDF or ZIP for either label.

## 5. Read-back and OpenAPI

- [x] 5.1 Make `GET /templates/{id}` report every colour as the author wrote it, a reference as
  `"{name}"`, and `stroke.color` even when omitted (as `"black"`), while an omitted `text.color` or
  `background` stays absent from the response.
- [x] 5.2 Update the read-back assertions at `src/lib.rs:9876` to the authored spellings and add
  coverage for `stroke: { thickness: 0.2, color: "#F0F" }` reporting `"#F0F"`, a defaulted stroke
  colour reporting `"black"`, `background: "{brand}"` reporting `"{brand}"`, and an uncoloured `text`
  item carrying no `color` key.
- [x] 5.3 Deregister `Ink` from `src/openapi.rs` and update `Color`'s registered schema description to
  the accepted literal forms.

## 6. The cross-field invariant and the output paths

- [x] 6.1 Test that a `text` item with `color: red` inside a container with `background: red` emits
  the same paint value for both, and that `red`, `green`, `gray` and `yellow` on a text item denote
  the CSS values (`#ff0000`, `#008000`, `#808080`, `#ffff00`) rather than the engine's constants.
- [x] 6.2 Test that a template carrying an unreadable colour on a shape is quarantined with an error
  naming the file, the layout path and the field while a valid template is still served, and that the
  template write endpoint refuses the same template without writing a file.
- [x] 6.3 Test that a coloured `text` item inside a painted container carries both colours into PNG
  and PDF and into every slot of a multi-slot sheet, and that `color_mode=bilevel` thresholds both by
  the same global luminance threshold.

## 7. Records

- [x] 7.1 Write `docs/adr/0093-*.md`: the type is `Color`, why "ink" keeps its typographic meaning
  here (ADR-0043, ADR-0050, ADR-0084), the one sixteen-name table, the reference form on every colour
  field, the authored read-back, and the silent value shift for text colours. Supersede ADR-0091's
  vocabulary and naming clauses and ADR-0092's clauses 5 and 6.
- [x] 7.2 Add the ADR-0093 row to `docs/adr/README.md`.
- [x] 7.3 Rewrite the colour paragraph of `docs/AUTHORING.md` (`docs/AUTHORING.md:500-504`) to the one
  vocabulary accepted by `stroke.color`, `background` and `text.color` alike, document `color` on a
  text item, record that `ink` is gone and a template using it is quarantined, and spell the stroke
  default `black`.
- [x] 7.4 Rewrite the `## Purpose` paragraph of `openspec/specs/text-ink/spec.md` so it describes the
  `color` field on a text item rather than `ink`, leaving its requirements to the archive sync.

## 8. Gates

- [x] 8.1 Confirm `Ink` appears nowhere under `src/`.
- [x] 8.2 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test`, and fix what
  they report.
