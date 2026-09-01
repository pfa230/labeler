## Context

See `proposal.md` for motivation and `specs/shape-paint/spec.md` for the contract.

Four facts about the code and the compiler shape the approach.

**The container already paints in two placed layers.** `render_container_item` emits
`#rect(width, height, stroke, radius)` at the container's position and then, separately,
`#place(...)[#box(width, height, clip: true)[children]]` at the same position
(`src/render/mod.rs:2097-2121`). The outline is a rectangle that happens to sit where the container
sits, which is why it does not behave like the container's own border.

**Typst's box already carries paint.** `BoxElem` has `fill`, per-side `stroke`, per-corner `radius`,
`inset`, `outset`, `clip` and `body` (`typst-library-0.15.1/src/layout/container.rs`), and
`fill_and_stroke` prepends the painted shape to the frame, so a box's own fill and stroke render
behind its body. The rectangular case therefore needs one element, not two.

**Typst has no round box, and no round clip.** `layout_shape` branches on `kind.is_round()` into
`Geometry::Curve(Curve::ellipse(size))` and ignores `radius` entirely; only the non-round branch
applies a radius (`typst-layout-0.15.1/src/shapes.rs:576-591`). Clipping is a bool resolved through
`clip_rect(size, radius, stroke, outset)` (`shapes.rs:617`, called from `flow/block.rs:89` and
`inline/box.rs:66`), so the only clip region available is a rounded rectangle. `clip_rect` is passed
the box's own `radius`, so a clipping box with a radius cuts its body at the corner curve.

**A box's clip does not cut the box's own paint, but its stroke does inset that clip.** Two separate
facts, and only the first is reassuring.

`layout_box` clips first and paints second: `frame.clip(...)` groups the body that already exists into
a clipped group (`inline/box.rs:66`, `typst-library-0.15.1/src/layout/frame.rs:356-360`), and
`fill_and_stroke` then `prepend_multiple`s the painted shape onto the frame outside that group
(`shapes.rs:664-680`). Fill and stroke are siblings of the clipped group, not members of it, so a
container's own centred stroke keeps its outer half whatever its clip says.

The body is another matter. `layout_box` passes the box's own stroke into `clip_rect`
(`inline/box.rs:66`), which halves each thickness into `stroke_widths` (`shapes.rs:626`) and then
builds the curve from the corners' **inner** control points (`shapes.rs:638-658`), where
`center_inner` is the corner offset by `stroke_width_before/after` on each axis
(`shapes.rs:1153-1160`, `1241-1246`). The clip region is therefore the box inset by half the stroke on
every side. Today's child box carries no stroke and no radius, so it clips at the full outer box
(`src/render/mod.rs:2144`); putting the stroke on it changes that.

The `shape-paint` capability this delta modifies is already in `openspec/specs/`, landed by #280 and
since edited by #291, which moved colour into `colour-vocabulary`. The `MODIFIED` blocks are copied
from that landed text.

## Goals / Non-Goals

**Goals:**

- A geometry field on `container` whose default reproduces today's rendering exactly.
- One emitted element for the rectangular case, replacing the overlay pair, painting and clipping on
  one boundary.
- A squareness guarantee for `circle` that holds for every request, wherever the extents resolve.

**Non-Goals:**

- Clipping children to a **round** geometry. Typst cannot, and emulating it (an SVG mask, or
  rendering children to an image and masking) would put new machinery on the hot path with its own
  resolution and bounds failure modes, to enforce something no label design in the catalog needs. The
  rounded rectangle is not in this bucket: Typst clips to one natively, and this change adopts it.
- Changing size resolution. A geometry never affects an extent; `layout-sizing` is untouched.
- Any geometry beyond the three. Polygons and paths need a vertex-list coordinate model the resolver
  does not have, and are out of scope per the issue.

## Decisions

**The geometry goes on `container`, not on a new leaf item.** A rectangle or ellipse on a label
exists to hold text, so the painted thing must be the thing that holds children. The alternative, a
`shape` leaf item beside `container`, is the SVG and Figma arrangement and is wrong for this engine
for two reasons: a badge would become two items coordinated by hand with nothing checking they still
agree, and a painted rectangle would then have two spellings, since a shape with no children is
already expressible as a container with `items: []` (as
`tests/fixtures/templates/avery5163_asset_tag.yaml:43-51` does today). Content-layout systems put
paint on the layout node (CSS `background`/`border-radius`, Flutter's `BoxDecoration`, Compose's
`Modifier.background(color, shape)`), and Typst agrees: `rect`, `square`, `ellipse` and `circle` all
take a body (`typst-library-0.15.1/src/visualize/shape.rs:127,204,254,329`).

**`circle` emits a Typst `#ellipse`, never `#circle`.** Typst's quadratic kinds force their size to
`min(width, height)` when both are given (`shapes.rs:600-612`), which would make a circle the only
item whose paint is smaller than its resolved box and would require new rules for where it sits and
what its children re-base to. Emitting `#ellipse` on a box the service has already proven square
produces the same drawing with none of that.

