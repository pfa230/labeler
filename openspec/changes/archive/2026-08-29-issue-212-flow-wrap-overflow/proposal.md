## Why

[#212](https://github.com/pfa230/labeler/issues/212), the part of it
[#263](https://github.com/pfa230/labeler/issues/263) deliberately left behind. #263 shipped the flow
arrangement and settled what a packed child is, and its overflow requirement names what is missing in
so many words: "Whether an author can ask for that child to be dropped instead is #212, which adds
`wrap` so a child that does not fit starts a new line and an `overflow` policy so one that still does
not fit can be trimmed. Until then a flow container has one line and an overrun is an error."

So today a flow container has exactly one line. A row of items whose widths come from data either fits
that line or fails the render, and there is no way to say "put the rest on the next line" or "print
what fits". Both are ordinary things to want on a label, and neither is expressible.

## What Changes

- The `flow` block gains three keys: `wrap` (boolean, default `false`), `line_gap` (number ≥ 0,
  default `0`) and `overflow` (`fail` or `trim`, default `fail`).
- With `wrap: true`, a child whose **box** does not fit the room left on its line starts a new line.
  Lines advance along the secondary axis, separated by `line_gap`.
- `line_gap` with `wrap: false` is **inert**, not refused. A container with one line has nothing to
  separate, exactly as `gap` separates nothing in a container with one occupying child.
- The **assembled extent** gains lines: its primary axis becomes the largest line total rather than
  the single total, and its secondary axis becomes the sum of the line extents plus one `line_gap`
  between each adjacent pair, rather than the largest requirement among all active children. A line,
  like a child, has a **box** extent that positions the next line and a **requirement** extent that
  the assembly is built from; they differ only where a line holds a `fill` child.
- `overflow: trim` drops the first child that does not fit and every child after it, from the drawing
  and from the assembled extent. The overrun itself then raises nothing; a trimmed child is still
  sized and evaluated, so the render succeeds only if everything else about the template does.
  `overflow: fail`, the default, keeps today's `item_out_of_frame`.
- Two load-time restrictions, each derived rather than stipulated, and each stated with the failure it
  prevents:
  - `wrap: true` requires the container's **primary** axis to be resolved. You cannot wrap against a
    frame that is derived from you: a `content` primary axis is the assembly of the very children
    being packed.
  - `overflow: trim` requires **both** of the container's axes to be resolved. You cannot overflow a
    frame that grows to fit you, and a trim removes a child from the assembly on both axes, not only
    the one it overran, so an unresolved second axis lets a trim change the container's own size and
    then fail the child it just dropped.
- ADR-0089 records the decision and **amends ADR-0083** in three places, since that record fixes the
  `flow` block to `direction` and `gap`, defines a single-line assembled extent, and says an overrun
  consistently raises `item_out_of_frame`. Its index row is updated to say so rather than leaving two
  accepted records in conflict. Not a breaking change: every template that renders today renders
  identically, since all three keys default to the current behaviour.

**Out of scope**, and unchanged from #212's own list: marking a trim on the label, reporting a trim to
the caller, and secondary-axis alignment control.
[#260](https://github.com/pfa230/labeler/issues/260) still owns giving `fill` on a packed child the
leftover-room meaning; this change keeps the whole-inner-extent meaning #263 shipped.

## Capabilities

### New Capabilities
None. This extends `flow-layout`, which #263 created.

### Modified Capabilities
- `flow-layout`, four requirements:
  - *A `flow` block selects the flow arrangement* — the three new keys, their defaults and their
    refusals.
  - *Packing places the children that take up room along the primary axis* — a leading edge is now
    per line, and a line break is chosen by the child's box.
  - *The assembled extent is what a flow container reports* — both axes gain lines.
  - *Packing past the padded inner box fails where it lands* — its check 2 becomes the `overflow`
    policy, and the paragraph deferring this to #212 is replaced by the rules it was waiting for.

`layout-sizing` is **not** modified. Wrapping and trimming decide position and drawing; no child's box
changes, and #263 already amended every sizing rule a packed child touches.

## Impact

- **Template schema**: `raw.rs` (`FlowRaw` gains three fields), `models.rs` (`Flow` gains them, plus a
  `FlowOverflow` enum), `convert.rs` (the `TryFrom` that applies the defaults and checks what the block
  alone decides). The three move together, per ADR-0002. The two resolved-axis restrictions are not
  among them; see Validation below.
- **Arrangement**: `resolver.rs::arrange_flow` gains lines and the trim policy. It keeps taking
  `FlowChildInput { resolved_box, requirement }` and returning rectangles plus an assembled extent, so
  its callers are unchanged in shape.
- **Validation**: local checks stay in `convert.rs`, where a JSON path exists: the enum values, the
  defaults and the sign of `line_gap`. The two axis restrictions depend on the frame a container is
  given, and the resolved-axis state (`container_inner_axes_resolved`) is available only in the
  recursive layout traversal in `templates.rs`, so they run there and report by message rather than by
  path, as that traversal already does.
- **API surface**: `Flow` is already registered in `src/openapi.rs` and is a serialized API model, so
  its schema gains three optional properties and a template using them returns them wherever a layout
  is returned. Additive: no endpoint, no error envelope and no existing property changes.
- **Web UI**: none required. The field walker recurses on `type === "container"` and reads `items`.
- **Docs**: ADR-0089 plus its row in `docs/adr/README.md`; `docs/AUTHORING.md` gains the wrapped-row
  example. `docs/SPEC.md` stays frozen and is not further superseded: #263 already took the `flow` key
  into §4.1.
