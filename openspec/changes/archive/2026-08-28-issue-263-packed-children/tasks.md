## 1. The anchorless case in the resolver

- [x] 1.1 Add `Anchor::Absent` to `src/resolver.rs`. `available` returns the frame extent for it and `requirement` returns the claim, with no anchor term. `Anchor::resolve` has no answer for it: an `Absent` anchor reaching it is a bug, not a coordinate, and it must say so rather than return zero.
- [x] 1.2 Make `Placement.at` an `Option<Position>` in `src/models.rs`, with `skip_serializing_if = "Option::is_none"`, and fix every construction site the compiler flags. `source_of` classifies an absent `at` as `Anchor::Absent`.
- [x] 1.3 In `src/convert.rs`, keep normalising an omitted `at` on an absolutely arranged item to `Some([0, 0])` exactly as today, so no existing template, response or OpenAPI requiredness changes. Only a packed child carries `None`.
- [x] 1.4 Lift `place`'s inline far-edge comparison into `fits_frame(low, extent, limit) -> Result<(), Violation>`, returning `AnchorBeyondFrame` when `low` passes the limit and `ExtentBeyondFrame` when `low + extent` does, and have `place` call it. One implementation, no second copy.
- [x] 1.5 Add `resolve_packed(placement, inner, geometry_values, intrinsic) -> Result<(f32, f32), Violation>`: the anchor-free part of `precheck` (a written extent must be positive), `resolve` on each axis, then `fits_frame(0.0, extent, inner)` per axis. Both stages call it; load passes no intrinsic, as it does everywhere else.

## 2. The flow arrangement

- [x] 2.1 Add the arrangement to `src/resolver.rs`: given the padded inner box, the `flow` settings, and per child in template order its resolved box extents and its requirements, return a rectangle per child plus the assembled extent. Check the accumulation in **packing coordinates** with `fits_frame(cursor, extent, inner primary extent)`, cursor starting at zero and only increasing; convert the cursor to a drawing coordinate (`x = cursor` for a row, `y = inner_h − (cursor + extent)` for a column) last, and never check a converted coordinate. It reads no request state and calls nothing that measures.
- [x] 2.2 Implement occupancy and gaps: a child occupies the packing axis when it is active and its box's primary extent exceeds zero; the k-th occupying child's leading edge is the sum of the preceding occupying children's extents plus `k − 1` gaps; an active child with a zero primary extent is placed at the leading edge the next occupying child would take, advances nothing, consumes no gap, and still contributes its secondary extent. Every child's secondary leading edge is the padded inner box's leading edge on that axis.
- [x] 2.3 Implement the assembled extent: the sum of the occupying children's **requirements** plus one `gap` between each adjacent pair on the primary axis, and the largest requirement among all active children on the secondary axis. Assembly consumes requirements; positioning consumes boxes. Do not collapse the two.

## 3. Schema and model

- [x] 3.1 Add `FlowRaw { direction, gap }` and `ContainerRaw.flow` to `src/raw.rs`, and `Flow` with its direction enum to `src/models.rs` as `LayoutItem::Container.flow: Option<Flow>`. `direction` is required; `gap` defaults to `0`. The three files move together, per ADR-0002.
- [x] 3.2 In `src/convert.rs`, refuse at load with the JSON path of the offending key: a `flow` block with no `direction` or an unrecognised one; a negative or non-finite `gap`; a packed child carrying `at` or `to`; and a `line` as a packed child.
- [x] 3.3 Register `Flow` and its direction in `src/openapi.rs`, and confirm `at` becomes optional in the response schema without any currently-servable response changing.

## 4. Load and render integration

- [x] 4.1 Point `src/templates.rs`'s placement validation at `resolve_packed` for a packed child instead of `place` (`templates.rs:1547`), so an authored extent larger than the padded inner box is refused where it is written. Load runs no arrangement.
- [x] 4.2 Divert the measuring walk's direct `precheck` call (`src/render/mod.rs:1058-1059`) to `resolve_packed` for a packed child. This is the site independent of `place`, and a packed child reaching it would resolve an absent anchor on every render.
- [x] 4.3 Divert `resolve_placement_box` (`src/render/mod.rs:1501`) for a packed child, and pass the rectangle the arrangement returned down to the existing per-item render path rather than cloning it into a rewritten `Placement`.
- [x] 4.4 Make the container intrinsic arm (`src/render/mod.rs:1323-1366`) aggregate by arrangement: the largest requirement per axis without `flow`, the assembled extent with it, padding added and the author-space pair swapped under rotation in both cases.
- [x] 4.5 Place packed children at the arrangement's rectangles in `render_items`, and confirm a flow container beneath `rotate: 90` or `270` packs in author space by reading the state `container_inner_axes_resolved` already produces.

