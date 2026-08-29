## 1. Schema

- [x] 1.1 Add `wrap: bool`, `line_gap: Option<f32>` and `overflow: Option<FlowOverflow>` to `FlowRaw`
      in `raw.rs`, keeping `deny_unknown_fields`.
- [x] 1.2 Add the matching fields and a `FlowOverflow { Fail, Trim }` enum to `Flow` in `models.rs`,
      with `Fail` as the default, and register `FlowOverflow` in `src/openapi.rs`.
- [x] 1.3 In `convert.rs`, apply the defaults and refuse what the block alone decides: a negative or
      non-finite `line_gap`, and an `overflow` that is neither `fail` nor `trim`, each naming the JSON
      path. Do **not** put the axis restrictions here; they need frame context (design §5a).
- [x] 1.4 Assert in a test that `line_gap` with `wrap: false` loads and changes no geometry, so the
      inertness in the spec is pinned rather than assumed.

## 2. The two axis restrictions

- [x] 2.1 In the recursive layout traversal in `templates.rs`, where
      `container_inner_axes_resolved` is already called, refuse `wrap: true` when the container's
      **primary** author axis is unresolved, naming `wrap`.
- [x] 2.2 In the same place, refuse `overflow: trim` when **either** author axis is unresolved,
      naming `overflow`.
- [x] 2.3 Cover both refusals for `row`, for `column`, and under `rotate: 90` and `rotate: 270`, since
      the quarter turn swaps which physical axis each author axis is.
- [x] 2.4 Assert the accepting cases too: a `fill` container under a sign-negative anchor is resolved
      and accepts `wrap`, and `overflow: fail` is accepted on every spelling the other two refuse.

## 3. Arrangement

- [x] 3.1 Extend `arrange_flow` in `resolver.rs` to break lines: an occupying child whose **box**
      primary extent does not fit the room left starts a new line and is its first occupying child.
- [x] 3.2 Track both line extents: the largest secondary **box** among the children drawn on a line,
      which positions the next line one `line_gap` later, and the largest secondary **requirement**,
      which feeds the assembled extent.
- [x] 3.3 Assemble from lines: the largest line total on the primary axis, and the sum of the line
      requirement extents plus one `line_gap` between adjacent pairs on the secondary.
- [x] 3.4 Assign a zero-primary-extent child to the line current when the arrangement reaches it, drawn
      at that line's cursor, triggering no break and following none.
- [x] 3.5 Implement `overflow: trim`: stop at the first child failing check 2, leave it and every later
      child undrawn and out of the assembled extent, and raise nothing for the overrun.
- [x] 3.6 Keep check 1 ahead of the policy, so a child whose own resolved extent exceeds the padded
      inner box still fails under `trim`.
- [x] 3.7 Unit-test the boundary the review asked for: a child whose box plus its pending `gap` exactly
      equals the room remaining stays on the current line, given `BOUNDS_EPSILON`.

## 4. Rendering

- [x] 4.1 Pass the new `Flow` fields through both arrangement call sites in `render/mod.rs`, the
      intrinsic pass against the provisional frame and the final pass against the resolved one.
- [x] 4.2 Skip drawing trimmed children while still sizing and evaluating them, so a trimmed
      `content`-sized `qr` or `text` still raises and a trimmed authored-size `image` does not.

## 5. HTTP behaviour

- [x] 5.1 Add an HTTP test that a wrapped template renders `200` and one that an unwrapped overrun
      still fails with `UnsupportedLayoutItem` / `item_out_of_frame`.
- [x] 5.2 Add an HTTP test that `overflow: trim` returns `200` for a layout that `overflow: fail`
      refuses, asserted at the status code rather than a layer below it.
- [x] 5.3 Add an HTTP test that a trimmed `text` whose value interpolates a missing field still fails
      with `MissingField`, and that a trimmed authored-size `image` with an absent data key does not.

## 6. Docs and decisions

- [x] 6.1 Write `docs/adr/0089-wrapping-and-the-overflow-policy.md`, naming the three statements of
      ADR-0083 it amends: the `flow` block declaration, the single-line assembled extent, and the
      unconditional `item_out_of_frame` on overrun.
- [x] 6.2 Add the ADR-0089 row to `docs/adr/README.md` and mark ADR-0083's row as amended by it.
- [x] 6.3 Add the wrapped-row worked example to `docs/AUTHORING.md`.

## 7. Verify

- [x] 7.1 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test`, and fix what
      they flag rather than silencing it.
- [x] 7.2 Render the acceptance set in `design.md` to PNG against a running server, open each image and
      check it against intent, fix and re-render until correct. This is not a checkbox anyone can
      verify later (`AGENTS.md`, "Templates are visual artifacts"): check it only after actually
      looking at the images.
