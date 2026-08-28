## Context

See `proposal.md` for motivation. What #226 left in place, and what this has to fit into:

- **One resolver, two callers.** `src/resolver.rs` holds `source_of`, `available`, `claim`,
  `requirement`, `resolve`, `precheck`, `place` and `container_geometry`. Load-time validation and
  render-time resolution both call them and cannot tell which stage they are in; only the walk
  supplying intrinsic sizes differs, because load cannot measure text, encode a QR or decode an image
  (ADR-0080). Load substitutes the available extent for an unmeasurable intrinsic
  (`resolve_unmeasured`), which is a true upper bound there precisely because a content extent is
  clamped to availability.
- **Sizes already flow both ways.** A node reports `min(intrinsic, max_*, available)` upward and takes
  the available extent downward. `claim` and `resolve` are separate functions today and already differ
  for `ExtentSource::Frame` (`resolver.rs:165-192`). That asymmetry is not a wrinkle to smooth over;
  it is what makes an item that stretches to a label still be the item that sizes it, and this change
  uses it as it stands.
- **Every universal is keyed to an anchor.** `source_of` reads `placement.at` unconditionally
  (`resolver.rs:75-87`), `available` subtracts the anchor, `requirement` matches on it, and `place`
  derives the box's origin from it. A packed child has no anchor, which is why the sizing contract has
  to be amended rather than merely extended.
- **A container's intrinsic is already defined in terms of its children**, as the largest frame
  requirement plus padding, computed in author space and swapped as a completed pair
  (`render/mod.rs:1323-1366`).
- **Text overflow is settled and is the item's own business** (ADR-0082). A `text` is laid out against
  the box it will get, and its `overflow` policy decides what happens when it cannot fit. A packed
  child's box is the container's padded inner box, which is known before any packing, so nothing here
  disturbs that order.

This change adds **ADR-0083, "A packed child is anchorless, and its container's arrangement places
it"**, which **amends ADR-0080 §1 and ADR-0081 §1**. Both define, in terms of an anchor, a quantity
this change extends to an item that has none: ADR-0080 §1 says "`available(frame, axis_spec)` is the
space an item has from its anchor", and ADR-0081 §1 defines `fill` as "the remaining space within the
parent frame from the item's anchor: `parent_frame - resolved_anchor`". Neither is wrong for an
anchored item and neither is retired; each gains the anchorless case. ADR-0083's Status names both,
and their `docs/adr/README.md` rows gain "(amended by [0083](...))", the annotation ADR-0036 and
ADR-0051 already carry for ADR-0080 (`docs/adr/README.md:49`, `:64`). ADR-0082 is untouched. ADR-0083 is claimed here; #212's committed planning artifacts name the same number for the
arrangement it was going to add, and #212 renumbers when it resumes on top of this change.

## Goals / Non-Goals

**Goals:**

- Amend the sizing contract so an anchorless child is a stated case, not a formula that happens to
  degenerate correctly. Six requirements, not the issue's five: the resolved-axis requirement is keyed
  to the anchor in its second clause, so an anchorless item leaves it ambiguous, and the first plan
  review found it.
- Add one arrangement, and nothing else. Every question about how big a box is stays answered by
  `layout-sizing`.
- Keep the arrangement's inputs quantities the resolver already produces, so there is no second sizing
  rule to keep in step.
- No template that renders today changes, and no response that is served today changes.

**Non-Goals:**