**`circle` is refused rather than coerced on a non-square box.** The alternative, drawing an oval
under a key that says circle, is a silent fallback. Refusing is what makes `circle` worth a spelling
distinct from `ellipse`: the check is the feature.

**Render checks every circle; load additionally refuses the subset it can decide.** The two are not
alternatives and load is not a filter in front of render. `resolve`, `available` and `requirement`
are shared by load-time validation and render-time resolution and cannot tell which stage they are in
(ADR-0080, ADR-0081), so both stages ask the identical question and the two cannot drift the way they
did in #150 and #155 — but they are not handed the same inputs, and the split has to be keyed to that
rather than to the shared code.

Load may refuse only where the number an extent resolves to is written in the template: a literal, or
a `to` whose corners are both non-negative or both sign-negative. `ExtentSource` alone does not say
that, because `source_of` maps a `"{param}"` reference to `ExtentSource::Author(v)` by looking the
name up in the `geometry_values` map it is handed (`src/resolver.rs:105-111`). That map carries
declared defaults at load and the request's values at render, and a request's `data` outranks a
declared `default:` (`param-resolution`), so `size: ["{w}", 12]` with `default: w = 12` is
author-source and square at load while a request of `w: 14` still has to be refused. Worse, a
parameter with no numeric default falls back at load to its `min`, its `max`, or **0.0**
(`templates.rs:1607-1621`), so load can be handed a value the author never wrote at all.

**The fix is a second field on `AxisSpec`, not a second reading of the spelling in `templates.rs`.**
`source_of` is the only place a spelling is classified and nothing downstream may branch on one
(ADR-0080, ADR-0081), so a load check asking "was this a literal, a constant `to`, a parameter
reference or a shrinking `to`?" would duplicate the extent grammar in `templates.rs` and restore the
drift that rule exists to prevent. `AxisSpec` already carries exactly this kind of derived fact:
`written_as_to` is recorded by the classifier "so no later rule has to look at `Extent` again to know
which spelling produced the extent it is judging" (`resolver.rs:57-63`). `fixed_by_template: bool`
joins it on the same terms, set true only for an `Author` extent from a number or a constant `to`, and
false for a parameter reference, a shrinking `to`, `content` and `fill`. The load check then reads
`spec_0.fixed_by_template && spec_1.fixed_by_template` and nothing else. The name is the property, not
the stage, because the resolver cannot tell which stage it is in and its vocabulary should not pretend
otherwise.

The other direction is the one that would quarantine a correct template. Load cannot measure text,
encode a QR or decode an image, so it passes the available extent in place of an intrinsic size,
which makes a `content` extent resolve exactly as a `fill` one does at that stage (ADR-0080). A
square intrinsic circle inside a non-square frame would look non-square to a load check that trusted
that proxy. Content and frame extents are therefore not judged at load at all; they are not "not yet
known", they are answered wrongly there.

Render then checks every **active** `circle`, including the ones load already passed, because a load
pass proves squareness only for the request-independent subset and re-checking a fixed extent costs a
comparison. "Active" is not a carve-out bolted on: an item under a false `when:` is excluded from
rendering (frozen `docs/SPEC.md` §5), so an inactive `circle` has no resolved box to judge, exactly as
it has no required parameters. The check sits at the top of `render_container_item` on the emission
walk where the final `place` frame is known, and `render_items` filters on `is_item_active` before
dispatching, so a gated-off container never reaches it. Checking only at load
would let a dynamic template ship a `circle` that draws an oval; checking only at render would move a
static author's mistake from startup to print time.

The two refusals are two different errors and the plan keeps them apart. Load's is ordinary structural
validation: the template is quarantined and reported as `TemplateInvalid`, with no new reason slug.
Render's is `circle_box_not_square`, which is therefore render-time only, and it reaches the caller
through whichever envelope the path already has: top level on a single render, and an entry of
`details.failures` under `422 BatchInvalid` in a batch, which stays all-or-nothing and produces
nothing (frozen `docs/SPEC.md` §2.1). `src/reason.rs` gains one variant; no new envelope is invented.

**The rectangular case collapses into the child box; the round case keeps two layers.** For
`shape: rect` the fill, stroke and radius move onto the `#box` that already holds the children and
already carries `clip: true`, so one element carries the container and one boundary both paints and
clips it. For `ellipse` and `circle` there is no round box, so an
`#ellipse(width, height, fill, stroke)` is placed first and the child `#box` second, which preserves
the background-then-stroke-then-children order the spec requires because the ellipse is emitted
before the box.

**The clip is the boundary of the element that holds the children, and that is contractual.** One
rule, stated once, from which both of this change's clipping differences follow. At `rect` the
child-holding element is the painted one, so the clip follows its corner radius and its stroke's inner
edge; at a round geometry it is not, because no round box exists, so the clip is the plain rectangle.
Stating it this way is what keeps the round geometries' rectangular clip a derived consequence rather
than an exception a reader has to accept on faith.

It reverses two of #280's lines, both by the same move and both verified invisible in this repository:

- **The corner radius now clips.** `clip_rect` is handed the box's radius (`shapes.rs:617`). No
  template sets `rounded`, which became a number only with #280.
