# 80. One size-resolution protocol across validation, measurement, and rendering

Date: 2026-08-26

## Status

Accepted. Issue [#226](https://github.com/pfa230/labeler/issues/226). Supersedes [ADR-0026](0026-auto-length-dynamic-width.md), [ADR-0053](0053-max-bounds-cap.md), [ADR-0054](0054-auto-fallback-position.md), [ADR-0059](0059-auto-length-text-box-is-the-alignment-slot.md). Amends [ADR-0036](0036-container-rotation.md) §5 and [ADR-0051](0051-edge-relative-and-corner-placement.md) §4, §10, §11.

## Context

Prior to this decision, size resolution in the layout engine was split between two parallel, drifting implementations:
1. `templates.rs` (template validation) executed one set of resolution formulas to check layout bounds before printing.
2. `render/mod.rs` (the rendering pipeline) executed a second, separate set of resolution and fallback formulas during the measurement pre-pass and Typst emission.

This split caused recurring defects where templates that passed validation failed at render time, or vice-versa (#152, #155, #180). Furthermore, sizing semantics varied depending on the item type and length mode:
- `auto` had four distinct behaviors across item types (`text`, `container`, `qr`, `image`) and format modes (fixed vs dynamic).
- Containers had separate rotated and unrotated render paths, and rotated containers were restricted from measuring content or allowing `auto` descendants.
- Four separate error reasons (`size_auto_without_max`, `size_auto_no_room`, `container_padding_no_room`, `auto_length_cursor_mismatch`) were required to diagnose divergent resolution failure modes.

## Decision

1. **Unified Resolver Protocol (`src/resolver.rs`)**:
   Establish a single, authoritative sizing protocol used identically by validation, measurement, and rendering:
   - `source_of(placement, axis, geometry_values)` classifies an axis into an `AxisSpec`: one of `Author(f32)` (a number, a parameter value or a constant `to`), `ShrinkingTo` (a `to` whose extent shrinks as the frame grows), `Content` (item-intrinsic) or `Frame` (the available parent space), plus the anchor, the far-edge inset and the spelling the author used. This is the only place a size spelling is given a meaning; every rule downstream reads the `AxisSpec`.
   - `available(frame, axis_spec)` is the space an item has from its anchor. It is signed: an anchor past the frame yields a negative extent, which the bounds rules then refuse rather than silently clamp.
   - `resolve(axis_spec, frame, available, cap, intrinsic)` gives the concrete extent. `intrinsic` is data, not a mode: a stage that measured passes what it measured, and one that cannot passes availability (`resolve_unmeasured`), which makes `content` resolve exactly as `fill` does.
   - `claim` and `requirement(axis_spec, claim)` give what the item reports upward into its parent's frame requirement.
   - `precheck` and `place` hold the bounds and refusal rules, returning a `Violation` that validation and rendering each word in their own vocabulary without owning a copy of the rule.
   - One function is deliberately **not** shared: `intrinsic`, in `render/mod.rs`, is the single item-type dispatch in the whole sizing path. Load never calls it, because load cannot measure.
2. **Container Composition and Axis Swapping**:
   Rotated containers (`rotate: 90 | 180 | 270`) compose uniformly through the resolver. For orthogonal rotations (`90` and `270`), child layout and intrinsic content measurement occur in author space and swap axes `(h, w)` to physical space.
3. **Eliminate Parallel Sizing Arithmetic**:
   Remove all ad-hoc fallback and clamping logic from `src/templates.rs` and `src/render/mod.rs`. Both modules delegate strictly to `src/resolver.rs`.
4. **Reason Code Harmonization**:
   - Withdraw four obsolete reason codes: `size_auto_without_max`, `size_auto_no_room`, `container_padding_no_room`, and `auto_length_cursor_mismatch`.
   - Introduce `intrinsic_size_undefined` for an item whose extent is demanded from its own content — `content` **or** `fill`, since both ask the item what it measures — and which cannot supply that measurement. A `qr` without `module_size` is refused at load instead, because the demand is visible in the template; the reason therefore reaches a request only for an `image` whose dimension metadata is absent on the demanded axis or unparseable.

## Consequences

- Template validation and rendering can never drift in dimension calculation or bounding logic.
- Rotated containers support nested `content` and `fill` children without special casing or blanket bans.
- Sizing code is centralized in one module (`src/resolver.rs`), exercised through the validation and render suites that call it.
