# 50. Edge-relative coordinates and `to:` opposite-corner placement

Date: 2026-08-12

## Status

Accepted. Issues [#146](https://github.com/pfa230/labeler/issues/146) and
[#147](https://github.com/pfa230/labeler/issues/147). Extends the placement model of
[ADR-0026](0026-auto-length-dynamic-width.md) (dynamic-width `single` templates) and leaves the
rotated-container rules of [ADR-0036](0036-container-rotation.md) otherwise unchanged.

## Context

Every coordinate was absolute from the frame's bottom-left origin, and a box's extent was always
`at` + `size`. Two things a template author routinely wants were both awkward: anchoring an item to
the right or top edge of a label without knowing the label's exact width up front (in particular on
a dynamic-width `single`, where the width is not known until render), and describing a box by its
two corners instead of a corner plus a size, e.g. a right-hand margin, a full-width divider line, or
a box that should always reach the far edge regardless of how the auto-length measurement resolves.

## Decision

**1. Sign-bit sentinel, not `< 0.0`.** A coordinate component is edge-relative when its sign bit is
set (`f32::is_sign_negative`), not when it is numerically less than zero. `-0.0 < 0.0` is `false`, so
a `< 0.0` test cannot recognize `-0.0` as "the far edge exactly", and `-0.0` is the only way to spell
that edge without also picking an arbitrary nonzero inset. YAML's `-0` and `-0.0` both parse to a
sign-negative `f32`, so both spellings arrive correctly as edge-relative.

**2. Negative `y` is the top edge, for symmetry with negative `x` being the right edge.** #146
phrased the ask as "negative `y` means measured from the bottom", read from a top-left screen mental
model. The coordinate system is bottom-left-origin, y-up (§6), so the edge `y` is *far* from is the
top, and treating `x` and `y` asymmetrically (right edge for negative `x`, bottom edge — the origin
edge, already reachable with `0` — for negative `y`) would make negative `y` redundant with existing
non-negative `y`. The resolver is one function either way: `resolve_coord(v, frame_extent) =
frame_extent + v` when sign-negative, `v` otherwise; the axis distinction is entirely in what
`frame_extent` is for each call site.

**3. `Extent` as an enum, not `size: Size` plus `to: Option<Position>`.** `Placement` gets an
`extent: Extent` field (`Extent::Size(Size) | Extent::To(Position)`, serialized flattened, untagged
by field name) rather than two independently-optional fields. This makes "exactly one of `size` or
`to`" a type invariant instead of a runtime check repeated at every construction and validation site.
The cost is real: roughly 43 call sites across `raw.rs`, `convert.rs`, `models.rs`, `templates.rs`,
and `render/mod.rs` construct or match a `Placement`/`Extent`, all touched by this change. Paid once,
at the type level, rather than as an ongoing invariant every new call site could get wrong.

**4. The measure rule: an edge-relative coordinate never contributes a frame-dependent width to the
measured content extent, but an item that sizes itself to its content still contributes that
content.** This departs from #146's literal wording, which (read as "an edge-relative coordinate is
never measured") would have made #147's motivating case — a full-width divider line or a
right-anchored text box on a label sized by its content — impossible: if the item that is supposed
to size the label is itself skipped from measurement, nothing sizes the label. The actual rule
distinguishes *how* an item contributes:

- A leaf item's own edge-relative `at.x` contributes only its inset (`-at.x`, the narrowest label it
  fits on), never a frame-dependent term, because the frame width is exactly the unknown being
  solved for.
- A text item whose *width* is frame-dependent (an `auto` size, or a `to` whose width XORs to
  frame-dependent — see decision 5) is measured at its natural content width via the same
  fit-and-shrink pass used for fixed-width auto-length text, and that natural width is what
  contributes to the label's content extent. Several full-width centered lines each measure at their
  own text width, so the label sizes to the longest of them, exactly as a fixed-width auto-length
  item would.
- A `line`'s edge-relative endpoint follows the leaf-item rule: each endpoint contributes `|x|`, so
  an inset endpoint contributes its inset and a `-0.0` endpoint (the full-width divider) contributes
  `0`. An earlier draft of this design filtered edge-relative endpoints out of the measurement
  entirely, which contradicted the "its inset, the narrowest label it fits on" principle above and
  let the label resolve narrower than the line's own inset: `at: [-20, y], to: [-0.0, y]` beside
  content-sized text failed with "a coordinate resolves outside the frame" while the equivalent
  right-anchored box rendered. The line rule now matches the box rule.
- A container whose own position is edge-relative but whose *inner width* is not (see decision 8) is
  measured through: its children still contribute their content width, and the container itself adds
  only its own inset on top.

