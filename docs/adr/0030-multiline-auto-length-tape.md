# 30. Multi-line auto-length tape labels

Date: 2026-06-25

## Status

Accepted. Extends [ADR-0026](0026-auto-length-dynamic-width.md) (auto-length dynamic-width single
labels). Related to issue [#78](https://github.com/pfa230/labeler/issues/78).

## Context

ADR-0026 introduced auto-length rendering for continuous-tape singles but explicitly rejected
`multiline: true` on dynamic-width templates, noting that "line-break affects width, which affects
line-break" creates an ambiguity. Issue #78 resolves that ambiguity by fixing the wrap budget: text
wraps against the item's auto-width budget (`width.max - at.x`, minus any container padding) so the
maximum label extent is bounded before wrapping begins.

The key insight is that `max` already caps tape use. Wrapping inside that cap and then measuring the
longest wrapped line to set the actual label width gives a deterministic algorithm with no circular
dependency.

A second gap in ADR-0026 was that `alignment.vertical` for auto-length text items was hardcoded to
center regardless of the schema value. This ADR corrects that: `vertical` is now honored literally,
and the schema default is `top`.

## Decision

**Multiline is now allowed on dynamic-width single templates.** The validation-time rejection added
in ADR-0026 is removed.

**Wrap algorithm.** For a `text` item with `multiline: true` on a dynamic-width `single` template:

1. Compute the item's auto-width budget: `width.max - at.x` (minus container padding if the item
   is inside a container).
2. Starting from `font_size.max`, attempt to lay out wrapped lines inside the budget using `fontdue`
   metrics. Count lines as `floor(available_height / line_height)` at the current font size.
3. If the text does not fit within the line count at `font_size.max`, shrink in 0.5 pt steps toward
   `font_size.min` and retry. The line count is emergent: it grows as the font shrinks, so smaller
   fonts can accommodate more lines.
4. If the content still overflows at `font_size.min`, keep the fitting lines and ellipsize the last
   one.
5. The tape label extent is `at.x + longest_wrapped_line_width`, clamped to `[width.min, width.max]`.

**Measure / render consistency rule.** The precomputed wrapped lines (and the chosen font size) are
stored in a `MeasuredText` struct and passed directly to the render pass. The render pass emits each
line verbatim and never re-wraps. Each emitted line has its spaces replaced with non-breaking spaces
(NBSP) so Typst cannot break lines again. This preserves the guarantee from ADR-0026: the measurement
and render passes share state and cannot diverge.

**`alignment.vertical` is honored literally.** `Top`, `Center`, and `Bottom` are all respected. The
schema default is `top`. Auto-length items that omit `alignment.vertical` now use `top` rather than
the old implicit center. The bundled tape templates (`brother_12mm`, `brother_18mm`, `brother_24mm`,
`brother_18mm_qr`, `brother_24mm_qr`) already set `vertical: center` explicitly and are unaffected.
Authors adding or migrating tape templates who relied on the implicit centering must add
`vertical: center` explicitly.

**Single-line auto-length also honors `alignment.vertical`.** The same fix applies to
`multiline: false` auto-length items; previously they also used a hardcoded center offset.

**Bundled example template.** `templates/brother_24mm_multiline.yaml` ships with the feature as an
example of a multiline continuous-tape template.

**Unchanged.** `multiline: false` single-line auto-length, fixed-width multiline, and sheet labels
are unaffected.

## Consequences

- Continuous-tape labels can now wrap text across multiple lines, using the tape length to show the
  longest line and fitting as many lines as the label height allows at the chosen font size.
- Template authors control the auto-shrink range via `font_size: { min, max }`. A narrow min/max
  range may mean fewer wrapped lines; a wide range allows more aggressive shrinking to fit more.
- The measure/render-consistency rule from ADR-0026 is extended: adding any new measurement that
  affects line layout requires updating `MeasuredText` and threading the result through to the render
  pass (never re-measuring in the render pass).
- `alignment.vertical` default changes from implicit `center` to schema default `top` for auto-length
  items. Any template that depended on the implicit centering needs an explicit `vertical: center`.
- The bundled templates are unaffected because they already set `vertical: center`.
