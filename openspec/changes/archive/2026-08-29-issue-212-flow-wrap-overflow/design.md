## Context

See `proposal.md` — Why. What [#263](https://github.com/pfa230/labeler/issues/263) left in place, and
what this has to fit into:

- **The arrangement exists and is one function.** `resolver.rs::arrange_flow` takes the padded inner
  box, the `Flow` settings and a `FlowChildInput { resolved_box, requirement }` per child, and returns
  a rectangle per child plus the assembled extent. It measures nothing and reads no request state.
- **Two quantities per child, and `flow-layout` already says which goes where.** A child's
  **requirement** is what it reports upward and is what the assembled extent is built from; its **box**
  is what it takes downward and is what the packing positions and draws. They are one number for an
  author or content source and differ for a frame source, which is what `fill` means.
- **A flow container is sized before its own extent is known**, so children are sized against a
  *provisional* padded inner box during assembly and against the resolved one afterwards.
- **Overrun is already an error** with `item_out_of_frame`, split into two checks: a child's own
  extents (load and render) and its arranged box (render only). Check 1 runs first so a child too big
  for the box is reported as itself.
- **The resolved-axis state exists** and `container_inner_axes_resolved` already swaps it for a
  quarter turn.

This change adds **ADR-0089, "Wrapping and the overflow policy"**, which **amends ADR-0083** in three
places: its `flow` block declaration, fixed there to `direction` and `gap`; its assembled extent,
defined there for a single line; and its statement that an overrun consistently raises
`item_out_of_frame`, which `overflow: trim` makes conditional. ADRs are append-only here, so ADR-0089
names those three and the index row for ADR-0083 is updated to point at it, rather than leaving two
accepted records that disagree. ADR-0080 and ADR-0081 supply the sizing and ADR-0082 supplies the
`overflow` vocabulary this reuses; neither is amended.

## Goals / Non-Goals

**Goals:**

- Add lines and a drop policy, and nothing else. No child's box changes, so `layout-sizing` is not
  touched.
- Every new restriction is derived from a failure it prevents, stated beside it.
- Templates that render today render identically, since all three keys default to current behaviour.

**Non-Goals:**

- Marking or reporting a trim. Secondary-axis alignment. Distribution of leftover room along a line.
- Giving `fill` on a packed child the leftover-room meaning, which is
  [#260](https://github.com/pfa230/labeler/issues/260).

## Decisions

### 1. Line breaking reads the box, the assembled extent reads the requirement

#263 drew that line and this change keeps it rather than blurring it. The box is what must physically
fit between the cursor and the line's end, so it decides the break. The requirement is what the child
reports upward, so it builds the assembled extent. For every author and content source they are the
same number and the distinction is invisible; it matters only for `fill`, whose box is the whole inner
extent while its requirement is its bounded intrinsic.

The same split applies to a **line**, which the first draft missed and the review caught: a line's box
extent decides where the next line begins, and so the secondary position of every child on it, while
its requirement extent feeds the assembled extent. Check 2 still tests a child's own box at the
position those stacked line box extents produced; a line extent is never itself the thing measured.
The two may differ when a line holds a frame source and only then, since a `fill` child whose bounded
intrinsic equals the extent it was given reports and takes the same number. Conflating them either
overruns a box that fits or reports a size that was never drawn.

The visible consequence is stated in the spec rather than left to be discovered: under `wrap: true` an
**uncapped** `fill` child takes a line of its own whenever anything precedes it. A capped one is
`min(inner, cap)` and shares a line like any other child, which the merged requirement already says.
That is the ordinary rule applied to a box the width of the line, not a special case, and it is the
behaviour #260 exists to change.

### 2. `wrap: true` requires a resolved primary axis

Wrapping is the first thing in the arrangement that *decides* something from the container's own
extent rather than merely laying out against it. On a `content` primary axis that extent is the
assembly of the children being packed, so the decision would be taken against the provisional inner
box and then contradicted by the resolved one. Refusing it at load is one predicate `layout-sizing`
already computes.

The restriction is narrow: an unwrapped flow container still reads nothing about its frame while
packing, so it stays legal on an unresolved axis, which is what lets a flow container size a
dynamic-width label.

*Alternative considered: wrap against the provisional box and accept that the resolved box may
differ.* That is the cycle five plan-review rounds on the original #212 kept finding, and it is a
fixed-point problem rather than an ordering one.

### 3. `overflow: trim` requires both axes resolved

The first draft of this decision required only the axis the overrun accumulates along, and the plan
review broke it with a case worth keeping in the record. A `row` container spelling
`size: [20, content]` with `overflow: trim`, holding an 8 by 4 child and an authored 15 by 10 child:
the second overruns the width and is trimmed, its height leaves the assembly, the container resolves
to 4 tall, and the child that was just dropped no longer fits the box check 1 measures it against.

The feedback runs through the *other* axis, because a trim removes a child from the assembly on both.
So both axes must be ones whose assembled extent determines nothing, which is what "resolved" means.
That is also one rule instead of a case analysis over `wrap`, and it loses nothing an author wants: on
an unresolved axis the container grows to fit, so there was never anything to trim.

With it, "a trimmed child contributes nothing to the assembled extent" can be stated flatly, with no
ordering caveat.

### 4. `line_gap` without `wrap` is inert, not refused

A container with one line has nothing to separate, exactly as `gap` separates nothing in a container
with one occupying child. There is no invariant behind a refusal, and an earlier round of review on
the original #212 was right to call the refusal an unproven carve-out.

### 5. `trim` removes a child from the drawing and the assembly, and from nothing else

A trim takes a child out of two things, the drawing and the assembled extent, and out of nothing else.
It is not an exemption: every active child is sized and evaluated as `layout-sizing` requires whether
or not it is trimmed, so `trim` grants no relief from any child's own contract, and "the render
succeeds" means the overrun stopped raising, not that nothing else can. What a trimmed child still raises therefore
follows from what sizing demands of it: an authored-size `qr` or `image` is asked for nothing and
raises nothing, while a `content`-sized one is asked for its intrinsic and can still raise. That
asymmetry is `layout-sizing`'s and is stated rather than smoothed, because trimming is the first thing
in the engine that skips drawing.

### 5a. Where each refusal is checked

The local parts of the `flow` block, its enum values, its defaults and the sign of `line_gap`, are
decided from the block alone, so they stay in `convert.rs` where a JSON path exists. The two axis
restrictions are not local: they read the resolved-axis state of the frame the container was given,
which `container_inner_axes_resolved` produces only during the recursive layout traversal in
`templates.rs` (`:1930-1966`). They run there and report by message, because that traversal returns a
bare `String` rather than a path-carrying error. The spec is written to promise exactly that and no
more.

### 6. Check 1 stays ahead of the policy

`overflow` governs check 2, the arranged box. A child whose own resolved extent exceeds the padded
inner box fails check 1 and fails under `trim` as well, which keeps #263's rule that such a child is
reported as itself rather than as whatever the accumulation then does to it.

## Risks / Trade-offs

- **`fill` under `wrap` surprises an author who expected it to share a line** → It is stated in the
  spec with a scenario, it is the consequence of the meaning #263 shipped, and #260 is the issue that
  changes that meaning. Nothing here silently mislays content.
- **Two resolved-axis restrictions are two places to get the axis wrong** → Both read the same state
  through the same helper, and the spec carries a scenario each for `row`, `column` and a quarter
  turn, which is where an axis mix-up shows up first.
- **A wrapped label can look right on the developer's data and trim on the customer's** → That is what
  `fail` being the default is for; `trim` is the opt-in an author takes when a missing item beats a
  failed print.

## Acceptance evidence

Templates are visual artifacts, so acceptance is rendered labels opened and inspected, not a green
suite. Each renders to PNG, is opened, checked against intent, fixed and re-rendered:

1. A wrapped row filling two lines, with a visible `gap` and a visibly different `line_gap`.
2. The same template with `wrap: false`, to confirm one line and today's overrun error.
3. A wrapped row whose lines have different heights, to confirm each line's extent is its tallest
   drawn child.
4. `overflow: trim` dropping a child mid-line, and `fail` refusing the same layout.
5. `overflow: trim` dropping a whole line that does not fit the secondary axis.
6. A `fill` child among wrapped siblings, to confirm it takes a line of its own.
7. A `column` container that wraps into a second column.
8. A rotated wrapping container, to confirm it wraps in author space.
9. A `content`-secondary-axis wrapped container, to confirm it grows to its lines rather than trimming.
10. A capped `fill` child sharing a wrapped line, beside an uncapped one taking a line alone.
11. A wrapped line holding a `fill` child whose box and requirement differ, to confirm the next line is
    positioned from the box while the container assembles from the requirement.

## Migration Plan

Additive. All three keys default to the current behaviour, so a template written before this change
parses, validates, sizes, packs and renders identically. Rollback is reverting the commit; a template
using the new keys would then fail to load on an unknown key and be quarantined per-template rather
than aborting startup (ADR-0058).

## Open Questions

None.
