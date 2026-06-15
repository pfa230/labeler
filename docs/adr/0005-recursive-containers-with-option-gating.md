# 5. Recursive containers with option gating

**Status:** Accepted

## Context

A single physical label often needs to support multiple layouts (e.g. a horizontal vs. vertical
arrangement of the same data), and groups of items need shared positioning, padding, and an optional
outline frame. Encoding each variant as a separate template would duplicate metadata and data-binding,
and a flat layout list cannot express grouped, relatively-positioned content.

## Decision

Model `layout` as a tree. `container` is a recursive layout item that nests `items`, owns a padded
inner coordinate frame, and may carry an `option` map. Templates declare `options` (name → allowed
values); a request supplies an `option` selection; a container renders only when its `option` entries
all match the selection. A dedicated `frame` (outline) on the container replaced an earlier standalone
rectangle item.

## Consequences

- One template expresses multiple variants selected at request time (see `templates/avery5163.yaml`).
- Rendering and validation recurse, and option validation happens both at the request boundary and per
  container.
- The layout model is a tree, so any traversal (validation, rendering, future export) must handle
  arbitrary nesting.
