## Why

The renderer resolves a child's final box, checks it against the frame and the sheet stock, loads its content and writes its Typst markup in one pass, so nothing can prepare a child and learn whether it fails without also drawing it. #260's ordered packed fill needs exactly that to prepare an allocated child and to choose a trim marker, and its unmerged attempt failed review where the two concerns were tangled. This refactor gives preparation and drawing an explicit boundary on main first, without changing behavior.

Source issue: https://github.com/pfa230/labeler/issues/495

Fixes #495

## What Changes

- Split `render_items` into a preparation walk, which produces a tree of prepared items carrying each item's final box, text fit, resolved weight, loaded content and arranged children, and an emission walk, which writes Typst markup from that tree.
- Move every check and lookup that can fail for a template or request reason into preparation, in the order the renderer performs it today, so the first error a render reports is unchanged.
- Leave measurement, the shared resolver, the `render_items` signature and every existing test unchanged.

## Capabilities

### New Capabilities

### Modified Capabilities

None. This is a behavior-preserving refactor; `.openspec.yaml` sets `skip_specs: true`.

## Impact

`src/render/mod.rs` only. No API, template schema, layout, rendering or error contract changes, and no new dependencies. Callers of `render_items` in `src/render/mod.rs` and `src/templates.rs` keep compiling unchanged. #260's allocation and refitting rules stay in #260.
