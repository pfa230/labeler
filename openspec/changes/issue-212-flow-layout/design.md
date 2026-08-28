## Context

See `proposal.md` — Why. What #226 left in place, and what this has to fit into:

- **One resolver, two callers.** `src/resolver.rs` holds `source_of`, `available`, `claim`,
  `requirement`, `resolve`, `place` and `container_geometry`. Load-time validation and render-time
  resolution both call them and cannot tell which stage they are in; only the walk supplying intrinsic
  sizes differs, because load cannot measure text, encode a QR or decode an image (ADR-0080). Load
  substitutes the available extent for an unmeasurable intrinsic, which is a true upper bound there
  precisely because a content extent is clamped to availability.
- **Sizes already flow both ways.** A node reports `min(intrinsic, max_*, available)` upward and takes
  the available extent downward. That asymmetry is what lets an item that stretches to a label still
  be the item that sizes it, and it is exactly what a packed child needs.
- **A container's intrinsic is already defined in terms of its children**, as their frame requirements
  plus padding, with the aggregation stated once, in the rotation clause of the container requirement.
- **Rotation composes through sizing** since #226 retired ADR-0036 §5: a rotated container computes
  its intrinsic in author space and swaps the completed aggregate.
- **Text overflow is settled and is the item's own business** (ADR-0082): a `text` is laid out against
  the box it will get, and `overflow: ellipsis | fail` decides what happens when it cannot fit.

This change adds **ADR-0083, "A container's arrangement decides where its children go"**. It amends no
ADR: ADR-0080 and ADR-0081 supply the sizing this builds on, and ADR-0082 supplies the vocabulary the
`overflow` field reuses.

## Goals / Non-Goals

**Goals:**

- Add an arrangement, and nothing else. Every question about how big a box is stays answered by
  `layout-sizing`.
- The arrangement decides position only. Every extent it consumes is one `layout-sizing` already
  produced, so there is no second sizing rule to keep in step and no exception to carve.
- The arrangement has one implementation and one caller, the render walk. Load has no measured extents
  to feed it, so it checks what the template alone decides.
- No template that renders today changes.

**Non-Goals:**

- Cross-axis alignment, distribution (`space-between` and friends), per-child grow factors, and
  reordering. Each is a separate arrangement parameter and none is needed by #212's cases.
- Marking or reporting a trim.
- Anything about a child's own content. A `text` that cannot fit its box raises under its own policy,
  in a flow container exactly as anywhere else.

## Decisions

### 1. `flow` is a block on `container`, not a new item type

One nested key is the discriminator: `container.flow.is_some()` answers "are these children packed?"
in one place, so refusing `at` on a packed child is one check in `convert.rs` rather than a rule
spread across four optional keys. A chip is an ordinary container with `frame` and `padding`, so
nesting needs no new type, and the web UI's field walker recurses on `type === "container"` and reads
`items` (`ui/src/lib/templateFields.ts:276`), so a flow container needs no UI change.

`direction` is required rather than defaulted, so `flow: {}` cannot select an arrangement by accident.

*Alternative considered: a new `flow` item type.* It would duplicate the container plumbing across
`raw.rs`, `models.rs`, `convert.rs`, `templates.rs`, `render/mod.rs` and that walker for a box that is
a container in every other respect.

### 2. The arrangement lives in `resolver.rs`, and only the render walk calls it

The packer takes an ordered list of resolved extents, the padded inner box and the `flow` settings, and
returns a rectangle per placed child plus the assembled extent. It reads no request state and calls
nothing that measures, so it is a pure function of values its caller already holds.

