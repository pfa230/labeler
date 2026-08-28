# 83. A packed child is anchorless, and its container's arrangement places it

Date: 2026-08-28

## Status

Accepted. Issue [#263](https://github.com/pfa230/labeler/issues/263). Amends [ADR-0080](0080-unify-size-resolution.md) §1 and [ADR-0081](0081-size-vocabulary-content-and-fill.md) §1.

## Context

Prior to this decision, layout containers positioned children strictly by absolute coordinates (`at` and `to`). Positioning items sequentially in a row or column required authors to manually calculate offsets and coordinate arithmetic or rely on fixed absolute positions.

Introducing automatic ordering and packing within containers requires separating the **positioning** of an item from its **sizing**:
- An arrangement decides position alone; size remains governed by `layout-sizing` (ADR-0080, ADR-0081).
- A child in a flow container has no authored anchor (`at` / `to`), so resolver quantities defined in terms of an anchor (`available`, `requirement`, and `fill`) must extend to the anchorless case.
- Accumulation along the packing axis must correctly handle zero-extent children, gated-off items (`when`), gaps between occupying children, and coordinate mapping to the bottom-left origin coordinate system.

## Decision

1. **Flow Container Declaration (`flow` block)**:
   A container may declare a `flow` block with a required `direction` (`row` or `column`) and an optional non-negative `gap` (default `0`).
   - `direction: row`: Primary axis is horizontal (+x from left edge); secondary axis is vertical (aligned to top edge).
   - `direction: column`: Primary axis is vertical (−y from top edge); secondary axis is horizontal (aligned to left edge).
   - A rotated container with `flow` packs in author space.

2. **Anchorless Packed Children**:
   A direct child of a flow container is a **packed child**:
   - It carries neither `at` nor `to`. Specifying `at` or `to` is refused at template load.
   - It cannot be a `line` item (refused at load).
   - In `src/resolver.rs`, an absent anchor is classified as `Anchor::Absent`.
   - `available(frame, spec)` for an `Anchor::Absent` item is the full frame extent.
   - `requirement(spec, claim)` for an `Anchor::Absent` item is `claim`.
   - `Anchor::resolve` panics if called on `Anchor::Absent`.
   - In OpenAPI schemas and template serialization, `at` is omitted for packed children, ensuring clean round-tripping via `GET /api/templates/{id}`.

3. **Sizing and the `fill` Keyword on Packed Children (Amending ADR-0080 §1 and ADR-0081 §1)**:
   A packed child is sized against the container's padded inner box:
   - A `fill` packed child resolves against the container's padded inner box (the available extent).
   - Alone in a container, an uncapped `fill` child stretches to the inner extent.
   - Beside a sibling, an uncapped `fill` child claims the full inner extent, causing accumulation overflow and failing with `item_out_of_frame` at render time. Authors cap siblings with `max_w` / `max_h`.
   - A packed container without `size` defaults to `size: [fill, fill]`.

4. **Arrangement and Accumulation (`arrange_flow`)**:
   - A child occupies the primary axis if it is active and its resolved primary extent > 0.
   - The first occupying child starts at the leading edge; each subsequent occupying child is offset by `gap`.
   - Gaps appear only between occupying children (no leading or trailing gaps).
   - Gated-off children (`when`) are skipped entirely without leaving holes or gaps.
   - Active zero-extent children are placed at the leading edge the next occupying child would take, advance nothing, consume no gap, and contribute their secondary extent.
   - Container assembled extent is the sum of occupying requirements + gaps on the primary axis, and the maximum active requirement on the secondary axis.
   - Accumulation bounds checking is evaluated in packing coordinates via `fits_frame(cursor, extent, inner_primary)` before conversion to bottom-left drawing coordinates, preventing coordinate inversion masking.

## Consequences

- Authors can build dynamic rows and columns of content with configurable gaps without manual coordinate arithmetic.
- Load-time validation quarantines templates with invalid flow structures (`at`/`to` on packed child, `line` packed child, invalid `direction` or negative `gap`).
- Overflow on either primary or secondary axis consistently raises `UnsupportedLayoutItem` with reason `item_out_of_frame` (never `coord_out_of_frame`).