**5. Frame-dependence is the XOR of the two corners.** For a `to`-extent box, `size = to.x - at.x`.
Each edge-relative corner independently contributes one `frame_width` term to its resolved value
(`resolve_coord(v, frame_width) = frame_width + v`), so when *both* corners are edge-relative the two
`frame_width` terms cancel in the subtraction and the width is a compile-time constant — a
right-anchored box of fixed size. When *exactly one* corner is edge-relative, exactly one
`frame_width` term survives and the width is genuinely frame-dependent. `width_is_frame_dependent`
(`models.rs`) is therefore `at.x().is_sign_negative() != to.x().is_sign_negative()`, an XOR, not a
predicate on `to.x` alone. A predicate keyed only on `to.x`'s sign gets two representative cases
backwards in opposite directions: `at: [-20], to: [-0.0]` (both edge-relative, both signs negative)
is a **fixed** 20-unit box anchored to the right edge, correctly resolved regardless of frame width;
`at: [-20], to: [90]` (one edge-relative, one plain) is **genuinely** frame-dependent, since only the
start corner tracks the frame. A `to.x`-only test would flag the first as dependent and miss the
second.

**6. The measure budget subtracts the right-edge inset.** When a frame-dependent text or container
is measured, the width available to it is `budget_w - at.x - inset`, where `inset` is `-to.x()` for
an edge-relative `to.x` (0 otherwise) — the margin the item itself asked for. Subtracting it before
fitting, rather than fitting to the full remaining budget and adding the margin back afterward, means
the item's total footprint (`at.x + width + inset`) can never exceed `budget_w`: the text is wrapped
and shrunk to fit inside its own margin from the first pass, so it can never be clipped by the margin
it asked for.

**7. Validation splits across compile time and render time; render-time bounds checking is new.**
Before this work, `render/mod.rs` validated numeric `size` values and nothing else — coordinate and
bounds enforcement lived entirely in `templates.rs` at load time, because every coordinate was a
constant once the template parsed. That stopped being true here: on a dynamic-width `single`, an
edge-relative `x` resolves against the label's *final* width, which is not known until the measure
pass runs at render time. Compile-time validation (`templates.rs`) still runs first and is
conservative — it bounds an edge-relative `x` against `format.width.max`, the widest the label could
ever be, so a template that can never fit is rejected at load regardless of what any particular
request's data happens to measure to. `render/mod.rs` gained a second, exact check
(`RenderContext::check_box_bounds`, `check_line`) against the frame width actually resolved for this
render, because the load-time bound is necessarily looser than the per-request truth.

