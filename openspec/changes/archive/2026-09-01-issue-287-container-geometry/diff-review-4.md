TREE_SHA256: 78e849ebb53d5d131649a07c638a673e3202d70561c5c4553a873862c7f30a34

Reviewed the full diff (`src/{raw,models,convert,templates,resolver,render/mod,reason,openapi,lib}.rs`, `docs/AUTHORING.md`, nine fixtures) against `proposal.md`, `specs/shape-paint/spec.md`, `design.md`, `tasks.md`, `ANSWERS.md`, `review.md`, `diff-review-1..3.md` and `AGENTS.md`.

**Gates run here** [verified]: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features` clean with no warnings, `cargo test` green (753 lib tests, 0 failed, 2 ignored; plus 3 integration tests). `SPECS_SHA256` recomputes to `87a5c970…`, matching `review.md`; `AUTHOR: claude` / `REVIEWER: codex` differ; `review-gate-check.sh --plan-only` passes over the changed files.

## The five open findings from round 3

All five are addressed [verified]:

1. **Dynamic-width regression test** is now `circle_dynamic_width_frame_sourced_extent_checked_at_final_frame` (`src/render/mod.rs:8054-8137`), and it is genuinely load-bearing. `compile_label_source` measures a dynamic-width label against `(max_w, height_units)` (`src/render/mod.rs:562-567`), so a check inside `measure_items` sees `fill` resolve to 60 in `DynOval` and passes, while the final frame is 20 wide: `unwrap_err()` would panic. The square counterpart (`DynCircle`, final width 20, `size: [fill, 20]`) would be falsely refused at the probe frame of 60, so `.expect("square circle must render")` would panic too. Both directions are pinned. I did not re-apply the round-2 code to observe the failures; the red-before-green claim above is [verified] by reading the probe frame, not by running it.
2. **`docs/AUTHORING.md:494-499`** now documents the stroke clipping reversal alongside the radius one.
3. **`src/templates.rs:2919, 2926, 2933`** now assert `contains("must be square")`. The message matches the surrounding convention (`"stroke thickness must be finite and >= 0.0001"` at `src/templates.rs:2070` carries no path either), so the absent JSON path is not a new inconsistency.
4. **The JSON path is now asserted** at `src/render/mod.rs:7679, 7719, 7752, 7789, 7833`, `src/lib.rs:10412, 10440, 10473, 10499` and on the batch failure entry.
5. **`shape` no longer serializes by default**: `#[serde(default, skip_serializing_if = "Shape::is_default")]` (`src/models.rs:1087-1088`) matches its four `Container` neighbours, so `GET /api/templates/{id}` is unchanged for pre-existing templates and `proposal.md`'s Impact stays true as written.

## Blocking

**1. `tasks.md` 4.2 is checked but describes the placement that was round 2's blocker.** `openspec/changes/issue-287-container-geometry/tasks.md:47` reads "Check every active `circle`'s resolved box at render, **on the walk that measures**, so an item excluded by a false `when:` is never measured and never checked". The implementation deliberately does not do that: the check is at the top of `render_container_item` (`src/render/mod.rs:2070-2077`), on the emission walk, because the measurement walk runs against `(max_w, height_units)` for a dynamic-width label (`src/render/mod.rs:562-567`) and therefore judges a box that is not the resolved one. That is exactly why `diff-review-2.md` sent this back. `design.md:129-130` repeats the claim ("The check belongs on the same walk that measures, and then it inherits that exclusion instead of restating it"), and both files archive permanently under `openspec/changes/archive/`.

The behaviour the task's clause buys is still true, but by a different mechanism: `render_items` filters on `is_item_active` before dispatching (`src/render/mod.rs:1657`), so a gated-off container never reaches `render_container_item`. The record should say that. As written, a reader who trusts the checked box is pointed at the defect. This is the same class as round 3's finding 1, which was fixed by rewording 6.6; 4.2 and the design paragraph were left behind.

## Minor

**2. `docs/AUTHORING.md:646` omits `shape` from the packed-child key list.** It enumerates "container properties (`padding`, `stroke`, `background`, `rounded`, nested `flow`)". A packed child may carry `shape`, and it works: `into_placement` only refuses `at` and `to` on a packed child (`src/convert.rs:72-84`), `source_of`'s `Extent::Size` branch does not touch the anchor, and `render_container_item` receives the flow's placed box. One word.

## Verified as correct

The single-box collapse matches Typst 0.15.1 exactly: `layout_box` clips the body first and prepends fill and stroke outside that group (`typst-layout-0.15.1/src/inline/box.rs:65-72`), so a container's own paint survives its own clip, and `clip_rect` halves each thickness and builds the curve from the corners' inner control points (`shapes.rs:616-658`), so child ink is cut half a thickness in. Both are what the geometry and paint-coverage requirements state. No `#rect` remains anywhere in `src/`. `fixed_by_template` is set in `source_of` alone and matches the delta's spelling table row for row (`src/resolver.rs:106-165`, test at `883-957`). The load check cannot panic on `source_of`'s `expect`: `validate_references` runs first (`src/templates.rs:1095`) and recurses into container sizes (`1588-1600`), and `load_geometry_values` inserts an entry per declared param (`1609-1623`); the `Extent::To` panic is unreachable because packed children cannot carry `to`. `resolve` ignores `cap` for an `Author` extent (`src/resolver.rs:189`), so load and render agree exactly on a fixed box. A flow child trimmed by `FlowOverflow::Trim` has no rect and is dropped by the `zip` at `src/render/mod.rs:1695-1697`, so it has no resolved box to check, which is consistent with the delta rather than a gap in it. `avery5163_asset_tag.yaml:43-50` remains the only pre-existing stroked container and declares `items: []`, and no template outside the new fixtures sets `rounded`, so "nothing renders differently" holds for this repository.

VERDICT: REVISE
