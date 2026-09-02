TREE_SHA256: 277e725866508f89580724c0ee6cfb7db44ca02fe33fee5796ed5f8e3e739a24

## Diff review — issue-287-container-geometry (round 2)

Reviewed: `src/{raw,models,convert,templates,resolver,render/mod,reason,openapi,lib}.rs` and the nine new fixtures, against `proposal.md`, `specs/shape-paint/spec.md`, `design.md`, `tasks.md`, `ANSWERS.md`, `review.md`, `diff-review-1.md` and `AGENTS.md`.

Gates run here: `cargo fmt --check` = 0, `cargo clippy --all-targets --all-features` = 0, `cargo test` = 0 (all green). `diff-review-1.md` findings 1 and 2 are fixed: `container_circle_gated.yaml` now declares `w: default 20` against `size: ["{w}", 20]`, and `validate_circle_containers(&self.layout, ...)` (`src/templates.rs:1163`) is a scoped addition, leaving `validate_layout(&instantiated.layout, ...)` untouched at `src/templates.rs:1155-1161`. `SPECS_SHA256` still matches (`87a5c970...`).

### Blocking

**1. The render-time squareness check runs inside the measurement probe, against a frame the label does not render in. A dynamic-width single therefore both accepts an oval under `shape: circle` and refuses a circle that would render square.** [verified, empirically]

The check sits in `measure_items` (`src/render/mod.rs:1321-1372`). For a dynamic-width single, `measure_items` is called with the frame `(max_w, height_units)` (`src/render/mod.rs:562-568`), and only afterwards is the real width chosen: `width_units = root_w_req.clamp(min_w, max_w)` (`src/render/mod.rs:569`). `render_items` then resolves every box against `(width_units, height_units)`. Frame-sourced and content-sourced extents resolve through `available(frame, spec)`, which is frame-dependent (`src/resolver.rs:171-177`), and a `fill` axis contributes nothing to `root_w_req`, because `intrinsic` is `None` for an axis that does not demand a measurement (`src/render/mod.rs:1437-1442`) and `claim` then yields 0 (`src/resolver.rs:208-226`). So the circle does not pin the width it is judged at.

Proven against a running server (`LABELER_NO_AUTH=true`, 200 dpi), two templates, both loading clean (`broken=0`):

- **Oval accepted.** `width: {min: 10, max: 60}`, `height: 60`, a `text` at `size: [20, 5]`, and `container` with `shape: circle, at: [0,0], size: [fill, 60]` → `200 OK`, PNG **157 × 472 px** = 20 mm × 60 mm. The rendered image is a 20 × 60 oval drawn under `shape: circle`. Measure resolved 60 × 60 and passed; render resolved 20 × 60 and was never checked.
- **Circle refused.** The same template with `height: 20` and `size: [fill, 20]` → `422 UnsupportedLayoutItem`, `details.reason` `circle_box_not_square`, message `circle container at 'layout[1]' is not square`. The byte-identical template with `shape: ellipse` renders **157 × 157 px** = 20 mm × 20 mm, so the box the render would have resolved is square and the refusal is false.

This is the guarantee the change exists for. `specs/shape-paint/spec.md` requires that at render "every **active** `circle` that reaches measurement or rendering SHALL have its **resolved** box checked", the content scenario says "square renders the circle, non-square is refused", and `design.md` states the goal as "a squareness guarantee for `circle` that holds for every request, wherever the extents resolve". The delta's own spelling table names this exact case as the reason a frame source defers to render: "the frame follows the label's own sizing, which a dynamic-width label decides per render". The check has to be taken where the final box is resolved, not in the probe.

### Major

**2. Task 6.6's byte-identity claim is checked but not performed.** `tasks.md` 6.6 reads "renders byte-identically to the same template before this key existed". `src/lib.rs:10577-10580` compares `container_default_rect` (no `shape`) against an inline copy carrying `shape: rect`. Both go through the new single-`#box` emitter, so the test proves the default mapping and says nothing about the pre-collapse `#rect` output. Nothing in the suite compares against it. This was `diff-review-1.md` finding 5; it was not addressed and the box was checked anyway. `AGENTS.md`: "check one only after performing it."

