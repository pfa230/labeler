## 1. The colour type

- [x] 1.1 Add `Color` to `src/models.rs`, storing normalized RGBA. Its `Serialize` emits the canonical
      lower-case `#rrggbbaa` string, because that form is the API contract, not an implementation
      detail.
- [x] 1.2 Add the colour parser in `src/raw.rs`: a leading `#` with 3, 4, 6 or 8 case-insensitive hex
      digits (3 and 4 expanding by digit doubling), or one of the sixteen CSS Level 1 names matched
      case-insensitively, with the values in the spec's table. Reject everything else.
- [x] 1.3 Unit-test the parser against every accepted form and every refusal named in the spec:
      five digits, a missing `#`, a non-hex character, and an unknown name. Assert `red` is
      `#ff0000ff` and not Typst's `#ff4136`, so a future edit toward the renderer's table fails here.
- [x] 1.4 Register `Color` in `src/openapi.rs` as a **string** schema carrying the `#rrggbbaa` form,
      not its internal RGBA storage.

## 2. The stroke type and the paint fields

- [x] 2.1 Add `Stroke { thickness: f32, color: Color }` to `src/models.rs`, and `StrokeRaw` to
      `src/raw.rs` with `deny_unknown_fields` so `stroke: { thickness: 0.2, dash: dotted }` is refused.
- [x] 2.2 Make every optional paint key presence-preserving (`Option<Option<T>>` via
      `deserialize_present_typed`, `src/raw.rs:65`) so an explicit `null` is `Some(None)` rather than
      absence. Cover `stroke`, `background`, `rounded` and `stroke.color`, and handle a null
      `thickness` so it is reported as a null rather than as a missing field.
- [x] 2.3 Replace `frame: Option<Frame>` on `ContainerRaw` with `stroke`, `background` and `rounded`;
      replace `LineRaw`'s bare `thickness` with `stroke`. Delete `Frame` from `src/models.rs`.
- [x] 2.4 Convert both in `src/convert.rs` via `TryFrom`, with `serde_path_to_error` paths attached,
      refusing a missing `thickness` and an explicit null on any paint key (`stroke`,
      `background`, `rounded`, `stroke.color`, `stroke.thickness`), and via `deny_unknown_fields`
      on `LineRaw` (`src/raw.rs:393`) refusing `background` or `rounded` on a `line`; the
      finite and `>= 0.0001` bounds on `thickness` and `rounded` are enforced in
      `src/templates.rs:1882,1985,1990` (`validate()`), which is the path that yields
      `template_validation_failed` for those values.
- [x] 2.5 Register `Stroke` in `src/openapi.rs`.

## 3. Validation

- [x] 3.1 Replace the container frame check at `src/templates.rs:1967` and the line thickness check at
      `:1867` with the new rules. Add no new error code and do not remap existing ones: which
      `details.reason` a refusal carries is #289's question, not this change's.
- [x] 3.2 Refuse `stroke`, `background` and `rounded` on `text`, `qr` and `image`.
- [x] 3.3 Test that a template using `frame:`, a bare `line.thickness`, or `rounded: true`/`false` is
      quarantined naming the field, **and** that the registry still serves every other template and
      the server still starts.
- [x] 3.4 Test the numeric boundary both ways: `0.0001` accepted, `0.00001` refused, on both
      `thickness` and `rounded`; plus `.nan`, `.inf`, `0` and a negative.
- [x] 3.5 Run a line's endpoint resolution and bounds check **unconditionally**, gating only the
      emitted `#line` on whether a stroke is present (`src/render/mod.rs:1762`). A strokeless line
      draws nothing and still fails the checks a stroked one fails. Test it against the template in
      `a_container_with_no_room_left_fails_cleanly_at_render` with its `stroke` removed: it must still
      fail, not render a page.

## 4. Rendering

- [x] 4.1 In `render_container_item` (`src/render/mod.rs:2032`), emit the rect when a `stroke` **or** a
      `background` is present, as `#rect(width, height, fill: …, stroke: … , radius: …)`, with
      `fill: none` and `stroke: none` for the absent one. Keep it emitted before the child box so the
      background stays behind the children.
- [x] 4.2 Clamp the radius to half the shorter resolved side before emitting, so Typst's own
      stroke-dependent clamp is never reached.
- [x] 4.3 Give `#line` a `{thickness} + {colour}` stroke in `render_line_item`
      (`src/render/mod.rs:2020`).
- [x] 4.4 Emit every colour as `rgb("#rrggbbaa")`, so the renderer never resolves a name.
- [x] 4.5 Assert the generated Typst source for: fill only, stroke only, both, neither; a rounded fill
      with no stroke; and a rotated container, whose painted rect must stay axis-aligned.
- [x] 4.6 Add an HTTP-level test that a filled, rounded container renders successfully to **PNG** and
      to **PDF**, at the status-code level rather than one layer below it.

## 5. Migration of existing callers

- [x] 5.1 `tests/fixtures/templates/avery5163_asset_tag.yaml:48`: `frame: { thickness: 0.02, rounded: false }`
      becomes `stroke: { thickness: 0.02 }`, with `rounded` dropped.
- [x] 5.2 `tests/acceptance_issue_263.rs:566`: the embedded template moves to
      `stroke: { thickness: 0.5 }`. It asserts the `flow-layout` guarantee this change respells, so it
      must keep asserting the same thing.
- [x] 5.3 The five `Frame { … }` constructions in `src/render/mod.rs` unit tests (`:3799`, `:3839`,
      `:4015`, `:4739`, `:7197`) become `Stroke { … }`. `:4739` passes `rounded: true` and is the only
      one that must choose an explicit radius.
- [x] 5.4 Move every remaining bare `line.thickness` in fixtures and tests to `stroke: { thickness: … }`.

## 6. Specs, decisions and docs

- [x] 6.1 Write `docs/adr/0092-a-shape-carries-a-stroke-and-a-background.md`: the paint vocabulary, the
      reversal of the monochrome constraint **for shape paint only** (leaving #282 free), the
      project-owned CSS name table and why it deliberately differs from the renderer's, and the
      authored radius.
- [x] 6.2 Add the ADR-0092 row to `docs/adr/README.md`.
- [x] 6.3 Rewrite `docs/AUTHORING.md` §9, which documents `frame` by worked example twice.
- [x] 6.4 Add the upgrade note to `docs/DEPLOY.md`: the three removed spellings, their replacements,
      and that an un-migrated template is quarantined rather than fatal.

## 7. Gates

- [x] 7.1 Run `cargo fmt`.
- [x] 7.2 Run `cargo clippy --all-targets --all-features` and fix the root cause of anything it flags,
      never with `#[allow]`.
- [x] 7.3 Run `cargo test`.
