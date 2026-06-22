# 26. Auto-length dynamic-width single labels (continuous tape)

Date: 2026-06-22

## Status

Accepted. Extends [ADR-0004](0004-bottom-left-coordinate-system.md) (coordinate system) for dynamic
label dimensions. Related to issue [#77](https://github.com/pfa230/labeler/issues/77).

## Context

Brother TZe and similar continuous-tape printers cut each label to the length needed for the content.
The existing `Dimension` type already accepts a `{ min, max }` object on `format.width`, but prior to
this ADR the renderer resolved it statically to `max` (with `min` as fallback), ignoring the actual
content width. That meant tape labels always printed at the maximum length, wasting tape.

A useful auto-length mode must:

1. Measure the actual content width before choosing the page size.
2. Clamp the measured width to `[min, max]`.
3. Fit the largest font that fills the clamped budget (then truncate with an ellipsis if still over).
4. Keep the measurement and the render pass in sync so font size and label width are consistent.

Multiline text on a dynamic-width single is inherently ambiguous (line-break affects width, which
affects line-break), so it is deferred. Both bounds are required: `min` prevents a label so narrow it
is unreadable; `max` caps tape use and is the render budget.

## Decision

A `single` template whose `format.width` is a dynamic object `{ min, max }` (both required) is
**auto-length**: the label width is determined at render time, not at template definition time.

**Measurement pass.** Before emitting Typst markup, the renderer runs a content-measurement pass over
every layout item in the template. For `text` items with a `font_size` range, this pass calls
`largest_fitting_font` against the `max` budget to find the best font size, then measures the
resulting text width via `fontdue`. For `line` items, the extent is `max(at.x, to.x)`. The largest
measured content width becomes the candidate label width, clamped to `[min, max]`.

**`auto` item width on a dynamic-width label.** For items whose `size[0]` is `auto` (no explicit
width), the width resolves to the content width: `label_width - at.x` (the space remaining from the
item's x anchor to the right edge of the measured label). This resolution happens after the
measurement pass, so the item width and the page width are derived from the same measurement.

**Render pass.** The measured font size and clamped label width are threaded into the render pass via
a `MeasuredText` struct so the two passes share state without re-measuring. The Typst page width is
set to the clamped value.

**Multiline rejected on dynamic-width templates.** A `text` item with `multiline: true` on a
`single` template with a dynamic `format.width` is rejected at validation time with
`422 TemplateInvalid`. Multi-line tape is deferred to issue #78.

**Both bounds required.** A dynamic `format.width` with only one of `min` or `max` is rejected at
parse time with `422 TemplateInvalid`. (A width with only one bound was already accepted by the
`Dimension` type; this ADR adds the two-bound requirement for dynamic single templates.)

**Sheet and fixed-width single templates are unaffected.** The auto-length path is gated on
`Format::Single` with a `Dimension::Dynamic` width; all other paths continue as before.

## Consequences

- Tape labels print at the content-fitted width, not always at `max`.
- Template authors get a simple two-parameter contract: set `min` (readable floor) and `max` (tape
  budget), declare items with `auto` width, and the renderer handles the rest.
- The measurement and render passes must stay in sync. Adding a new item type that contributes to
  label width requires updating the measurement pass alongside the render pass (same discipline as the
  existing validation / render sync noted in ADR-0004).
- Multiline text on a dynamic-width `single` template is a validation error, keeping the rendering
  model simple until #78 defines the line-break / width interaction.
- Both bounds being required is a breaking addition for any dynamic-width `single` already using only
  one bound (none existed in the bundled templates prior to this change).
