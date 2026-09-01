## Why

Implements [#287](https://github.com/pfa230/labeler/issues/287).

A container's outline is the only way to draw anything with an interior, and the geometry it draws is
not a field: the renderer emits one `#rect` and there is no place to ask for anything else. A label's
rectangles and ellipses exist to hold text, so the painted thing has to be the thing that holds
children; any model where an author places a shape and then places a container over it leaves two
items coordinated by hand with nothing checking they still agree.

#280 gives every shape a paint vocabulary and states in its own purpose that "a shape added later
inherits the spelling rather than inventing one". This change is that shape, added to the container
itself rather than beside it.

## What Changes

- `container` accepts **`shape:`**, one of `rect`, `ellipse` or `circle`, defaulting to `rect`. Every
  existing template renders identically; the two clipping changes below are why that needed checking
  rather than asserting.
- `ellipse` fills the container's resolved box exactly. `circle` is the same ellipse, refused unless
  that box is square, so a later edit to `size` cannot silently turn a circle into an oval.
- `rounded` (#280's authored corner radius) is accepted on `rect` alone and refused on the round
  geometries, which have no corners.
- A container's paint follows its geometry rather than always being the box's rectangle. Everything
  else #280 says about paint is unchanged: it covers the padding band, it draws behind the children,
  its stroke is centred on the boundary and grows nothing, and it does not rotate.
- The rectangular case emits one `#box` carrying its own fill, stroke, radius, clip and children,
  instead of a `#rect` overlay placed beside a separate child `#box`.
- A container therefore clips its children to the boundary of the element that holds them. At `rect`
  that is the painted box itself, so the clip follows the corner radius **and** the inner edge of the
  stroke: with no stroke it is the painted boundary, with one it is the concentric curve half a
  thickness inside it. At a round geometry the painted curve is not the child-holding element, because
  the emitter has no round box, so the clip stays the whole rectangle.

Not breaking. `shape` is new and optional, and its default is the current behavior. Collapsing to one
box reverses two clipping rules #280 stated, both consequences of the same move and both invisible in
this repository:

- **The corner radius now clips.** #280 said the radius governed the paint alone. No template renders
  differently, because `rounded` became a number only with #280 and no template in the repository
  sets it.
- **A stroke now cuts child ink at its inner edge.** Today the child box is unstroked and clips at the
  outer box, so child ink draws over the stroke. On one box the emitter builds the clip from the
  stroke's inner control points, insetting it by half the thickness. No template renders differently,
  because every stroked `container` in the repository declares `items: []`
  (`tests/fixtures/templates/avery5163_asset_tag.yaml:43-51` is the only one); every other `stroke:`
  sits on a `line`. Layout is untouched either way: children keep their coordinates and extents, and
  only ink crossing into the stroke band is cut.

Both match `border-radius` with `overflow: hidden`, which rounds the clip and clips inside the border,
at the padding edge. The alternative, keeping the paint off the child-holding box, is the two-element
overlay this change exists to remove, and it would leave the container a rectangle with a decoration
laid over it rather than one drawn thing.

## Capabilities

### New Capabilities

None. The geometry belongs beside the paint that fills it, in the capability #280 creates for exactly
that reason.

### Modified Capabilities

- `shape-paint`: adds the geometry a container's box is painted with, and scopes the existing corner
  radius and paint-coverage requirements to it.

**Base.** `shape-paint` is already in `openspec/specs/`, landed by #280 (`b57bc43`) and since edited
by #291 (`742b602`), which moved the colour vocabulary into its own `colour-vocabulary` capability and
added a parameter-reference scenario. The `MODIFIED` blocks in this delta are copied from that landed
spec, not from #280's delta, so they carry those edits forward.

## Impact

- `src/raw.rs`, `src/models.rs`, `src/convert.rs`: the geometry field, its three layers, and its
  refusals.
- `src/resolver.rs`: the classifier records whether an extent is fixed by the template, alongside the
  source it already records, so validation branches on resolver output and no second reading of the
  spelling appears outside it.
- `src/render/mod.rs`: geometry-aware emission for a container, and the collapse of the `#rect`
  overlay at `2097-2121` into the container's own `#box`.
- `src/templates.rs`: the load-time squareness check for `circle`, gated on that resolver flag.
- `src/openapi.rs`: the new model registered.
- `src/reason.rs`: one new render-time reason for an active `circle` whose box resolves non-square.
  Every active `circle` reaches that check, including one sized by a parameter reference whose default
  was square; a gated-off one is never measured and never reaches it. A load-time refusal is ordinary
  `TemplateInvalid` quarantine and uses no new reason, and in a batch the failure rides the existing
  `BatchInvalid` `details.failures` envelope.
- No ADR. `docs/adr/` is frozen at ADR-0091 (#285, `8100b4f`); the rationale for this change lives in
  this proposal and in `design.md`, kept under `openspec/changes/archive/`.
- No API surface changes beyond the template schema, its OpenAPI model, and that reason string.
