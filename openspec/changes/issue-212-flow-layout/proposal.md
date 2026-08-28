## Why

[#212](https://github.com/pfa230/labeler/issues/212). Every layout item is placed by its own `at`, so
a child's position never depends on what its siblings turned out to be. Two consequences are reachable
with nothing but what ships today:

- A `when:`-gated child leaves a hole. Gating skips an inactive item and every surviving sibling keeps
  its own `at`, so hiding the middle line of a three-line column prints a gap where that line was.
- A child sized by its content cannot be followed by anything. `content` on a text resolves to the
  width of an interpolated value nobody knows until the data arrives, so an item placed after it is
  either overlapped or stranded in whitespace.

[#226](https://github.com/pfa230/labeler/issues/226) supplied the half this needs. Every node now
reports a **claim** upward and takes its frame downward, `content` means intrinsic size on every
format and both axes, and a container's intrinsic size is already defined in terms of its children
(`layout-sizing`, ADR-0080, ADR-0081). What is missing is only the **arrangement**: nothing combines
those claims into positions. This change adds the one that does.

## What Changes

- `container` gains an optional `flow` block: `{ direction, gap, wrap, line_gap, overflow }`. Its
  presence is what switches that container from placing children by coordinate to packing them; a
  container without it is unchanged in every respect.
- A **packed child** (a direct child of a container that has `flow`) carries no position. `at` and
  `to` on one are refused at load, because the container decides where it goes.
- A `line` cannot be a packed child. This is derived, not an exception: `layout-sizing` makes a `line`
  contribution-only and never asks it for an intrinsic size, so it has no claim to pack.
- **A packed child's box comes from the child, never from the arrangement.** It is sized by
  `layout-sizing` against the container's padded inner box, exactly as the same item would be at
  `at: [0, 0]` in an absolutely arranged container, and the arrangement then positions that box
  without altering it. Packing advances by that one number.
- **`fill` keeps its ordinary meaning** on a packed child: the container's padded inner extent, the
  same thing it means for any other child of that container. It does not mean the room the arrangement
  has left, because the arrangement never supplies a box. The consequence is stated rather than
  smoothed over: a `fill` child occupies a whole line, so a sibling sharing that line overflows and,
  under the default `overflow: fail`, fails loudly. Giving `fill` the leftover-room meaning is
  [#260](https://github.com/pfa230/labeler/issues/260), which needs a contract for the circularity that
  meaning creates.
- A flow container's **intrinsic size** is its assembled arrangement, not the largest child
  requirement: the largest line total on the main axis, and the sum of the line extents plus one
  `line_gap` between each adjacent pair on the cross axis, plus padding. This changes the two
  `layout-sizing` requirements that state how a container aggregates its children.
- `wrap` requires the container's main axis to be **resolved**, reusing `layout-sizing`'s own
  predicate. That makes it the second rule to consult that state, so the requirement saying the
  shrinking-`to` rule is the only one is amended to name both.
- **Line selection, trimming and overflow are decided at render.** Load cannot measure, so it takes a
  content source to yield its available extent (`layout-sizing`); running an arrangement on those
  substitutes would hand the first packed child the whole box. Load validates the structural rules and
  checks each packed child against the inner box as if it were the only one.
- With `wrap: true`, a child that does not fit the remaining main-axis room starts a new line.
- `overflow` decides what happens to a child that still does not fit: `fail` (the default) raises the
  render error, `trim` drops that child and every child after it. The vocabulary matches the `text`
  `overflow` field ADR-0082 introduced, and the default differs from text's for a stated reason: an
  ellipsis is visible on the label and a dropped child is not.
- ADR-0083 records the decision. Not a breaking change: every template that renders today renders
  identically.

**Out of scope**, by decision with the issue owner: marking a trim on the label, reporting a trim to
the caller, and cross-axis alignment control. Re-breaking a child's own text is not out of scope so
much as not this capability's business at all, since ADR-0082 already settles what a `text` does
inside whatever box it is given. List-valued data
([#213](https://github.com/pfa230/labeler/issues/213)) stays separate: this change assembles the
children the author wrote, however many that is.

## Capabilities

### New Capabilities
- `flow-layout`: how a container arranges its children by packing rather than by coordinate — what a
  `flow` block declares, what a packed child may and may not carry, what frame a packed child is
  sized against, how lines are chosen, and what happens when the packed content does not fit.

### Modified Capabilities
- `layout-sizing`, three requirements, none of which changes how any existing template is sized:
  - *An intrinsic size is a content extent times a scale* — the `container` row of the extent table
    aggregates children by the container's arrangement rather than by a single stated rule.
  - *A container establishes a padded frame, and rotation swaps it* — names the two arrangements and
    keeps the rotation clause pointing at whichever one applies.
  - *A frame's axis is resolved unless something inside it decides its size* — its "the only rule that
    consults it" clause becomes two rules, the second being `wrap`.

## Impact

- **Template schema**: `raw.rs` (`ContainerRaw`, a new `FlowRaw`), `models.rs`
  (`LayoutItem::Container` gains `flow: Option<Flow>`), `convert.rs` (the `TryFrom` that refuses a
  packed child's `at`/`to` and a packed `line`). The three move together,
  per ADR-0002.
- **Sizing**: `resolver.rs` gains the arrangement: one function turning an ordered list of desired
  extents and a padded inner box into a rectangle per placed child plus the assembled extent. It has a
  single implementation, beside `claim`, `available` and `requirement`, and the render walk is its only
  caller: load has no measured extents to give it (ADR-0080), so load checks the structural rules and
  each packed child against the inner box instead.
- **Rendering**: `render/mod.rs` places packed children at the rectangles the arrangement returns
  instead of at their own `at`.
- **API surface**: the new `Flow` model registers in `src/openapi.rs`, and `TemplateDetail` gains it
  as an optional container field. `Placement.at` becomes optional in the domain model and is omitted
  when absent, so a packed child round-trips through `GET /api/templates/{id}` as authored instead of
  being returned with the `at: [0, 0]` the schema refuses on the way in. No endpoint or request shape
  changes; the response schema gains two optional properties.
- **Web UI**: none required. The field walker recurses on `type === "container"` and reads `items`
  (`ui/src/lib/templateFields.ts:276`), which a flow container still has.
- **Docs**: ADR-0083 plus its row in `docs/adr/README.md`; `docs/AUTHORING.md` gains the worked
  example. `docs/SPEC.md` stays frozen. This is a first touch of two of its clauses that #226 left
  authoritative, and the delta names both: the `at` and `to` rows of the §4 placement table, for packed
  children only, and the `container` field list of §4.1, which gains `flow`.