- **A stroke now cuts child ink at its inner edge**, by half its thickness (`shapes.rs:626`,
  `638-658`, `1153-1160`). Where both are present the clip curve is inside the painted one and not
  identical to it: the sides move in by `t/2`, while the inner arc's radius is the authored radius less
  the **full** thickness, floored at zero (`radius_inner`, `shapes.rs:1174-1176`), so the corner is
  tighter than a plain `t/2` offset of the painted curve would be. The contract states the
  distinction and leaves that arithmetic to the emitter rather than pinning a formula it does not
  own. Every stroked
  `container` in the repository declares `items: []`
  (`tests/fixtures/templates/avery5163_asset_tag.yaml:43-51`, the only one); every other `stroke:` is
  on a `line`, which has no children. So nothing rendered today changes.

Both match CSS, which is the model the single-box decision was taken against: `border-radius` rounds
the clip, and `overflow: hidden` clips at the **padding** box, inside the border. A centred stroke's
inner edge is that padding edge. Layout is untouched in both cases: children keep their coordinates,
their extents and their padded inner frame, and only ink crossing the boundary is cut, which the
contract now says outright next to #280's "never insets its children".

**Rejected: collapse only when there is no stroke.** It would preserve today's outer-box clipping
exactly, and it is an exception with nothing to show for it. It makes the emitted structure depend on
a paint key, so the container has two shapes in the code and two clipping rules in the contract; it
makes "the boundary clips" false precisely when there is a visible boundary; and the behaviour it
preserves is not observable in any template here. The uniform rule is stated instead, with what it
changes named above.

**Squareness compares to `BOUNDS_EPSILON`, never to exact equality.** A resolved extent is an `f32`
and a constant `to` extent is a subtraction, so a box an author wrote as square need not compare
square: `at: [0.2, 0]`, `to: [0.3, 0.1]` gives a width of `0.3f32 - 0.2f32` against a literal height
of `0.1f32`, two values that paint identically and differ in the eighth decimal. The bound is
`resolver::BOUNDS_EPSILON`, already `1.0e-4` and already documented as "the tolerance every bounds
comparison uses, so load and render agree on the edge cases" (`resolver.rs:288-289`). Reusing it
rather than declaring a private constant is the whole point: squareness is one more comparison on the
same resolved numbers, and a second tolerance beside it would eventually disagree with it.

It is a tolerance, and the plan does not dress it up as a proof. `format_float` writes lengths as
`{value:.4}` (`src/render/helpers.rs:285`), but the two dimensions are formatted independently, so
being within `1e-4` does not make them format alike: `1.00004` and `1.00006` are `0.00002` apart and
emit as `1.0000` and `1.0001`. The value of the rule is that it is one stated line drawn in the same
place at load and at render, alongside every other bounds comparison; its cost is that a circle may
be emitted one quantum out of square, which no target device resolves. The alternative, exact `f32`
equality, would refuse boxes an author wrote as square and the renderer draws as circles.

**Three-layer plumbing as usual.** `shape` is a new enum: `raw.rs` (with `deny_unknown_fields`
already rejecting a misspelling), `models.rs`, and the `TryFrom` in `convert.rs` that classifies the
value and rejects an unknown one with a `serde_path_to_error` path. `src/openapi.rs` registers the
model.

## Risks / Trade-offs

**The collapse changes clipping for every stroked container, not only rounded ones.** The stroke's own
outer half survives, because a box's paint is prepended outside its clipped group
(`inline/box.rs:66-71`, `shapes.rs:664-680`); its children do not, because `clip_rect` builds the
region from the stroke's inner control points. → Accepted rather than worked around, specified in the
geometry requirement and in #280's paint-coverage requirement, and carrying its own scenario. The
exposure is bounded and was checked, not assumed: no `container` in the repository has both a `stroke`
and children. Verify both clipping changes on a rendered image before calling the change done: a
stroked container with a child reaching the edge, and a rounded one with a child in the corner. That
is the render-and-look loop, and no task should claim it.

**`circle` is the first geometry that can fail at render.** Every other template error either
quarantines at startup or is a data problem. A template with a parameterised or content-driven extent
can now ship, validate, and refuse a specific print. → The spec states both refusal points explicitly
so neither is a surprise, and states the render check as unconditional so no reader concludes a
load-time pass exempts a request.

**The base capability is moving.** `shape-paint` gained requirements from #280 and was then edited by
#291 within a day, and `background` now resolves through `colour-vocabulary`. → The `MODIFIED` blocks
were rebuilt from the landed spec and machine-checked for scenario loss against it; if another change
lands against this capability before this one archives, `archive-merge-check.sh` is the gate that
catches the drift, and the blocks must be rebuilt the same way.

**A round geometry leaves an unpainted corner region that authors may not expect.** A container whose
background was a rectangle becomes an ellipse the moment `shape` is set, and any sibling relying on
that ground changes appearance. → Not a regression, since the default is `rect` and no existing
template can set the key, and the spec carries a scenario making the unpainted corners explicit.

## Migration Plan

None. `shape` is additive and optional, its default is the current behavior, and no existing template
can carry it. Rollback is the removal of the field.

## Open Questions

None.
