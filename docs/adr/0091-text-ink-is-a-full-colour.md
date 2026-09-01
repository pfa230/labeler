# 91. Text ink is a full-colour RGBA value

Date: 2026-08-31

## Status

Accepted (vocabulary and naming clauses superseded by [ADR-0093](0093-one-colour-type-and-vocabulary.md)). Issue [#282](https://github.com/pfa230/labeler/issues/282). Supersedes nothing.

## Context

Labeler historically assumed all text rendered in the default text colour (black ink). However, label templates frequently target physical media or environments requiring distinct text colours—such as hazard labels in red, accent text in blue, or light text on coloured tape stock.

An initial premise might assume that because thermal label printers are monochrome, label styling should be restricted to bi-level black/white or validated against printer capabilities. However, several architectural facts reject this monochrome premise:
1. `docs/VISION.md:5` establishes Labeler as a general-purpose label printing engine targeting both continuous tape printers and multi-label sheet media on standard laser/inkjet printers.
2. `docs/SPEC.md:933` and ADR-0033 describe capability-aware rendering where format conversion and colour mode (`bilevel` vs `color`) are negotiated per printer at print time, not baked into template authoring or validation.
3. `src/driver.rs:447` (`PrinterCapabilities::from_parts` at `src/driver.rs:440`) advertises bi-level as a printer capability parsed at runtime, while the actual luminance thresholding is handled downstream during raster preparation (`binarize_rgba` at `src/render/helpers.rs:18`).

Several design questions arose:
1. Should ink be authored per text item, on containers, or as a global template property?
2. What colour vocabulary and representation should be accepted?
3. How should colour interact with text measurement, font fitting, and rendering?
4. How should downstream printer drivers handle physical colour capabilities?

## Decision

1. **`ink` is a property of `Text` items.** It is not placed on `Container` or inherited recursively. Container styling is limited to framing, padding, and layout flow; text styling belongs to the text layout item itself.
2. **`Ink` is a full-colour RGBA model with a pinned vocabulary.** Colour is not constrained to bi-level monochrome at the template authoring or validation boundary. The authoring format accepts 18 standard pinned colour names (`black`, `gray`, `silver`, `white`, `navy`, `blue`, `aqua`, `teal`, `eastern`, `purple`, `fuchsia`, `maroon`, `red`, `orange`, `yellow`, `olive`, `green`, `lime`) pinned to the values Typst documents today, as well as `#`-prefixed hex strings in 3, 4, 6, or 8 hexadecimal digit formats (`#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`).
3. **Typst `#text(..., fill: rgb(...))` handles the paint.** The rendering pipeline emits a `fill: rgb(r, g, b, a)` argument on the outer `#text(...)` wrapper in Typst markup when `ink` is specified. If `ink` is omitted (`None`), no `fill:` argument is emitted and Typst uses its default paint.
4. **Text measurement is strictly colour-agnostic.** Font metrics, glyph sizing, auto-shrink fitting, line breaking, and bounding box calculations depend solely on typeface, font size, font weight, and string content. `ink` has zero effect on measurement passes (`measure_items`).
5. **Print drivers and render targets handle physical capabilities downstream.** Full-colour raster and vector targets (PDF, colour PNG) preserve exact RGBA values. Bi-level rasterization (`color_mode=bilevel`) applies luminance thresholding downstream during raster image conversion, converting light inks (including `white` or `yellow`) to white pixels and dark inks to black pixels.

## Consequences

- Templates can author rich colour styling for colour PDF sheets, multi-slot labels, and web previews without template validation failing on colour printer checks.
- Invalid ink strings (unsupported names or malformed hex) are rejected at template load time (`TemplateInvalid`) or parameter resolution time (`InvalidRequest` with reason `ink_param_invalid`).
- Font fitting and layout performance remain unaffected because ink is ignored in the measurement pass.
- Single-bit printers and thermal printers receive appropriate thresholded bi-level rasters without requiring changes to the template schema.
