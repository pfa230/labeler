# 36. Layout-aware container rotation (container-only, parent-frame, CCW orthogonal)

Date: 2026-06-29

## Status

Accepted

## Context

The vertical `avery5163` variant is a portrait design that must be rotated onto a landscape slot
("read by turning the box"). A `rotate: Option<f32>` field already existed on the shared `Placement`
(embedded in the Text, Qr, Image, and Container variants of `LayoutItem`; `Line` has none), but it was
naive: `wrap_rotation` emitted `#rotate({n}deg)[…]` with no `reflow`, the rotated content (absolute
`#place` children) had zero intrinsic size so Typst kept the unrotated box, and a container's frame
`#rect` and its children were rotated as two separate placements. Bounds validation rejected nothing
useful for rotation, and the dynamic-width `measure()` pass would walk a rotated subtree in
physical-horizontal terms.

A font-weight research run and two codex design/plan reviews established the constraints: Typst groups
faces and lays out via an explicitly sized box, Typst positive angles rotate clockwise (so CCW needs
negation), and `#place` children must be wrapped in a sized `#box` before `#rotate(reflow: true)` for
reflow to compute a real footprint.

## Decision

1. **Container-only, orthogonal, counter-clockwise.** `rotate` is valid only on a `container` and must
   canonicalize (via `rem_euclid(360)`, tolerance `1e-3`) to one of `{0,90,180,270}`. A `Rotation`
   enum (`models.rs`) interprets the stored `f32`; the wire format and OpenAPI schema stay "optional
   number" (no `PlacementRaw` split, no custom serde). `rotate` on a non-container item, or a
   non-orthogonal value, is a validation error.

2. **`at`/`size` stay parent-frame; rotation is an inner transform.** A rotated container is placed
   and bounds-checked exactly like an unrotated one (its `size` is its footprint in the parent), so
   nested rotated containers compose without compounding coordinate flips.

3. **Explicit author canvas, single reflowed rotation, unrotated physical frame.** The renderer builds
   the children inside an explicitly sized `#box` (the full physical box, dimensions swapped for
   90/270), wraps it in one `#rotate(<−deg for CCW>, reflow: true)`, and `#place`s it once at the
   physical container origin. The frame `#rect(W×H)` is emitted unrotated. `R0` takes the prior path
   unchanged (output byte-identical).

4. **Author-space padding; swapped child bounds.** Padding is subtracted inside the author canvas (it
   rotates with the design), and for 90/270 child bounds validate against the swapped author content
   area `[inner_h, inner_w]`, rejecting a non-positive content area.

5. **No `auto` under rotation.** A rotated container must have an explicit `size`, and no descendant
   may use `auto`; the `measure()` dynamic-width pass short-circuits at a rotated-container boundary.

## Consequences

- A portrait container with `rotate: 90` renders rotated CCW onto a landscape slot, verified by
  per-rotation corner-mapping render tests (QR marker quadrant) and a manual visual check (#67).
- The CCW sign is `#rotate(-90deg)` for R90 / `#rotate(90deg)` for R270 (Typst positive = clockwise).
- The size logic now lives in three synced places (validate, render, measure); the no-`auto`-under-
  rotation rule keeps the measure pass from needing a swapped model.
- Rejected alternatives: declaring the container at its natural portrait (taller-than-slot) outer size
  and footprint-mapping placement (makes nesting confusing); arbitrary (non-orthogonal) angles
  (sin/cos bounds, fuzzy fit); rotation on all item types (leaf items have no inner space to swap);
  physical-space padding (would need per-angle side permutation); supporting `auto` under rotation
  (a third divergent size path). A follow-up issue can add rotated auto-width if ever needed.
- Relationships: enables the `avery5163` rebuild (#95), which consumes this capability; independent of
  the deferred font-weight work (#101/#97/#96).
