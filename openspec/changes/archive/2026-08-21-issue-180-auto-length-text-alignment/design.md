## Context

See proposal.md - Why for the defect and specs/auto-length-layout/spec.md for the contract.

The constraint that shapes everything here is that the auto-length pipeline runs **twice** over the
same layout, and the two passes must agree on which items they touch:

- `RenderContext::measure` (`src/render/mod.rs:916`) walks the tree against a frame of `width.max`,
  fits every frame-dependent `text` into `min(max_w, budget_remainder)`, pushes a `MeasuredText`,
  and returns the content extent. `render_single_label` clamps that extent into
  `[width.min, width.max]` and that becomes the page width (`src/render/mod.rs:342`).
- The render pass then walks the same tree against a frame of the **final** width, and
  `render_text_item` replays the measured fit by consuming `MeasuredText` entries through a shared
  cursor (`src/render/mod.rs:1364-1376`). Both passes gate on the same
  `Placement::width_is_frame_dependent()` plus the same right-anchor skip, and a mismatch is a hard
  `AutoLengthCursorMismatch` error, so the *set* of items on this branch must not change.

Two further facts constrain the fix:

- The measured lines are emitted verbatim, joined with `#linebreak()`
  (`src/render/mod.rs:1406-1417`), so widening the box cannot re-wrap text or change the chosen font
  size. Only where the glyphs sit inside the box can change.
- A rotated container forces `LengthMode::Fixed` on its children (`src/render/mod.rs:1826-1834`), so
  this branch is unreachable under rotation and no rotation interaction exists.

The immediate precedent is nine lines away: on a dynamic label an `auto`-width **container** already
takes its width from the frame remainder rather than from its content
(`src/render/mod.rs:1665-1674`), with an explicit `min`/`max` rather than the shared
`resolve_size_value` helper.

## Goals / Non-Goals

**Goals:**

- `alignment.horizontal` behaves the same on an auto-width item of a dynamic-width template as on a
  fixed-width item.
- Byte-identical Typst source for every template that does not ask for `center` or `right`.
- The label keeps shrinking to its content: the measurement pass is not touched.

**Non-Goals:**

- No change to `alignment.vertical`, to `Extent::To` text, to `qr`/`image`/`line`/`container`, or to
  any fixed-width or `sheet` template.
- No new template field, error `Reason`, or API surface. No horizontal counterpart to the `#124`
  ink-overflow padding: that problem is vertical (ascenders and descenders leave the cap-height line
  box), and there is no horizontal analogue to reserve.
- Not touching `at.x`-relative *right-anchored* auto text (`at.x` negative). The measurement pass
  skips such an item (clause 1, `src/render/mod.rs:927-940`) and the auto-length replay branch skips
  it too (`!placement.at.x().is_sign_negative()`, `src/render/mod.rs:1365`), so it is never rendered
  by the branch this change edits - but it *is* still rendered, by falling through to the fixed-size
  path (`src/render/mod.rs:1449`), where horizontal alignment already works. On a dynamic-width
  template the combination is rejected at validation anyway
  (`src/templates.rs:1141-1147`, frozen `docs/SPEC.md` §6). Either way, nothing here changes for it.

## Decisions

### 1. Widen the render box, never the measured width

The `Extent::Size` arm of `box_w` (`src/render/mod.rs:1401`) becomes alignment-aware; `measure` is
untouched.

The label's width is `content_extent.clamp(min, max)`, and `content_extent` is built from
`at.x + m.width`. Feeding the alignment slot back into the measurement would make every centred
auto-length label render at `width.max` - it would never shrink to its text again, which is the
entire point of an auto-length tape. So the two numbers must be allowed to differ: the measured
width says *how wide the label should be*, the rendered box says *how much room the glyphs may be
aligned within*. That split is the ADR.

*Alternative rejected:* teach `measure` to record both widths and have the render pass replay the
slot. Same visible result, but it puts a render-only concern into the pass whose output decides the
label width, and it grows `MeasuredText` for nothing.

### 2. Compute the slot explicitly, not through `resolve_size`

```rust
Extent::Size(_) => match alignment.horizontal {
    HorizontalAlign::Left => m.width,
    HorizontalAlign::Center | HorizontalAlign::Right => (self.frame_width_units - left)
        .min(placement.max_w.unwrap_or(f32::INFINITY))
        .max(m.width),
},
```

