## 1. Schema: the flag is `wrap`, and the old spelling is refused

- [x] 1.1 In `src/raw.rs`, rename `TextRaw`'s field to `wrap: bool` and add
      `#[serde(default, deserialize_with = "deserialize_present")] multiline: Option<serde_yaml_ng::Value>`
      so the old key is captured on presence, an explicit YAML null included.
- [x] 1.2 In `src/convert.rs`, refuse a captured `multiline` with `TemplateError::Validation` whose path
      is the layout path and whose message names the rename; pass `wrap` into the domain model.
- [x] 1.3 Rename the field in `src/models.rs`, in `src/templates.rs` where the layout item is cloned
      (`:1197-1218`), in `src/batch.rs`, and in `src/openapi.rs`.
- [x] 1.4 Tests: `multiline: true`, `multiline: false`, `multiline: "yes"` and `multiline:` (null) are
      each quarantined with an error naming file, layout path and rename; the service still starts and
      serves the other templates; the write endpoint refuses the same body and writes no file. Prove
      each red before green.

## 2. Step 1 segments instead of selecting

- [x] 2.1 In `layout_text` (`src/render/helpers.rs:640`), replace the
      `if item.multiline { .. } else { raw_text.lines().next() }` selection (`:656-659`) with
      segmentation: normalise `\r\n` to `\n`, split on `\n`, keep every segment.
- [x] 2.2 Apply `wrap_text` per segment when `wrap` is true; pass segments through when false.
- [x] 2.3 Remove the empty-string early return (`:662`) so an empty value is one empty line.
- [x] 2.4 Fix `wrap_text` (`:834`, `:900`) so a whitespace-only segment yields an empty line instead of
      vanishing.
- [x] 2.5 Helper tests: `"abc\r\nabc"` matches `"abc\nabc"` in line count, chosen size and width, with a
      guard asserting a bare `\r` still measures as `.notdef` so the test cannot pass vacuously; an empty
      value is one line; a whitespace-only segment keeps its line.

## 3. Step 4 stops trimming, and emission carries the fitted metrics

- [x] 3.1 Delete the blank-edge trim and the `lines_to_trim` binding; every segment reaches emission.
- [x] 3.2 Report intrinsic height as the block height of the lines emitted **after** the overflow
      policy, at the chosen size.
- [x] 3.3 In `render_text_item` (`src/render/mod.rs:1521-1537`), move `size` and `weight` from each
      `#text` to a wrapper around the whole block, so every line, `#linebreak()` and fallback run
      inherits the fitted values.
- [x] 3.4 Emit a trailing `#linebreak()` when the last line is empty, so a trailing blank occupies a box
      Typst would otherwise give it none.
- [x] 3.5 Render tests at a font size well away from the default: a leading blank, an interior blank, a
      trailing blank and an empty value each produce a rendered block height matching what the fitter
      measured for the same value.

## 4. The overflow marker reports the field

- [x] 4.1 Make `Overflow::Ellipsis` add the marker whenever any line was dropped, not only when the
      retained block fails to fit, and land it on the last retained line whatever that line holds.
- [x] 4.2 Leave `Overflow::Fail` alone: a dropped line is already content that did not fit.
- [x] 4.3 Tests: `"message\n"` in a one-line box wide enough for `message` but not `message...` emits a
      marked form with at least one character removed; `"\nmessage"` in a one-line box at least as wide
      as `...` emits `...`; a value whose every line is shown carries no marker.

## 5. Update template fixtures and the bundled catalog

- [x] 5.1 In `catalog/tape/brother/*.yaml` and `catalog/sheet/avery/avery5163.yaml`, rewrite `multiline:`
      to `wrap:` on layout items; in `avery5163.yaml` keep `params.message.multiline: true` because
      that is the parameter-type flag (spec, Decision 1).
- [x] 5.2 In `tests/fixtures/templates/*.yaml`, rewrite `multiline:` to `wrap:`.

## 6. Decisions and documentation

- [x] 6.1 Record the decisions in `docs/adr/0085-text-wrap-flag.md`: the flag renamed to `wrap`; step 1
      segments every `\n` rather than selecting the first line; step 4's blank-edge trim removed; the
      shortening marker reported at the field level. It supersedes ADR-0045's blank-edge rule and
      amends ADR-0082's step 1, step 4 and overflow decision. The input list's control derivation is
      **not** part of it: that is #269, and the ADR says so.
- [x] 6.2 Add `0085-text-wrap-flag.md` to `docs/adr/README.md`, after 0083 and 0084, which landed while
      this change was in review.
- [x] 6.3 Update `docs/AUTHORING.md` to use `wrap: true|false` on text items and to state the form
      control accurately: a declared parameter's control comes from `params.<field>.multiline`, while an
      undeclared field still takes a textarea from a wrapping item until #269 removes that fallback.

## 7. Gates

- [x] 7.1 Run `cargo fmt --check`
- [x] 7.2 Run `cargo clippy --all-targets --all-features`
- [x] 7.3 Run `cargo test`
- [x] 7.4 Run `(cd ui && npm run lint && npm test)`
