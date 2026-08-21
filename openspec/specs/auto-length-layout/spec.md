# auto-length-layout Specification

## Purpose
Defines how an `auto` width resolves on a dynamic-width (auto-length) `single` template: what the
pre-render measurement pass contributes to the label's own width, what box the render pass draws for
the same item, and how `alignment.horizontal` decides whether those two widths are the same number.

## Requirements

### Requirement: Auto-width text measures to its content and renders into its alignment slot

On a dynamic-width `single` template (`format.width: { min, max }`), a `text` item whose `size` width
component is `auto` resolves its width in two independent places, and they SHALL NOT be required to
agree:

- **Measurement.** The pre-render pass SHALL fit the text within a budget of
  `min(max_w, frame_remainder)`, where `frame_remainder` is the enclosing frame's width (`width.max`
  at the top level, the container's padded inner width inside a container) less the item's resolved
  `at.x`, and SHALL contribute `at.x + fitted_width` to the label's content extent. The label width
  SHALL remain `content_extent` clamped to `[width.min, width.max]`. This is unchanged by this
  requirement: what the label shrinks to is always the fitted content, never the alignment slot
  below.
- **Rendering.** The box the item is drawn into SHALL be:
  - the **fitted width** when `alignment.horizontal` is `left` (the schema default);
  - the **alignment slot** when `alignment.horizontal` is `center` or `right`, where the alignment
    slot is `min(max_w, frame_remainder)` measured against the **final** label width, and never
    narrower than the fitted width.

  `frame_remainder` at render time is measured against the final (clamped) label width for a
  top-level item, and against the enclosing container's padded inner width for a nested one, so a
  centred item inside a container centres within that container, not within the label.

`alignment.horizontal` SHALL therefore have the same visible effect on an auto-width item of a
dynamic-width template as it has on a fixed-width item: `left` puts the text at `at.x`, `center`
centres it in the slot, and `right` puts its right edge at the slot's right edge.

A text item whose width comes from `to` rather than `size` is unaffected: its box is already the
resolved extent, on both paths.

This requirement governs a `text` item whose resolved `at.x` is non-negative. A **right-anchored**
(edge-relative) `at.x` is outside it: the frozen `docs/SPEC.md` §6 remains authoritative there, and
it forbids combining a right-anchored `at.x` with an `auto` or frame-dependent width on a
dynamic-width template at all. Such an item never reaches the measurement or replay path, so its box
and its alignment are unchanged by this requirement.

This requirement supersedes, for `text` only, the `docs/SPEC.md` §3.1 sentence "`auto` item width on
a dynamic-width label resolves to the content width (`label_width - at.x`)." and the §4 clause "On a
dynamic-width `single` template (§3.1), `auto` width instead resolves to the content width
(`label_width - at.x`) derived from the pre-render measurement pass for `text`, but still to the
frame remainder for `container`". The `container`, `qr`, and `image` halves of those sections, the
`max_w`/`max_h` capping rules, and the zero-remainder rules are unchanged and remain governed by the
frozen `docs/SPEC.md` §4.

#### Scenario: Short centred text on a label clamped to its minimum width

- **WHEN** a dynamic-width `single` template with `width: { min: 10, max: 100 }` renders an
  `auto`-width `text` item at `at.x = 0` with `alignment.horizontal: center`, and the fitted text is
  narrower than the label's minimum width
- **THEN** the label is `width.min` wide
- **AND** the text's box spans the full width remaining from `at.x`, not the fitted text width
- **AND** the text is centred within that box, leaving equal slack on its left and right

#### Scenario: Right alignment reaches the right edge of the slot

- **WHEN** the same template and data render with `alignment.horizontal: right`
- **THEN** the text's right edge sits at the right edge of the slot, one frame remainder from `at.x`

#### Scenario: Left alignment is unchanged

- **WHEN** the same template and data render with `alignment.horizontal: left`, or with `alignment`
  omitted
- **THEN** the text's box is exactly the fitted text width, as before this change
- **AND** the emitted output is byte-identical to what the same template produced previously

#### Scenario: A label still shrinks to text wider than its minimum

- **WHEN** an `auto`-width `text` item with `alignment.horizontal: center` fits at a width between
  `width.min` and `width.max`
- **THEN** the label width is `at.x + fitted_width` clamped into `[width.min, width.max]`, exactly as
  for a left-aligned item
- **AND** the alignment slot equals the fitted width, so centring has no slack to distribute and the
  output is unchanged

#### Scenario: `max_w` caps the alignment slot

- **WHEN** an `auto`-width `text` item with `alignment.horizontal: center` declares a `max_w` smaller
  than the frame remainder, on a label clamped to `width.min`
- **THEN** the text's box is `max_w` wide, not the frame remainder
- **AND** the text is centred within that `max_w` box, whose left edge is at `at.x`

#### Scenario: A centred item inside a container centres within the container

- **WHEN** an `auto`-width `text` item with `alignment.horizontal: center` is nested in a `container`
  with padding, on a label clamped to `width.min`
- **THEN** the text's box spans the container's padded inner width remaining from the item's `at.x`
- **AND** the text is centred within the container's padded inner box, not within the label