**8. A container is measured through even when its own position is edge-relative, as long as its
inner width is not frame-dependent.** This is the narrowest and least obvious rule in the set, and it
cost a fix round to get right (`960e885`, refs #147). A container's *position* (`at.x`) can be
edge-relative while its *extent* is fully known — `validate_placement_position` already forbids
pairing an edge-relative `at.x` with a frame-dependent width on a dynamic-width template, so whenever
a container's own position is edge-relative, its width is by construction a compile-time constant.
Its children's fits therefore depend only on that known inner width, never on where the container
itself ends up sitting once the label's final width resolves. The first implementation skipped the
whole subtree of a right-anchored container during measurement (treating it like a leaf item, which
*is* correctly skipped, since a leaf's inset is all it can contribute) — that desynchronized the
measure and render passes' `MeasuredText` cursor: `render_container_item` recurses into every
container unconditionally and has no such skip, so it consumed cursor entries the measure pass had
never pushed, producing a `500 RenderFailed` ("auto-length cursor mismatch"). The fix gives
`Container` its own clause in `RenderContext::measure` that measures the subtree with the container's
known inner width as the child budget, then adds only the container's own inset on top.

**9. The §4.2.1 rotated-container restriction is narrower than an early sketch of this design.** The
existing rule that a rotated container's inner canvas must resolve at compile time (ADR-0036: no
`auto`, anywhere in the subtree) is left alone in full — it was never in scope to relax. What is new
is that `to:` gets the same frame-dependence test as everywhere else: a rotated container's `to` is
rejected only when its width is frame-dependent (one corner edge-relative, one not) *and* the
template is dynamic-width; it is accepted when both corners are plain (always was), and now also when
both are edge-relative, since those cancel to a constant exactly as decision 5 describes. On a
fixed-width template `width_is_frame_dependent` is moot (there is no frame whose width varies), so
the `to`-frame-dependence branch of the check is skipped entirely.

**10. Dynamic-width mode is a property of the template's `format`, not of whether the measure pass
produced any `MeasuredText`.** Fixed earlier in this branch (`bb1dd03`) as a prerequisite: a
dynamic-width `single` whose content-sizing item is a line or a non-text container (both now
possible per decisions 4-6) produces an empty `measured` vector, and the render pass previously used
`measured.is_empty()` to decide whether it was even in auto-length mode — inferring "no text was
measured" as "this is not a dynamic-width label" and rendering such a container at the full frame
width instead of its measured content width. `LengthMode::{Fixed, Dynamic(AutoLength)}` replaces that
inference with a direct read of `format.width`, so `Dynamic` correctly carries an empty `texts` slice
when the label is sized by geometry alone.

**11. Deferred: intrinsic content sizing for `qr` and `image` items.** Neither a QR (modules ×
module_size plus quiet zone) nor a raster image (pixel dimensions) is measured for its natural size.
`size: auto` on either resolves to the remaining budget (it fills), so on an auto-length label
neither item type can size the label to itself, and a `to`-sized one contributes `0` to the measured
content extent rather than its own footprint. This is a new measurement capability, not a placement
change, so it is out of scope here and filed as a follow-up issue,
[#149](https://github.com/pfa230/labeler/issues/149).

**12. Defer only what is genuinely undecidable at load; "dynamic-width" is not itself a reason to
skip a check.** Decision 7 splits validation across load and render, and the first cut of that split
was too eager: it skipped a check whenever the template was dynamic-width *and* a coordinate was
edge-relative, without asking whether the frame width actually survived into the inequality. Two did
not. The box bound `resolve_coord(at.x, W) + width <= W` reduces to `at.x + width <= 0` for an
edge-relative `at.x` — `W` cancels — and `validate_placement_position` already guarantees such an
item's width is a compile-time constant, so deferring it let templates load that every render then
rejected. A `line`'s *plain* endpoint past `width.max` is likewise a constant no final width can
bring back inside. Both are now rejected at load. What remains deferred is only what depends on the
resolved width: an edge-relative endpoint's degeneracy against a plain one, and the render-time
mirror of the bounds checks (SPEC §7).

**13. A zero extent is a render-time outcome; a negative one is an authoring error.** `to`-extent
resolution rejects `<= 0` at load, where it resolves against the `width.max` frame and a
non-positive result means the corners are genuinely inverted or degenerate. At render it rejects only
a *negative* extent. An empty data value measures to nothing, so the label can clamp to exactly the
item's own `at.x` and a `to`-spanning box collapses to zero width; blank optional fields are ordinary
in CSV-driven printing, and a zero-width Typst box emits harmlessly, so failing the whole render with
a `422` would make a legitimate template data-dependent. The `size: [auto, ...]` spelling of the same
item already rendered an empty box, so this also removes an inconsistency between the two spellings.

## Consequences

- A template can anchor an item to the right or top edge (`at: [-0.0, y]` / `at: [x, -0.0]`), inset
  from an edge (`at: [-5.0, y]`), or describe a box by its opposite corner (`to: [x2, y2]`) instead of
  `size`, on any frame — page, container inner box, or rotated container's swapped author canvas.
  `to`-based right-anchoring composes with auto-length labels: a full-width divider or a
  right-anchored label can size, or be sized by, the same label. The acceptance fixture
  (`tests/fixtures/templates/brother_24mm_lines_divider.yaml`, refs #146/#147) renders two
  independently centered lines and a full-width rule on a content-sized 24 mm tape label with no
  fixed width in the format at all, verified by rendering and inspection rather than by parsing.
- Coordinate and bounds checking now runs twice for anything that can be edge-relative on a
  dynamic-width template: once conservatively at load (`templates.rs`, bounded by `width.max`) and
  once exactly at render (`render/mod.rs`, against the resolved frame). The two must stay in sync,
  same as the pre-existing size/bounds duplication described in §7 of the SPEC.
- `Extent` as an enum touches every `Placement` construction site (`raw.rs`, `convert.rs`,
  `models.rs`, `templates.rs`, `render/mod.rs`; ~43 sites). `Placement::sized` is added as the common
  `at`/`size`, no-bounds, no-rotation constructor to keep the ordinary case from re-spelling
  `Extent::Size(..)` everywhere.
- `Extent::To` was intentionally left unimplemented (returning a "not supported yet" error from both
  resolvers) for one commit in the middle of this branch, then completed two commits later — a
  deliberate sequencing choice to keep the mechanical 43-site diff separate from the commit a reviewer
  has to read for placement logic. No released state ever shipped a half-implemented `to`.
- Rejected: measuring `qr`/`image` content size in this change (decision 11 — separate capability,
  filed as a follow-up rather than folded in here, [#149](https://github.com/pfa230/labeler/issues/149));
  a `to.x`-only frame-dependence predicate (decision 5 — demonstrably wrong in both directions);
  negative `y` meaning "from the bottom" as #146 first phrased it (decision 2 — redundant with
  non-negative `y` and asymmetric with `x`).
