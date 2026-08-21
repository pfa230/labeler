## Why

Implements [#180](https://github.com/pfa230/labeler/issues/180).

`alignment.horizontal` is silently ignored for auto-width `text` on a dynamic-width (auto-length)
`single` template. The bundled `catalog/tape/brother/brother_12mm.yaml` asks for
`horizontal: center`, and a short message renders flush against the left padding instead. The cause
is one line: on the auto-length branch, an `Extent::Size` text box is emitted at exactly the
measured fitted width of its own text (`src/render/mod.rs:1401`), so the `#align(... + center)`
wrapped around it (`src/render/mod.rs:1426`) centers the text inside a box the text already fills, a
no-op. The fixed-width path a few lines below resolves its box against the frame
(`src/render/mod.rs:1449`, `allow_auto_fill: true`) and centers correctly, so the same template
behaves one way at a fixed width and another at a dynamic one.

The bug hides most of the time because an auto-length label normally shrinks to its content
(`width_units = content_extent.clamp(min_w, max_w)`, `src/render/mod.rs:342`), leaving no slack to
centre within. It surfaces exactly when the clamp opens a gap: content narrower than `width.min`, or
wider than `width.max`.

## What Changes

- On the auto-length render path, an `auto`-width `text` item whose `alignment.horizontal` is
  `center` or `right` is emitted in a box spanning the **remaining frame width from its resolved
  `at.x`**, capped by `max_w` and never narrower than the measured fitted width, instead of a box
  exactly the fitted width. `#align` then has real slack to work with, so `center` centres and
  `right` sits against the right edge of that slot.
- `horizontal: left` (the schema default) keeps the fitted-width box, so every existing template
  that does not ask for horizontal alignment emits byte-identical Typst source.
- The **measurement** pass is untouched: it keeps contributing the fitted width to the label extent,
  so an auto-length label still shrinks to its content and the `[min, max]` clamp is unchanged. Only
  the render-time box changes.
- Unchanged: `Extent::To` text (its box is already resolved against the frame), `qr`, `image`,
  `line`, `container`, every fixed-width `single`, every `sheet`, and `alignment.vertical` on all
  paths (its box already spans the full slot height, #123/#124).

## Capabilities

### New Capabilities
- `auto-length-layout`: how an item's `auto` width resolves on a dynamic-width (auto-length)
  `single` template, split between what the measurement pass contributes to the label extent and
  what the render pass draws, and how `alignment.horizontal` interacts with the two. Supersedes the
  `docs/SPEC.md` §3.1 sentence "`auto` item width on a dynamic-width label resolves to the content
  width (`label_width - at.x`)." and the §4 clause "On a dynamic-width `single` template (§3.1),
  `auto` width instead resolves to the content width (`label_width - at.x`) derived from the
  pre-render measurement pass for `text`, but still to the frame remainder for `container`", for
  `text` only.

### Modified Capabilities
<!-- None. `openspec/specs/` holds only `template-registry`, which this change does not touch. -->

## Impact

- `src/render/mod.rs:1390-1402`: the `Extent::Size` arm of `box_w` on the auto-length text branch
  becomes alignment-aware. No other call site moves; `check_box_bounds` already runs on the result.
- `src/render/mod.rs:988-994`: the measurement pass keeps its budget cap, but its comment ("the
  rendered box for this item is exactly `m.width`, so capping the budget is what caps the width") is
  no longer true for centred and right-aligned items and must be corrected, since the render side
  now applies `max_w` itself.
- `catalog/tape/brother/brother_12mm.yaml` and its 9mm/18mm/24mm siblings (all four set `horizontal: center`): no YAML edit, but their
  rendered output changes for messages shorter than `width.min`, which is the fix.
- `docs/adr/`: adds ADR-0059 recording that on an auto-length label a text item's measured width and
  its rendered box width are two different numbers, and why.
- No API, schema, or error-contract change: no new `Reason`, no `openapi.rs` edit, no new template
  field. `docs/SPEC.md` stays frozen.