It belongs beside `claim`, `available` and `requirement` because it is the same kind of thing and
because `AGENTS.md` records what happened when sizing logic lived in two places (#150, #155). One
implementation is the point; two callers are not.

**Load does not run it.** `layout-sizing` has load take a content source to yield its available extent,
since load cannot measure text, encode a QR or decode an image. Inside a flow container that available
extent is the whole padded inner box, so at load every `content` child reports the whole box and their
accumulation says nothing about how many lines they occupy or which of them sit together. Running the
arrangement on those substitutes could quarantine a template that renders correctly.

So load checks what the template alone decides: this capability's structural rules, and each packed
child against the padded inner box as if it were the only child, which is a true necessary condition
and refuses an oversized authored extent exactly where it is written. Line selection, trimming and
overflow are render-time. That is the load/render division `layout-sizing` already draws, not a new
one.

*Alternative considered: run the arrangement at load on substituted extents and ignore its verdict.*
An arrangement whose result is discarded is not a check, and the artifacts would then be claiming a
guarantee that nothing enforces.

### 3. A packed child's box comes from the child, and the arrangement never supplies one

Three plan-review rounds all failed at the same seam, and it was always one feature: a child whose box
comes from the arrangement. `layout-sizing` is built on a box being known from its frame, and it says
so in the requirement that matters most here: "The box a text is laid out against is known before its
content is: it is the item's own extent when that extent is authored, and the available extent, capped,
when it is content or frame", followed by the rule that breaks and font size are not re-decided when
the box turns out to be **larger**. Larger only. Any design in which the arrangement hands a child a
box would violate that, because the room left on a line is smaller than the container it was measured
against.

So a packed child's available extent is the container's padded inner extent, not the room left on its
line. `layout-sizing`'s formula degenerates to exactly that when there is no anchor and no inset, so
nothing about sizing changes and no exception is carved anywhere: a packed child is sized precisely as
the same item at `at: [0, 0]` in an absolutely arranged container of the same inner box.

`fill` is where an earlier draft of this decision went wrong, and the plan review caught it. I refused
it on a packed child and called the refusal necessary, but under the rule above it is not: a packed
child's available extent is the whole inner box, so `fill` has a well-defined meaning with no
arrangement involved. `fill` is therefore permitted and means exactly that. Its consequence is stated
in the spec rather than hidden: such a child occupies a whole line, so a sibling on that line overflows
and fails under the default policy.

What is genuinely circular is the *other* meaning of `fill`, the room the arrangement has left, and
that is [#260](https://github.com/pfa230/labeler/issues/260). A `font_size` range picks its size
against the box and a `multiline` text's height comes from breaking against its width, so a child
handed its box by the arrangement cannot first report the extent the arrangement needs to compute that
box; for `direction: column` it is worse, since the main axis is height and height comes from breaking
against the width, which is the cross axis. That is a contract to design, not a line to add here.

Everything the earlier rounds fought over then evaporates. There is no unclamped "desired extent"
beside `layout-sizing`'s clamped one, so no second sizing protocol and no exception to state. There is
no staging problem, because no child's box depends on where it lands. Wrapping still works, because a
child clamped to the *whole* inner box can still be too wide for what remains on the current line. And
overflow becomes purely about accumulation, since no single child can exceed the box it was clamped to.

*Alternative considered: make `fill` mean the leftover room here.* It means modifying five governing
requirements, a direction-dependent staging rule, and a text-layout contract permitting a box to shrink
after layout. That is a second sizing change wearing this issue's name, and it is #260.

### 4. `wrap` requires a resolved main axis, and that is the whole restriction

Wrapping is the only part of the arrangement that reads the container's own extent while deciding
where children go. `layout-sizing` already has the exact predicate for "is this extent known before
the children are sized": the resolved-axis state, which is precise rather than syntactic and which
accepts a `fill` container under a sign-negative anchor because the frame terms cancel.

So the restriction is one line of reuse rather than a new rule, and it is narrow: an unwrapped flow
container reads nothing about its frame while packing, so it stays legal on an unresolved axis, which
is what lets a flow container size a dynamic-width label.

The axis tested is the one `direction` names, which is the vertical one for `direction: column`, and
it is read in the container's own author space, so a `rotate` of 90 or 270 swaps which physical axis
that is. `container_inner_axes_resolved` (`resolver.rs:225-252`) already performs that swap, so the
check reads the state it produces rather than re-deriving it.

*Alternative considered: wrapping against the available extent at `width.max` on an unresolved axis.*
It would choose lines against a wider label than the one drawn, so measurement and drawing would
disagree about which children are on which line. That is the class of bug the pre-#226 engine shipped.

### 5. `overflow` reuses ADR-0082's vocabulary, inverts its default, and grants no exemptions

`fail` and the `item_out_of_frame` reason already exist; a child that leaves its frame is the same
event whether the arrangement or the author put it there, so no new reason is added.

The default differs from `text`'s deliberately. `ellipsis` is a safe default for text because the
reader can see the cut. A dropped child leaves nothing behind, so the safe default is the loud one, and
`trim` is the opt-in an author takes when a missing item is better than a failed print.

**A trim removes a child from the drawing, not from evaluation.** An earlier draft promised that a
trimmed child's own errors would not surface. That is unimplementable and would have been an exception
to two contracts at once: ADR-0082 runs its four-step pipeline for every active `text`, and a
content-sized container cannot supply an extent without measuring the subtree beneath it
(`render/mod.rs:1068-1104`), so the errors arrive before any trim decision exists. Rather than carve
out an exception, the rule is uniform: every child is evaluated by its own contract, `overflow: trim`
answers only "what happens when the container cannot hold everything", and a child that must not fail
its own layout says so with its own `overflow: ellipsis`.

### 5a. A packed child carries no anchor in the domain model either

`Placement.at` is always serialized today and an omitted raw `at` becomes `Position::default()`
(`convert.rs:18-59`, `models.rs:544-565`), so a packed child accepted precisely because it omitted `at`
would be returned by `GET /api/templates/{id}` carrying `at: [0, 0]`, a spelling the same schema
refuses on the way in. Reading a template and submitting it unchanged has to work.

`Placement.at` therefore becomes `Option<Position>`, and the invariant is narrow: `None` is legal only
for a packed child. Conversion keeps normalising an omitted `at` on an absolutely arranged item to
`Some([0, 0])` exactly as it does today (`convert.rs:53-59`), so every existing item still serializes
its anchor and no existing `GET` response or OpenAPI requiredness changes. Only a packed child, which
could not be authored before this change, omits it. It
touches every `Placement` construction site, which is the same cost ADR-0051 §3 paid to make "exactly
one of `size` or `to`" a type invariant, and for the same reason: the alternative is a runtime rule
every new call site can forget.

*Alternative considered: accept `at: [0, 0]` in the response and refuse only a non-default `at` on the
way in.* That is a silent carve-out in the validation rule, and it makes the round trip lossy in a way
no error reports.

### 5b. No staging, because nothing is re-laid-out

Refusing `fill` removes the staging problem entirely. Today a child's box is derived from its
containing frame and its text is laid out against that box before any sibling arrangement exists
(`render/mod.rs:1030-1104`), and the fit is reused when drawing (`:1512-1519`). That order is now
correct for a packed child too, because the box it was laid out against, the container's padded inner
box, is the box it keeps. The arrangement changes where the rectangle is placed and never how large it
is, so no measurement is invalidated and none is repeated.

### 6. Rendering is a placement substitution

`render/mod.rs` already places each child by resolving its own placement. For a packed child it uses
the rectangle the arrangement returned instead. The rectangle is passed down to the existing per-item
render path rather than cloned into a rewritten `Placement`, because a packed child may carry a whole
subtree and cloning it per render would be both wasteful and a second copy of the truth.

## Risks / Trade-offs

- **The arrangement drifts from the resolver's idea of a claim** → It cannot, if it takes claims as
  inputs rather than computing them. The design gives it no access to anything that measures.
- **Load and render pack differently because load substitutes intrinsics** → They do, which is why
  load does not pack at all (decision 2). The risk that remains is the opposite one: a template that
  loads and then fails every render because its content never fits. That is already true of every
  content-sized item since #226, and `layout-sizing` states it plainly: a refusal at load depends only
  on the template, and passing load is not a claim that a request will render.
- **Rust metrics versus Typst shaping** → A child that hugs its text sits inside a
  `#box(clip: true)`, so a measurement a hair narrow clips a glyph edge rather than overflowing. The
  metrics are the ones sizing already trusts (ADR-0049), and ADR-0050 is the precedent for reserving
  ink room should the render-and-look pass show an edge being shaved.
- **`overflow: trim` is silent** → Accepted and stated in the spec; `fail` is the default, and marking
  and reporting a trim are out of scope with the reason recorded.
- **`fill` on a packed child does something authors may not expect: it takes a whole line** → It is
  the uniform reading of the rule, it is stated in the spec with its consequence, and it fails loudly
  rather than silently mislaying content. The meaning authors usually want is #260.

## Acceptance evidence

Templates are visual artifacts, so acceptance is rendered labels opened and inspected, not a green
suite. Each renders to PNG, is opened, checked against intent, fixed and re-rendered:

1. A `when:`-gated column that closes its hole, rendered with the gate on and off.
2. A row whose children are `content`-sized text, rendered with a short and a long value.
3. A wrapped row filling two lines, with a visible `gap` and a different visible `line_gap`.
4. Both nesting directions: a flow container inside an absolute one, and an absolute container inside
   a flow one.
5. A dynamic-width label sized by a flow container, beside a non-flow `content`-width text item.
6. A rotated flow container, confirming it packs in author space.
7. `overflow: trim` dropping a child, and `overflow: fail` refusing the same layout.
8. A `fill` packed child alone in its container, and the same child beside a sibling, to show the
    whole-line consequence under each `overflow` value.
9. Two children whose accumulated line exceeds the inner box, under each `overflow` value.
10. A flow container as the root of a sheet slot, to show the arrangement is format-independent.
11. A `content`-sized multiline text with a `font_size` range as a packed child, to confirm it is laid
    out against the container's inner box and placed at the result.

## Migration Plan

Additive. A container without a `flow` block parses, validates, sizes and renders exactly as before,
so there is no data migration and no compatibility window. Rollback is reverting the commit; a
template written against `flow` would then fail to load on an unknown key and be quarantined
per-template rather than aborting startup (ADR-0058).

## Open Questions

None. The forks #212 left open were decided with the issue owner: the arrangement is computed in Rust,
`flow` is a block on `container`, the two gaps are `gap` and `line_gap`, container-level wrapping is in
scope, child-level re-breaking is not, and `overflow` is configurable. What #212 still lists beyond
that is out of this change's scope rather than undecided within it.
