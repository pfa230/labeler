# 41. Vertical text alignment is delegated to Typst, not computed from font metrics

Date: 2026-07-31

## Status

Accepted. Corrects how [ADR-0030](0030-multiline-auto-length-tape.md) ("`alignment.vertical` is
honored literally") is implemented for auto-length text. Issue
[#123](https://github.com/pfa230/labeler/issues/123).

## Context

Tape text rendered high in its slot despite `vertical: center` (-1.41 mm on `brother_12mm`,
-1.90 mm on two-line `brother_24mm_multiline`). The auto-length path placed the text block itself,
sizing it from `fontdue`'s full line height (~1.21 em) and offsetting it with a hand-computed `dy`.
But Typst's default line box runs cap-height to baseline (~0.73 em) and sits at the top of that
block, so centering the block left the glyphs ~0.24 em high; `bottom` floated ~0.48 em. The
fixed-size path never had the bug: it boxes the slot and calls `#align`.

## Decision

Both text paths emit a slot-height box and delegate placement to `#align`. Neither derives a
vertical offset from font metrics; the `line_height_units` helper is deleted.

`alignment.vertical` is therefore defined against Typst's line box (cap-height to baseline), not the
font's full line height: `top` pins the first line's cap-height and `bottom` the last baseline.
Descenders fall outside that box, so `bottom` clips them at the slot edge — the fixed-size path has
always behaved this way (verified with a probe template), and auto-length now matches rather than
floating text up to hide it. Recorded in SPEC §3.1; making the line box descender-aware is #124.

`fontdue` metrics remain correct for *fitting* (auto-shrink, wrap, line-count budget), which is a
different question from placement.

## Consequences

- Tape text moves down up to ~0.24 em (`center`) / ~0.48 em (`bottom`) versus previous output; `top`
  moves up by the former leading offset. No template changes needed.
- The emitted box is the slot, not the measured block, so `clip: true` now clips at the slot edge.
- Regression coverage is pixel-level (`autolength_text_centers_vertically`,
  `autolength_text_top_and_bottom_pin_to_slot_edges`): they assert the rendered ink box against the
  slot rather than the emitted source.
