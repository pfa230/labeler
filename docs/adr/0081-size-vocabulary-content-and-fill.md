# 81. The size vocabulary is `content` and `fill`

Date: 2026-08-26

## Status

Accepted. Issue [#226](https://github.com/pfa230/labeler/issues/226). Complements [ADR-0080](0080-unify-size-resolution.md).

## Context

Prior to this decision, the keyword `auto` served multiple contradictory roles across layout items and formats:
- On `text` in dynamic tape labels, `auto` meant "hug the content width".
- On `container` in fixed or dynamic labels, `auto` meant "fill the remaining parent space".
- On `qr` or `image`, `auto` required `max_w` and meant "take the cap value".
- In validation on fixed labels, `auto` was previously rejected or had no clear semantics.

This overloading made template authoring confusing and led template authors to write non-intuitive sizing definitions.

## Decision

1. **Explicit Sizing Keywords**:
   Replace the overloaded `auto` token with two distinct, explicit keywords:
   - `content`: Sized by the item's own intrinsic dimensions (e.g. text rendered bounds, QR matrix size, raster image pixel size).
   - `fill`: Sized to occupy the remaining space within the parent frame from the item's anchor: `parent_frame - resolved_anchor`, less any far-edge margin a `to` reserves, bounded by any authored `max_w` / `max_h`. Under a right-anchored `at: [-a]` that remainder is exactly `a`, on every frame. The remainder is not clamped at zero: an anchor that resolves outside its frame is a refusal, not a zero-width box.
2. **Rejection of `auto`**:
   The keyword `auto` is disallowed in YAML template parsing and is rejected with an informative error:
   `` `auto` was renamed: use `content` to hug the item's own size, or `fill` to stretch to the frame ``.
3. **Container Defaults**:
   When omitted, `container.size` defaults to `[fill, fill]`, matching standard container nesting intent.

## Consequences

- Template authoring semantics are unambiguous and intuitive.
- Sizing behaviors match modern layout systems (e.g., CSS flexbox / grid / SwiftUI `contentShape` vs `fill`).
- Existing templates using `auto` must be migrated to `content` or `fill`.