## 5. Tests

- [x] 5.1 Resolver unit tests for the anchorless case: `available` is the frame, `requirement` is the claim, `Anchor::Absent` never yields a coordinate, and `fits_frame` agrees with `place`'s previous inline comparison on an anchored item.
- [x] 5.2 Arrangement unit tests: gaps between occupying children only, no leading or trailing gap, a gated-off child closing the hole, a zero-primary-extent child drawn at the cursor without a gap and still contributing its secondary extent, and both directions.
- [x] 5.3 Overflow tests proving the slug, and proving it red first: a `row` overrun, a `column` overrun, and a too-tall child each fail with `item_out_of_frame`, and none raises `coord_out_of_frame`. The column case is the one that passes silently if the check is made on a converted drawing coordinate, so assert it against a deliberately wrong implementation before the right one.
- [x] 5.4 Sizing tests: a packed child sized identically to the same item at `at: [0, 0]` in an absolutely arranged container of the same inner box; an uncapped `fill` child alone and beside a sibling; a capped `fill` child sharing its line; a `content` flow container hugging its children; a flow container sizing a dynamic-width label; nesting in both directions; a flow container at a sheet slot root.
- [x] 5.5 HTTP round-trip test at the status-code level: a template with a flow container is returned by `GET /api/templates/{id}` with no `at` and no `to` on its packed children, and the returned document is accepted when submitted back unchanged.
- [x] 5.6 Load-refusal tests for each structural refusal in 3.2, each asserting the quarantine and the JSON path.

## 6. Docs

- [x] 6.1 Write ADR-0083, "A packed child is anchorless, and its container's arrangement places it". Its Status names ADR-0080 §1 and ADR-0081 §1 as amended, because both define a quantity this change extends (`available`, and `fill`) in terms of an anchor a packed child does not have. Re-check the highest ADR number on `main` first and take the next free one.
- [x] 6.2 Add ADR-0083's row to `docs/adr/README.md`, and add "(amended by [0083](0083-...))" to the ADR-0080 and ADR-0081 rows, matching how ADR-0036 and ADR-0051 are annotated for ADR-0080.
- [x] 6.3 Add the worked flow example to `docs/AUTHORING.md`, covering `direction`, `gap`, what a packed child may not carry, and the two `fill` outcomes. `docs/SPEC.md` stays frozen.

## 7. Verification

- [x] 7.1 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test`, and fix what they flag rather than silencing it.
- [x] 7.2 Render and inspect the acceptance cases in `design.md`'s "Acceptance evidence", opening each PNG and checking it against intent. This is evidence, not a checkable claim: record what each render showed, including the case that failed and why, rather than reporting a pass.

  Renders at 300 dpi through POST /api/render/label and POST /api/batch, every image opened:
  - Gated column: gate on gives first/middle/third; gate off moves third up into middle's place, no hole.
  - Row of content-sized text: correct at a short and a longer title, and the label's own width grows with the assembled extent.
  - Empty middle child: AAA and CCC separated by exactly one 4mm gap, and the label narrowed by the missing child plus one gap.
  - Nesting: flow inside absolute, absolute inside flow and flow inside flow render together, gaps and child frames where expected.
  - Rotation: rotate 90 over a row packs in author space; the label reads bottom-to-top with the gap preserved.
  - Column overrun: 422 item_out_of_frame at layout[0].items[2], never coord_out_of_frame.
  - fill: alone it takes the whole padded inner width; capped with max_w it shares the line with a QR; uncapped beside a sibling it fails 422 item_out_of_frame, which is what AUTHORING.md tells an author to expect.
  - Sheet slot root: three Avery slots, each packing its own row.
  - content multiline with a font_size range: laid out against the container's padded inner box and wrapped at that width.
  - What failed and why: the same multiline child placed after a QR fails 422 item_out_of_frame. That is the contract, not a defect, and it is finding 12.

