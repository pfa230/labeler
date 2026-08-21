# 59. Auto-length text box is the alignment slot

Date: 2026-08-21

## Status

Accepted. Issue [#180](https://github.com/pfa230/labeler/issues/180). Related to [0026](0026-auto-length-dynamic-width.md) and [0053](0053-max-bounds-cap.md).

## Context

On dynamic-width (`single` auto-length) templates, `alignment.horizontal` was silently ignored for
`auto`-width `text` items. A template setting `horizontal: center` rendered short messages flush
against the left padding instead of centred on the label.

The root cause was that the auto-length render branch emitted the item's `#box` at exactly the
fitted width of its measured text (`m.width`). Wrapping `#align(... + center)` around a box the text
already fills is a no-op: `#align` requires slack within the enclosing box to position the text.
The fixed-width render path, in contrast, resolved its box against the frame remainder and centred
correctly.

The bug was hidden in common cases because an auto-length label normally shrinks to its content extent,
leaving zero slack between the text width and the label width. It surfaced when a gap opened:
either when the label clamped to `width.min`, or when `width.max` or a sibling item forced a wider frame.

## Decision

**On an auto-length label, a text item's measured width and its rendered box width are two separate numbers.**

1. **Measurement pass is untouched.** The pre-render measurement pass continues to fit text against its
   budget and contributes `at.x + fitted_width` to the label's content extent. The label width remains
   `content_extent.clamp(min, max)`. Feeding the alignment slot back into measurement would force every
   centred auto-length label to expand to `width.max`, defeating auto-length shrinking.
2. **Render box is the alignment slot for `center` and `right`.** For `horizontal: center` and
   `horizontal: right`, the rendered box width `box_w` is computed as the remaining frame width from
   the resolved `at.x` (`frame_width_units - left`), capped by `placement.max_w` (if set) and floored
   at `m.width`:
   ```rust
   (self.frame_width_units - left)
       .min(placement.max_w.unwrap_or(f32::INFINITY))
       .max(m.width)
   ```
   `self.frame_width_units` is the final clamped label width at top level, or the padded inner width
   when nested in a container, so nested items centre within their container.
3. **`left` alignment keeps the fitted-width box.** `HorizontalAlign::Left` (the default) continues to
   emit `box_w = m.width`. This guarantees byte-identical Typst source and preservation of existing
   clipping boundaries for all templates that do not specify horizontal alignment.

## Consequences

- `alignment.horizontal: center` and `right` now work consistently on auto-length labels when slack
  exists (e.g. Brother tape templates clamped at `width.min`).
- **Box width is no longer an ink-only proxy.** A centred auto-width item's box now spans the full
  remaining slot width. A sibling item placed to its right will visually overlap this transparent box
  (though Typst places both absolutely, so z-ordering is unaffected).
- **Sibling-induced slot expansion.** If an edge-relative or right-anchored sibling forces the dynamic
  label wider than this item's text fit, the alignment slot expands accordingly and a centred text
  re-centres in the wider label.
- ADR-0026 and ADR-0053 remain accepted and unchanged.
