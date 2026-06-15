# 4. Bottom-left coordinate system

**Status:** Accepted

## Context

Label and sheet geometry (especially pre-cut sheet vendors like Avery) is conventionally specified from
the bottom-left of the page with y increasing upward. Typst, the rendering backend (ADR-0003), places
content from the top-left with y increasing downward. A coordinate convention had to be chosen for
template authors and reconciled with the engine.

## Decision

Template coordinates use a **bottom-left origin, y-up**, in the template's `unit`. The renderer converts
each placement to Typst space with `dy = frame_height - (y + height)`. A `container` defines a new
coordinate frame: its children are measured against the container's padded inner box, not the page.

## Consequences

- Templates read naturally for label-sheet vendors and match `positions` given bottom-left.
- Every placement path must apply the y-flip, and containers must re-base children — the two most
  error-prone spots in the renderer.
- Validation bounds checks and render-time size resolution both encode this geometry and must stay
  consistent.