`self.frame_width_units` is already the correct basis on both sides: at the top level it is the
final clamped label width, and inside a container the child `RenderContext` is built on the padded
inner box (`src/render/mod.rs:1729-1737`), so a nested item centres within its container.

`.min(max_w)` keeps `max_w` binding at render time. `.max(m.width)` is a floor, not an expectation:
`m.width` is measured against a budget derived from `width.max` while the render frame is the
clamped width, so the two are only guaranteed equal-or-wider in the normal case. Taking the max
means the box can never shrink below today's, so no template can start clipping text that renders
today.

*Alternative rejected:* `resolve_size(..., allow_auto_fill: true)`, the call the fixed-width path
makes. It resolves to exactly `min(max_w, frame - at.x)`, but it rejects a result of `<= 0` with
`size_auto_no_room` / `max_size_invalid` (`src/render/mod.rs:1946-1966`). On a dynamic label a zero
remainder is a legitimate outcome of measurement rather than an authoring error - which is precisely
why the container path next door avoids the helper too (`src/render/mod.rs:1667-1671`). Reusing it
here would convert a degenerate-but-renderable layout into a 422.

### 3. Gate on `horizontal != left`

`left` keeps the fitted-width box. Visually the two are identical - the glyphs start at `at.x`
either way - so the gate buys blast radius, not behavior: every bundled template and every user
template that never sets `alignment.horizontal` (the schema default is `left`,
`src/models.rs:543-548`) emits the same source it emits today, and the existing auto-length render
tests keep asserting the same widths.

It is not free: the box also clips (`clip: true`), so under `left` a fitted box still clips ink that
overflows the measured width while a centred box would show a little more. Keeping that difference
is the conservative choice - it preserves today's behavior everywhere it is observable - and #180
prescribes this shape.

*Alternative rejected:* always use the slot. Simpler to state and arguably more principled, but it
rewrites the emitted source for every auto-length template in the catalog for zero visible gain on
the `left` ones, and changes the clip boundary for all of them.

### 4. ADR-0059

Adds `docs/adr/0059-auto-length-text-box-is-the-alignment-slot.md` plus its row in
`docs/adr/README.md`: on an auto-length label a text item's measured width and its rendered box
width are two different numbers, and `alignment.horizontal` is what chooses between them. 0059 is
the next free number (`docs/adr/README.md` ends at 0058, and no in-flight worktree claims it).
ADR-0026 (auto-length) and ADR-0053 (`max_*` caps) are related but not superseded: neither states
the render box rule this change introduces.

### 5. The stale comment in `measure` is part of the change

`src/render/mod.rs:990-991` currently reads "The cap binds here, not at render: the rendered box for
this item is exactly `m.width`, so capping the budget is what caps the width." After decision 2 the
render side applies `max_w` itself for centred and right-aligned items. The comment is corrected in
the same commit; leaving it is how the next reader re-introduces this bug.

## Risks / Trade-offs

- **A centred item now draws a box wider than the text, and boxes are clipped to the frame.** →
  `check_box_bounds` already runs on `box_w` (`src/render/mod.rs:1403`) and the slot is
  `frame_width_units - left` by construction, so `left + box_w == frame_width_units` exactly, within
  the helper's `1e-4` epsilon. The `.max(m.width)` floor can exceed the frame only in cases that
  already exceed it today with the same value.
- **Two items on one line.** A centred auto-width item now occupies the whole remaining width, so a
  second item placed to its right overlaps its (transparent) box. Nothing is drawn there, and Typst
  places both absolutely, so this is a z-order non-event - but the box is no longer a reliable proxy
  for "where the ink is". Called out in the ADR.
- **`m.width` vs the final frame.** If a right-anchored sibling forces the label wider than this
  item's own measurement budget implied, the slot grows with it and a centred item re-centres in the
  wider label. That is the intended reading of "centre in the slot", but it means a centred item's
  position can depend on an unrelated sibling's inset. Documented in the ADR; no code guard.
- **Verification is visual.** Unit tests asserting the emitted `#box(width: ...)` are necessary but
  not sufficient (AGENTS.md - "Templates are visual artifacts"). The task list, written after this
  review, MUST therefore carry an explicit render-and-look step on `brother_12mm` at `width.min` for
  `center` and `right` - rendering to PNG and opening the image - alongside the three gates
  `cargo fmt`, `cargo clippy --all-targets --all-features`, and `cargo test`.
