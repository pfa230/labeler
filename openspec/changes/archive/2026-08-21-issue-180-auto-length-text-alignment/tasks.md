## 1. Prove the bug first

- [x] 1.1 Add a `text_source_h_aligned(horizontal, vertical, size_w, font_size, text)` helper beside
      `text_source_aligned` (`src/render/mod.rs:2125`), so a test can drive the auto-length path with a
      chosen `alignment.horizontal` on a frame wider than the fitted text. Keep `text_source` and
      `text_source_aligned` as they are: existing tests must keep passing unchanged.
- [x] 1.2 Write the failing unit test: an `auto`-width text at `at.x = 0` with
      `horizontal: center` on a dynamic frame emits `#box(width: <frame width>...)`, not the fitted
      text width. Run it and **confirm it fails** against the current code, quoting the assertion
      output. A test that passes before the fix is not a test of this bug.
- [x] 1.3 Same for `horizontal: right`, and an assertion that `horizontal: left` still emits the
      fitted width (the byte-identical guarantee in specs/auto-length-layout/spec.md).

## 2. Fix the render box

- [x] 2.1 In `render_text_item`, make the `Extent::Size` arm of `box_w` (`src/render/mod.rs:1390-1402`)
      alignment-aware, exactly as design.md decision 2 specifies:
      `left => m.width`, `center | right => (self.frame_width_units - left).min(max_w).max(m.width)`.
      Import `HorizontalAlign` in the match if it is not already in scope.
- [x] 2.2 Correct the now-false comment in `measure` (`src/render/mod.rs:990-991`): the render side
      applies `max_w` itself for centred and right-aligned items, so "the rendered box for this item
      is exactly `m.width`" no longer holds. State the measured-vs-rendered split instead.
- [x] 2.3 Re-run 1.2/1.3 and confirm they now pass. Run the whole `render` test module and confirm no
      existing auto-length assertion moved.

## 3. Cover the rest of the contract

- [x] 3.1 Test that `max_w` caps the alignment slot: a centred `auto`-width text with
      `max_w` smaller than the frame remainder emits `#box(width: <max_w>...)`.
- [x] 3.2 Test the container case: a centred `auto`-width text nested in a padded container on a
      dynamic frame gets the container's padded inner remainder, not the label width.
- [x] 3.3 Test the no-regression case end to end: a dynamic-width template whose text fits between
      `width.min` and `width.max` still renders a label sized to its content with `horizontal: center`
      (assert the emitted page width, so the measurement pass is proven untouched).
- [x] 3.4 Test the clamped case end to end: the same template with a short message renders at
      `width.min` and places the text box at the full remaining width.

## 4. Record the decision

- [x] 4.1 Write `docs/adr/0059-auto-length-text-box-is-the-alignment-slot.md` (Nygard: Context,
      Decision, Consequences), covering the measured-width vs rendered-box split, the `left` gate, and
      the two consequences design.md lists as trade-offs (a centred box now occupies the full slot, so
      it is no longer a proxy for where the ink is; and a sibling's inset can widen the slot a centred
      item centres in). Status: Accepted. Do not edit ADR-0026 or ADR-0053.
- [x] 4.2 Add the ADR-0059 row to `docs/adr/README.md`.

## 5. Look at the labels

- [x] 5.1 Run the server (`LABELER_CONFIG_DIR=./config-dev cargo run`) with the Brother tape templates
      available, `POST /api/render/label?format=png` for `brother_12mm` with
      `{"message":"Hi"}`, **open the PNG**, and confirm "Hi" sits centred on the 10mm label rather
      than against the left padding. This is the acceptance criterion from #180; a successful HTTP 200
      is not evidence.
- [x] 5.2 Repeat with a temporary `width.min: 40.0` (large slack makes an off-by-a-padding error
      visible) for `horizontal: center` and for `horizontal: right`, opening both PNGs. Revert the
      temporary edit afterwards.
- [x] 5.3 Render a long message on the same template and confirm the label still grows to the text and
      the glyphs are not clipped.

## 6. Gates

- [x] 6.1 `cargo fmt`
- [x] 6.2 `cargo clippy --all-targets --all-features` (no new warnings; never silence one with
      `#[allow(clippy::...)]`)
- [x] 6.3 `cargo test`
- [x] 6.4 Adversarial review of the **diff** (a second review, separate from review.md, which judged
      the plan). Address every finding or refute it with file:line evidence, then re-review until a
      pass surfaces nothing meaningful.
