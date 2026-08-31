## 1. The colour type

- [x] 1.1 Add `Ink` to `src/models.rs`: a struct holding the author's spelling and the `[u8; 4]` RGBA
      it resolved to, constructed only through `FromStr`. Parse the 18 names against the table pinned
      in `design.md`, and `#`-prefixed hex in 3, 4, 6 and 8 digit forms, case-insensitive. Reject
      everything else, including a hex string with no `#` and the empty string.
- [x] 1.2 Implement `Deserialize` for `Ink` through `FromStr` and `Serialize` as the stored spelling,
      so `GET /templates/{id}` reports `red` rather than rewriting it to `#ff4136`. Derive `ToSchema`
      as a string.
- [x] 1.3 Unit-test `Ink`: every one of the 18 names resolves to its pinned RGBA; `#f00` and
      `#ff0000` resolve equal, as do `#f008` and `#ff000088`; `chartreuse`, `"ff0000"`, `"#ff00"`,
      `""` and a non-string are each rejected; a parsed `Ink` serializes back to the exact string it
      was given.
- [x] 1.4 Register `Ink` in `src/openapi.rs`.

## 2. The field on a text item

- [x] 2.1 Add `Option<Dynamic<Ink>>` to `TextRaw` in `src/raw.rs` and `Option<DynamicValue<Ink>>` to
      `LayoutItem::Text` in `src/models.rs`, skipping serialization when absent.
- [x] 2.2 Carry the value across in `src/convert.rs`. It parses nothing: `Ink` is already validated by
      its own `Deserialize`, and `serde_path_to_error` supplies the layout path.
- [x] 2.3 Test that `ink: chartreuse` on a text item fails to load with an error naming the item's
      layout path and the `ink` field, and that the same YAML submitted to the template write endpoint
      is refused with no file written.
- [x] 2.4 Test that `ink` on a `qr`, `image`, `line` or `container` is refused as an unknown field.
- [x] 2.5 Test that one template with a bad ink is quarantined while a valid sibling is still served,
      and that startup is not aborted.

## 3. Parameter references

- [x] 3.1 In `src/templates.rs`, call
      `check_param_ref(params, name, "ink", &["string", "enum"])` for a `DynamicValue::Ref` ink,
      beside the `font_weight` call at `src/templates.rs:1457`.
- [x] 3.2 Test that `ink: "{missing}"` is refused at load naming the undeclared parameter, and that a
      reference to a `length`, `number`, `integer`, `boolean` or `datetime` parameter is refused
      naming the parameter and its type.
- [x] 3.3 In `derive_inputs_internal` (`src/templates.rs:191`), record an active text item's ink
      reference with `record_ref(r, false, false, false)`, beside the `font_weight` line at
      `src/templates.rs:295`.
- [x] 3.4 Test input derivation: an ungated `ink: "{brand}"` puts `brand` in the list marked not
      interpolated; a `when`-gated-off item contributes nothing while the `when`'s own parameters
      still appear; a parameter used as an ink and interpolated elsewhere appears once, interpolated.

## 4. Rendering

- [x] 4.1 Add `InkParamInvalid => "ink_param_invalid"` to `src/reason.rs` under the `InvalidRequest`
      group, beside `DatetimeParamInvalid`, and an `AppError` constructor for it that names the
      parameter.
- [x] 4.2 In the render pass of `src/render/mod.rs` (beside the `font_weight` resolution at
      `src/render/mod.rs:1757-1762`), resolve a `DynamicValue::Ref` ink. A value that is absent, not a
      string, not a colour, or itself a `{…}` reference returns the error from 4.1. Do not add a
      fallback, and do not touch the measure pass.
- [x] 4.3 Pass the resolved `Ink` into `render_text_item` and emit `fill: rgb(r, g, b, a)` on the
      outer `#text(size: …)` call at `src/render/mod.rs:1871`, for a literal and a resolved reference
      alike. No author-supplied string reaches the generated source.
- [x] 4.4 Test the emitted Typst source: a named ink and a hex ink each emit the same `rgb(...)` form
      with the pinned components, and an item with no ink emits no `fill:` at all.
- [x] 4.5 Test that an ink changes no metric: the same item with and without an ink resolves to the
      same box, the same fitted font size and the same line breaks.

## 5. The error contract over HTTP