- `wrap`, `line_gap` and the `overflow: fail | trim` policy (#212).
- The leftover-room meaning of `fill` (#260).
- Secondary-axis alignment, distribution, per-child grow factors, reordering.
- Anything about a child's own content. A `text` that cannot fit its box raises under its own policy,
  in a flow container exactly as anywhere else.

## Decisions

### 1. `Anchor` gains an absent case, and it is loud rather than a zero

`available` and `requirement` both produce the right numbers for a packed child if its anchor is read
as `Plain(0.0)`: `available` gives `frame - 0 - 0`, the whole inner extent, and
`requirement(Plain(0), claim)` gives `0 + claim`, the bare claim. That coincidence is exactly why the
case was easy to leave unstated, and it is a trap, because `place` also derives the box origin from
the anchor and a zero there would silently draw every packed child at the container's leading corner.

So `Anchor` gains an `Absent` variant. `available` returns the frame extent for it and `requirement`
returns the claim, so `source_of`, `available`, `resolve`, `claim` and `requirement` all keep working
on a packed child unchanged. `Anchor::resolve` has no answer for it and is unreachable, which means
`place` is unreachable for it too: `place` derives the box origin from the anchor
(`resolver.rs:412-413`) after `precheck` has already resolved it (`:328`), and load reaches `place`
for every item with a placement (`templates.rs:1547`). A packed child cannot go down that path, and
decision 1a says what it goes down instead.

*Alternative considered: read the absent anchor as `Plain(0.0)` throughout.* It needs no new variant
and gives the right numbers for `available` and `requirement`, and it is a silent fallback on the
third: nothing would stop `place` from drawing every packed child at the container's leading corner.

### 1a. Two anchor-free checks replace the anchored placement path, and both stages call both

`place` bundles three things an anchored item needs at once: resolve the extents, resolve the origin
from the anchor, and check the resulting box against the frame. A packed child needs the first and the
third, gets its origin from somewhere else, and needs the third at two different moments. So the
bundle is split rather than duplicated, and the split is in `resolver.rs` where the rule already
lives:

- `fits_frame(low, extent, limit) -> Result<(), Violation>` is the far-edge half of the bounds
  comparison `place` already performs inline (`resolver.rs:414-422`), lifted to a name: it returns
  `AnchorBeyondFrame` when `low > limit` and `ExtentBeyondFrame` when `low + extent > limit`. `place`
  calls it for an anchored item, so there is one implementation of "does this box end inside this
  frame" and not two.
- `resolve_packed(placement, inner, geometry_values, intrinsic) -> Result<(f32, f32), Violation>`
  runs the anchor-free part of `precheck` (a written extent must be positive; a `to` cannot appear at
  all on a packed child, so the inversion rule cannot be reached) and then `resolve` on each axis,
  and calls `fits_frame(0.0, extent, inner)` to reject an extent larger than the padded inner box.
  **Load and render both call it**, load with no intrinsic as it does everywhere else, so the
  load-time refusal the spec requires is this call and not a second copy of a rule.
- The arrangement checks the accumulation **in packing coordinates**, not in drawing coordinates:
  `fits_frame(cursor, extent, inner primary extent)` for each child, where the cursor starts at zero
  and only increases. This is deliberate and it is the whole of the low-edge problem. A cursor is
  non-negative by construction, so the near-edge test `precheck` performs for an anchored item
  (`resolver.rs:328-330`) has nothing to catch and `fits_frame`'s two far-edge tests are complete.
  Converting the cursor to a drawing coordinate is the **last** step and never a checked one: a `row`
  gives `x = cursor`, and a `column` gives `y = inner_h − (cursor + extent)`, which is exactly where a
  negative low edge would come from. Checking after that conversion would report a column overrun as a
  box hanging below the frame's origin, which is `AnchorBeforeFrame` and therefore
  `coord_out_of_frame` (`render/mod.rs:838-841`), while the same overrun in a `row` is
  `ExtentBeyondFrame` and therefore `item_out_of_frame` (`:855-858`). Checking before it, one overrun
  is one violation whichever way the container packs.

`Violation` gains nothing: `fits_frame` produces `AnchorBeyondFrame` when the cursor itself is past
the limit and `ExtentBeyondFrame` when only the far edge is, and `violation_error` already maps both
to `item_out_of_frame` (`render/mod.rs:855-858`). `AnchorBeforeFrame` is unreachable for a packed child, because nothing
resolves an anchor for one and no check runs on a converted drawing coordinate. That is the
implementation half of the spec's rule that a packed child never raises `coord_out_of_frame`.

**Every site that resolves an anchor today must divert**, and there are three, not one. `place` has two
callers, `templates.rs:1547` and `render/mod.rs:1501`, and both reach `Anchor::resolve` at
`resolver.rs:412-413`. The third is independent of `place` and is the one easiest to miss: the
measuring walk calls `precheck` directly for every active item carrying a placement, a container's
children included, at `render/mod.rs:1058-1059`, and `precheck` resolves the anchor at
`resolver.rs:328`. A packed child reaching that call would hit `Anchor::Absent::resolve` on every
render. All three call `resolve_packed` for a packed child instead, which is why `resolve_packed`
carries `precheck`'s anchor-free rules rather than only the extent resolution.

*Alternative considered: give a packed child an origin of zero at load so it can go through `place`.*
The extent check then passes for the wrong reason, and the same zero is one refactor away from being
used as the drawing origin, which is the trap decision 1 exists to close.

*Alternative considered: keep the check in drawing coordinates and teach `fits_frame` a near-edge
test.* It works, and it costs a second `Violation` meaning for the same event plus a rule that the
packed path must map that variant to a different slug from the anchored path. Checking before the
conversion needs neither.

### 2. `Placement.at` becomes `Option<Position>`

`Placement.at` is always serialized today and an omitted raw `at` becomes `Position::default()`
(`convert.rs:18-59`, `models.rs:554-556`), so a packed child accepted precisely because it omitted `at`
would come back from `GET /api/templates/{id}` carrying `at: [0, 0]`, a spelling the same schema
refuses on the way in. Reading a template and resubmitting it has to work.

`at` therefore becomes `Option<Position>`, and the invariant is narrow: `None` is legal only for a
packed child. Conversion keeps normalising an omitted `at` on an absolutely arranged item to
`Some([0, 0])` exactly as it does today, so every item authorable before this change still serializes
its anchor and no existing response or OpenAPI requiredness changes. Only a packed child, which could
not be authored before, omits it. This touches every `Placement` construction site, the same cost
ADR-0051 §3 paid to make "exactly one of `size` or `to`" a type invariant, and for the same reason:
the alternative is a runtime rule every new call site can forget.

*Alternative considered: accept `at: [0, 0]` on input and return it.* That makes the validation rule
read a coordinate's value rather than its presence, and ships a spelling that is accepted and then
ignored.

### 3. `flow` is a block on `container`, not a new item type

One nested key is the discriminator: `container.flow.is_some()` answers "are these children packed?"
in one place, so refusing `at` on a packed child is one check in `convert.rs` rather than a rule spread
across optional keys. A chip is an ordinary container with `frame` and `padding`, so nesting needs no
new type, and the web UI's field walker recurses on `type === "container"` and reads `items`
(`ui/src/lib/templateFields.ts:207-209` and `:291-292`), so a flow container needs no UI change.

*Alternative considered: a new `flow` item type.* It would duplicate the container plumbing across
`raw.rs`, `models.rs`, `convert.rs`, `templates.rs`, `render/mod.rs` and that walker, for a box that is
a container in every other respect.

### 4. The arrangement takes requirements and boxes, and computes nothing else

The packer is a pure function in `resolver.rs` beside `claim`, `available` and `requirement`: it takes
the padded inner box, the `flow` settings and, per child in template order, the child's resolved box
extents and its requirements. It returns a rectangle per child plus the assembled extent. It reads no
request state, calls nothing that measures, and never adjusts an extent.

It consumes both quantities because `layout-sizing` produces both and they are not interchangeable:
the assembled extent aggregates **requirements**, which is what the absolute arrangement's
largest-requirement rule already aggregates, and the positions advance by **boxes**, which is what is
drawn. For every source but `Frame` these are one number. Naming both is the answer to the finding
that killed #212's round 5, which asked for exactly this: say which quantity feeds intrinsic
aggregation and which feeds positioning.

It belongs in `resolver.rs` because `AGENTS.md` records what happened when sizing logic lived in two
places (#150, #155). One implementation is the point; two callers are not.

### 5. Render calls the packer twice, for two different reasons, and neither restages a measurement

`measure_items` already sizes a container's children against its **unmeasured** inner box, then
aggregates their requirements into the container's intrinsic (`render/mod.rs:1069-1091`,
`1323-1366`). For a flow container the aggregation changes and nothing else does: the same requirements
go into the packer's assembly instead of into a per-axis maximum. `render_items` then resolves the
container's real box and packs the children into its real padded inner box to obtain their positions.

No measurement is invalidated by that, because no child's box depends on where it lands. The box a
child was measured against is its container's padded inner box, which is the box it keeps. This is the
property that makes the change small, and it is the one #260 will have to give up.

*Alternative considered: run the arrangement once, during measurement, and reuse its rectangles.* The
measurement pass runs against a provisional inner box, so its rectangles are provisional too; keeping
them would mean drawing at coordinates derived from a frame that is not the final one.

### 6. Load does not run the arrangement, and says what it checks instead

At load a content source stands in at its available extent, which inside a flow container is the whole
padded inner box, so at load every content child reports the whole box and their accumulation says
nothing. Running the packer on those substitutes would quarantine templates that render correctly.

Load therefore checks what the template alone decides: this capability's structural refusals, and each
packed child against the padded inner box by the ordinary rules, which is the same check that child
gets in an absolutely arranged container and a true necessary condition. That is the load/render
division `layout-sizing` already draws; the amended requirement now says out loud that the division
leaves accumulated overflow to render, instead of implying a guarantee it cannot keep.

*Alternative considered: pack at load on substituted extents and ignore the verdict.* An arrangement
whose result is discarded is not a check, and the artifacts would then claim a guarantee nothing
enforces.

### 7. Overflow is the existing bounds rule, applied without an anchor, and it is one reason

The arrangement gives a child a position and `fits_frame` judges it (decision 1a), so this change adds
no reason, no policy and no field. What it must **not** do is route that position through the anchored
path, because the two edges are not symmetric: a `row` aligns to the padded inner box's top edge and a
`column` packs downward from it, so a child too tall for the box and a column that overruns both put a
box edge below the frame's origin. Anchored, that is `AnchorBeforeFrame` and therefore
`coord_out_of_frame` (`resolver.rs:328-330`, `render/mod.rs:838-841`), while a `row` overrun on the
far edge is `ExtentBeyondFrame` and therefore `item_out_of_frame` (`:855-858`). One event would be
reported under two slugs according to which way the container packs. Anchor-free, it is one call to
`fits_frame` and one slug.

#212 adds `wrap` and `overflow` on top, and both are opt-ins away from this behavior rather than
replacements for it.

### 7a. The `[fill, fill]` container default is left alone, and its consequence is stated

A `container` with neither `size` nor `to` resolves as `size: [fill, fill]` (`convert.rs:97-99`), and
that reaches a packed container too, so two chips authored without a `size` each take the whole padded
inner extent and the second fails `item_out_of_frame`. Defaulting a packed container to
`[content, content]` instead would make the archetype work first time. It is refused: the same
spelling would then resolve differently according to which container the item sits in, so a reader
could no longer read a container without reading its parent, and the rule would be a carve-out whose
only proof is convenience. The failure is loud, arrives at render, names the second container, and is
fixed by one line, so nothing is silently wrong in the meantime. The spec states the case and gives it
a scenario.

*Alternative considered: refuse a packed container that omits `size`.* It invents no default and is
also loud, and it is louder than the problem: `fill` on a packed child is a legal, defined spelling by
the requirement above, so refusing its default form would contradict the rule one paragraph earlier.

### 8. A zero-primary-extent child is drawn and occupies nothing

`gap` is the space between two adjacent children, and a child with no extent on the packing axis is
adjacent to nothing along it, so no gap falls on either side. Suppressing its **drawing** as well would
be a second, separate rule, and would contradict `layout-sizing`'s standing promise that a zero content
or frame extent renders an empty box, whose only visible consequence is a container `frame` stroke. So
it is drawn at the cursor, advances nothing, and contributes its secondary extent like any other drawn
child. Occupancy is judged on the **box**, not the requirement, because occupancy is about what is
drawn.

*Alternative considered: treat it exactly like a gated-off child.* One sentence shorter, and it needs a
proof that a zero-extent box may be skipped, which nothing supplies.

## Risks / Trade-offs

- **`Placement.at` becoming optional touches every construction site** → It is mechanical and the
  compiler finds all of them. `Placement::sized` keeps the common case to one call, and conversion
  still normalises absolutely arranged items, so the blast radius is construction, not behavior.
- **The packer drifts from the resolver's idea of an extent** → It cannot: it is given resolved boxes
  and requirements and has access to nothing that measures.
- **A `fill` packed child beside a sibling fails the render** → Stated in the spec with its reason, and
  it fails loudly rather than mislaying content. The meaning authors usually want is #260, and this
  change deliberately ships no meaning for the leftover room so that #260 is free to define one.
- **#260 will change what an already-shipped `fill` does** → Named as a breaking change when #260 is
  planned. Deferring `fill` entirely was considered and rejected: refusing a spelling the uniform rule
  resolves would be an exception with no proof behind it.
- **Rust metrics versus Typst shaping** → A packed child sits inside a `#box(clip: true)`, so a
  measurement a hair narrow clips a glyph edge rather than overflowing. The metrics are the ones sizing
  already trusts (ADR-0049).

## Migration Plan

Additive. A container without a `flow` block parses, validates, sizes and renders exactly as before, so
there is no data migration and no compatibility window. Rollback is reverting the commit; a template
written against `flow` would then fail to load on an unknown key and be quarantined per-template rather
than aborting startup (ADR-0058).

## Acceptance evidence

Templates are visual artifacts, so acceptance is rendered labels opened and inspected, not a green
suite. Each renders to PNG, is opened, checked against intent, fixed and re-rendered:

1. A `when:`-gated column that closes its hole, rendered with the gate on and off.
2. A row whose children are `content`-sized text, rendered with a short and a long value.
3. A row whose middle child's value is empty, confirming one gap rather than two.
4. Both nesting directions: a flow container inside an absolute one, and an absolute container inside a
   flow one, and a flow container inside a flow one.
5. A dynamic-width label sized by a flow container, beside a non-flow `content`-width text item.
6. A rotated flow container, confirming it packs in author space.
6a. A `column` whose children overrun the padded inner box, confirming it fails with
   `item_out_of_frame` and not `coord_out_of_frame`, which no `row` case reaches.
7. A `fill` packed child alone in its container, and the same child beside a sibling, to show both
   halves of what `fill` means here.
8. A flow container as the root of a sheet slot, to show the arrangement is format-independent.
9. A `content`-sized multiline text with a `font_size` range as a packed child, to confirm it is laid
   out against the container's padded inner box and packed at the result.

## Open Questions

None. The five questions the issue left open are decided: a packed child is anchorless in the model as well as in
the schema (decisions 1 and 2), it round-trips without an anchor, `fill` keeps its uniform meaning on
both axes with the consequence stated, a zero-primary-extent child is drawn and occupies nothing
(decision 8), and a parameter-authored extent that outgrows the inner box is reported by the child's
own placement check before any accumulation is considered.