**3. Five fixtures are exercised only by `every_template_renders`.** `container_ellipse_padded`, `container_ellipse_square`, `container_ellipse_stroked_cross`, `container_rect_rounded_corner` and `container_rect_stroked_edge` appear once each outside their own file, in the expected-id list at `src/render/mod.rs:5297-5305`. The `tasks.md` footer fairly declares the *clipping* evidence visual, but two of the delta's scenarios are not visual: "an ellipse touching all four sides" and "a square box makes the ellipse a circle" are assertable in the emitted Typst source, and no test reads these fixtures for that. `AGENTS.md`: "what makes them right is the test that reads them."

### Minor

**4. Draw order lost its assertion in the one place the emitter still owns it.** The old test asserted `rect_idx < child_idx`; it is now two `contains` checks (`src/render/mod.rs:7683-7684`). For `rect` the ordering is Typst's and that is fine. For a round geometry the emitter decides it, placing `#ellipse` before the child `#box` (`src/render/mod.rs:2196-2216`), and `src/render/mod.rs:7908-7909` asserts both are present without asserting which comes first. The paint-coverage requirement mandates background, then stroke, then children.

**5. The check re-implements `resolver::place`'s composition inline.** `src/render/mod.rs:1330-1362` duplicates the `resolve` / `resolve_unmeasured` branch of `place` (`src/resolver.rs:427-446`). `AGENTS.md`: "Adding a source or a bound means editing `resolver.rs` alone." Carried over from `diff-review-1.md` finding 7, and related to finding 1: a check taken where `place` is actually called would have had the render frame in hand.

**6. `docs/AUTHORING.md` §9 is stale.** The section is titled "Containers and shape paint" (`docs/AUTHORING.md:483`), lists the container paint keys with no `shape:`, and describes `rounded` as rounding "the container's stroke and background" (`docs/AUTHORING.md:496-499`), where the delta now also makes that radius clip children and refuses `rounded` on `ellipse` and `circle`. Carried over from `diff-review-1.md` finding 8.

**7. No PDF assertion for a round geometry.** The delta's ellipse scenario says "this holds in PNG output and in PDF output alike"; only PNG (`src/render/mod.rs:7989`) and Typst-source assertions exist. Low risk, same emitter, but the scenario names both. Carried over from `diff-review-1.md` finding 9.

### Verified as correct

`fixed_by_template` matches the delta's spelling table row for row (`src/resolver.rs:103-165`, test at `883-955`), and is set in `source_of` alone. The load check branches on that flag only, reuses `BOUNDS_EPSILON`, walks nested containers, runs on the un-instantiated layout after `validate_references()` (so the `expect` in `source_of` cannot panic), and reports ordinary structural validation with no new reason (`src/templates.rs:1900-1935`). The render check sits after `is_item_active`, so a gated-off circle is never measured. `circle` emits `#ellipse`, never `#circle`. `rounded` on a round geometry, an unknown `shape`, a null `shape` and `shape` on a non-container are all refused with the value and the accepted set (`src/convert.rs:254-319`). `avery5163_asset_tag.yaml:42-50` is confirmed the only pre-existing stroked container and it declares `items: []`, so the proposal's "nothing renders differently" holds.

I also ran the render-and-look loop on the four visual fixtures, since no task claims them. All four match the contract: `container_rect_rounded_corner` cuts the leading glyph on the rounded corner; `container_rect_stroked_edge` cuts the leading glyph half a thickness in from the box, at the stroke's inner edge; `container_ellipse_stroked_cross` draws its text in full across the painted curve and over the stroke; `container_ellipse_padded` paints an ellipse touching all four sides with the child inset by the padding.

VERDICT: REVISE