- [x] 5.1 HTTP test: `POST /render/label` supplying a non-colour for a referenced ink returns `400`
      with code `InvalidRequest`, `details.reason` `ink_param_invalid`, and a message naming the
      parameter. Assert the status and the body, not a layer below them.
- [x] 5.2 HTTP test: `POST /batch` with two labels, the second carrying a bad ink, returns `422`
      `BatchInvalid` whose `details.failures` holds one entry with `index` 1, code `InvalidRequest`,
      reason `ink_param_invalid` and the parameter's name, and produces no PDF or ZIP.
- [x] 5.3 HTTP test: a supplied value that is itself `"{other}"` fails the same way.
- [x] 5.4 Test that a `white` ink loads and renders rather than being refused, so legibility stays the
      author's.
- [x] 5.5 Test that a per-slot ink reaches the emitted source for each slot of a multi-slot sheet,
      that the sheet compiles to PDF, and that `color_mode=bilevel` thresholds a light ink to white.
      The sheet loop itself is asserted through separately built contexts, which is this repo's
      existing convention for sheet content (`src/render/mod.rs:8252-8270`).

## 6. Record and verify

- [x] 6.1 Write `docs/adr/0091-text-ink-is-a-full-colour.md`: the full-colour vocabulary, the evidence
      rejecting the monochrome premise (`docs/VISION.md:5`, `docs/SPEC.md:933`, ADR-0033,
      `src/driver.rs:693`), the pinned name table and why it is ours, and what a colour ink does on
      the bilevel path. Add its row to `docs/adr/README.md`.
- [x] 6.2 Confirm the change carries no `MODIFIED` delta and that the `ADDED` requirement naming
      `docs/SPEC.md` §4.1 still names only the `text` field list.
- [x] 6.3 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test`. Fix any lint
      at its root; never silence one with `#[allow]`.

## 7. Round-3 findings: narrow the suffix guard back to `Ink`

The diff review's blocking finding. The `mm`/`in` guard added to `DynamicValue`'s shared visitor
changes `f32` and `u16` parsing too (`SizeValue`, `FontSize`, `font_weight`), which is a behavior
change outside this change's scope and under no delta. Narrow it; do not record it here.

- [x] 7.1 Revert the guard in `DynamicValue::visit_str` (`src/models.rs:337-345`) so the shared
      length-suffix fallback behaves exactly as it did before this change. `font_size: "80mm"` and
      `font_size: "infmm"` must each parse as they did on `main`.
- [x] 7.2 Give the text item's ink field its own `Deserialize` instead: resolve a `{name}` wrapper to
      a reference, otherwise parse through `Ink::from_str`, and never take the length-suffix branch.
      `ink: redmm` and `ink: "#ff0000in"` stay refused, which is what 7.1 would otherwise give back.
- [x] 7.3 Test at both layers. For ink: `chartreuse`, `redmm`, `"#ff0000in"`, `"ff0000"`, `"#ff000"`
      and `""` are refused, and `"{brand}"` still deserializes as a reference. For the shared visitor:
      assert `font_size: "80mm"` parses and `font_size: "infmm"` parses to infinity, so the revert in
      7.1 is pinned in both directions rather than assumed.
- [x] 7.4 Correct the design decision headed "`Option<DynamicValue<Ink>>` needs no new deserialization
      machinery" (`design.md:103-107`). It is false as written: say that the ink field carries its own
      `Deserialize`, and why — the shared length-suffix branch would otherwise accept `redmm` and
      store the stripped spelling, which is the silent rewrite the `spelling` field exists to prevent.

## 8. Round-3 findings: corrections to the record

- [x] 8.1 Fix the citation in `docs/adr/0091-text-ink-is-a-full-colour.md` and in `design.md`:
      `src/driver.rs:693` is inside `#[cfg(test)] mod tests` (opens at `src/driver.rs:674`). The
      production parser is `PrinterCapabilities::from_parts` at `src/driver.rs:440`, and the bi-level
      parse is `src/driver.rs:447`. The claim is right; only the line is wrong. Fix it before the ADR
      lands, because ADRs are append-only.
- [x] 8.2 Move the `ink: 16711680` refusal test onto the path a YAML integer actually takes.
      `src/models.rs:1381` calls `Ink::deserialize` directly and bypasses `DynamicValue`'s `visit_u64`
      (`src/models.rs:297`), which is the code a template integer reaches.
- [x] 8.3 Re-run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test`.
