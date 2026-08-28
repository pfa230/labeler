## Why

[#263](https://github.com/pfa230/labeler/issues/263). `layout-sizing` (ADR-0080, ADR-0081,
[#226](https://github.com/pfa230/labeler/issues/226)) states its contract in universals, and every one
of them assumes an item has an anchor and that its frame is known before it is sized. A **packed
child**, the thing [#212](https://github.com/pfa230/labeler/issues/212) needs, has neither: it carries
no `at`, and where it lands depends on the siblings before it.

#212 was planned three times against the pre-#226 engine and twice against the sizing protocol that
replaced it. Every round returned `REVISE`, and every blocking finding was the same shape: an
arrangement was being added on top of a contract that had no room for it. The record is on branch
`issue-212-flow-layout` and is the evidence for this change.

This change amends the contract instead, and ships the smallest arrangement that exercises it. A
contract for packed children cannot be written before packed children can be authored, so it
introduces the concept it specifies.

## What Changes

- `container` gains an optional `flow` block: `{ direction, gap }`. Its presence, and nothing else,
  switches that container from placing children by coordinate to packing them in order. `direction`
  names the **primary** axis, the one children are packed along, the way reading direction is primary
  for words in a line; the other axis is the **secondary** one.
- A **packed child** carries no position. `at` and `to` on one are refused at load, because the
  container decides where it goes. A `line` cannot be a packed child: `layout-sizing` makes it
  contribution-only and never asks it for an intrinsic size, so it has no box to place.
- `layout-sizing` gains the anchorless case as a first-class one rather than an unstated one. Six
  requirements are amended, each carrying its complete post-change contract: the five #263 lists, plus
  the resolved-axis requirement, whose second clause is keyed to a sign-negative anchor and so says
  nothing about an item that has no anchor at all.
- **A packed child is sized by `layout-sizing` and by nothing else.** Its available extent on an axis
  is the container's padded inner extent, because it has no anchor to subtract and no inset to
  reserve. Every rule keyed to the source then applies unchanged, on both axes.
- **`fill` needs no rule of its own and gets none.** `layout-sizing` already says a frame extent
  reports `min(intrinsic, max_*, available)` upward and takes the available extent downward, and that
  asymmetry is what `fill` means. On a packed child it means the container's padded inner extent, on
  either axis. The consequence is stated rather than refused: a `fill` child alone in its container
  stretches to it, and an **uncapped** `fill` child beside a sibling takes the whole extent, so the
  sibling lands past the inner box and fails with `item_out_of_frame`. A `max_w` or `max_h` binds a
  frame extent like any other clamped one, so a capped `fill` child takes its cap and shares its line. Giving `fill` the other meaning, the room the
  arrangement has left, is [#260](https://github.com/pfa230/labeler/issues/260).
- **A `when:`-gated child occupies no slot**, which closes the hole that motivated #212: hiding the
  middle line of a column no longer prints a gap where that line was.
- **A `gap` separates two children that occupy primary-axis space.** A child whose primary extent
  resolves to zero separates nothing, so no gap falls on either side of it. It is still drawn at the
  cursor, still contributes its secondary extent, and still raises its own errors. This is the reading
  of "the space between two adjacent children", not an exception to it, and it keys on the resolved
  extent rather than on whether a value was empty.
- **A packed child round-trips as authored.** `Placement.at` becomes optional in the domain model,
  `None` legal only for a packed child, so `GET /api/templates/{id}` returns no anchor rather than the
  `at: [0, 0]` the same schema refuses on the way in.
- **A packed container with no `size` still defaults to `size: [fill, fill]`**, like every other
  container, so two chips authored without one each take the whole inner extent and the second fails.
  No separate default is invented for a packed child: the consequence is stated and given a scenario
  instead, because a spelling that resolved differently according to its parent would make a container
  unreadable on its own.
- **Accumulated packing can overflow at render**, and fails with the existing `item_out_of_frame`, on
  either axis and whichever edge lies outside. A packed child never raises `coord_out_of_frame`,
  because that slug reports a coordinate outside its frame and a packed child has no coordinate. No
  new reason is added. `layout-sizing`'s guarantee that nothing accepted at load can overflow at
  render for want of a measurement holds for one item's own extent and cannot hold for an accumulation
  of siblings, so it is scoped to what it can promise.
- ADR-0083 records the decision and **amends ADR-0080 §1 and ADR-0081 §1**, both of which define a
  quantity this change extends (`available`, and `fill`) in terms of an anchor a packed child does not
  have. Their `docs/adr/README.md` rows gain the amendment annotation those ADRs already use. Not a
  breaking change: every template that renders today renders
  identically, and every container without a `flow` block is unchanged in every respect.

**Out of scope, and left in #212:** `wrap`, `line_gap`, and the `overflow: fail | trim` policy. Every
blocking finding of #212's rounds 4 and 5 lived in one of those three, so they belong after the
contract is settled rather than in the change that settles it. Cross-axis alignment control, marking
or reporting a dropped child, list-valued data
([#213](https://github.com/pfa230/labeler/issues/213)) and the leftover-room meaning of `fill`
(#260) are out as well.

## Capabilities

### New Capabilities
- `flow-layout`: how a container packs its children in order instead of placing each by its own
  coordinate. What a `flow` block declares, what a packed child may and may not carry, which children
  occupy primary-axis space, what the container assembles from the result, and what happens when the
  packing runs past the padded inner box.

### Modified Capabilities
- `layout-sizing`, six requirements, none of which changes how any template that exists today is
  sized:
  - *An extent comes from the author, from the content, or from the frame*: defines the available
    extent of an item with no anchor. The formula degenerates to the frame, and "degenerates" is not a
    contract.
  - *An item requires of its frame the smallest extent that contains it*: a packed child is a seventh
    case beside the six placement spellings and the `line`. Its requirement is its claim, with no
    anchor term.
  - *An intrinsic size is a content extent times a scale*: the `container` row aggregates children by
    the container's arrangement, the largest requirement when absolute and the assembled extent when
    flow.
  - *A container establishes a padded frame, and rotation swaps it*: names the two arrangements and
    keeps the rotation clause pointing at whichever one applies.
  - *A frame's axis is resolved unless something inside it decides its size*: says which of its
    clauses decides an item with no anchor, rather than leaving it to an absence. Not one of the five
    #263 names; the first plan review found that the clause reading "a frame source under a
    sign-negative anchor" cannot classify an anchorless item, and a packed `container` establishes the
    state its own children read.
  - *Load-time validation and render-time resolution are one algorithm*: scopes the no-overflow
    guarantee to a single item's own extent, and says what an accumulation does instead.

## Impact

- **Template schema**: `raw.rs` gains `FlowRaw` and `ContainerRaw.flow`; `models.rs` gains `Flow` and
  `LayoutItem::Container.flow`, and `Placement.at` becomes `Option<Position>`; `convert.rs` refuses a
  packed child's `at`/`to` and a packed `line`, and keeps normalising an omitted `at` on every
  absolutely arranged item exactly as it does today. The three move together, per ADR-0002.
- **Sizing**: `resolver.rs` gains the anchorless case in `source_of`, `available` and `requirement`;
  an anchor-free resolve-and-check both stages call in place of the anchored `place`, sharing that
  function's bounds comparison rather than copying it; and the arrangement itself, one function
  turning an ordered list of resolved boxes and a padded inner box into a rectangle per child plus the
  assembled extent. Each has one implementation.
- **Rendering**: `render/mod.rs` aggregates a flow container's intrinsic size by its arrangement, and
  places packed children at the rectangles the arrangement returns instead of at their own `at`.
- **Validation**: `templates.rs` gains this capability's structural refusals. It does not run the
  arrangement: load cannot measure, so it checks each packed child against the padded inner box by the
  ordinary rules, which is the same check that child gets in an absolutely arranged container.
- **API surface**: `Flow` and its direction register in `src/openapi.rs`. `at` becomes optional in the
  response schema. No endpoint or request shape changes, and no existing response changes, because
  every item that could be authored before this change still carries its anchor.
- **Web UI**: none required. The field walker recurses on `type === "container"` and reads `items`
  (`ui/src/lib/templateFields.ts:207-209` and `:291-292`), which a flow container still has, and
  nothing under `ui/src` reads a placement's `at`.
- **Docs**: ADR-0083 plus its row in `docs/adr/README.md`; `docs/AUTHORING.md` gains the worked
  example. `docs/SPEC.md` stays frozen. This is a first touch of two of its clauses that #226 left
  authoritative, and the deltas name both: the `at` and `to` rows of the §4 placement table, for
  packed children only, and the `container` field list of §4.1, which gains `flow`.
