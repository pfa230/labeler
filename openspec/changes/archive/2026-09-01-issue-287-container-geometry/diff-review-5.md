TREE_SHA256: c89f9cc156c0ce38b6263194b3e5077043d97a339cef60729368560bb8cc348d

I reviewed the full diff against `proposal.md`, `specs/shape-paint/spec.md`, `design.md`, `tasks.md`, `ANSWERS.md`, `review.md`, `diff-review-1..4.md` and `AGENTS.md`, ran the gates myself, and ran a mutation experiment to test the one claim three rounds have circled.

**Gates, run here** [verified]: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features` clean, `cargo test` green (751 lib + 3 integration passed, 0 failed, 2 ignored). `.workflow/review-gate-check.sh --plan-only` over the changed files exits 0. `specs-digest.sh` recomputes `87a5c970…`, matching `review.md:33`; `AUTHOR: claude` / `REVIEWER: codex` differ.

**Round 4's two findings are fixed** [verified]: `tasks.md:46-50` and `design.md:127-131` now describe the emission-walk placement and name `is_item_active` filtering as the mechanism; `docs/AUTHORING.md:646` lists `shape` among the packed-child keys.

**The dynamic-width test is load-bearing, proven by running it** [verified]. I copied the tree to a scratch dir, moved the check out of `render_container_item` and into `measure_items` (the round-2 placement), and ran the circle tests: `circle_dynamic_width_frame_sourced_extent_checked_at_final_frame` fails (`unwrap_err()` on an `Ok` PNG — `fill` resolves to 60 against the probe frame and looks square) and `circle_render_time_squareness_check` fails too; the load-time test still passes. This is the red-before-green evidence `ANSWERS.md` asked for, and it was previously only reasoned about.

## Blocking

**1. `tasks.md:82` is checked but one of its three assertions is not made anywhere.** Task 6.3 reads "…and that no artifact is produced and no print job dispatched." Both batch tests run download mode — `src/lib.rs:10447` (`"mode": "download"`) and `src/render/mod.rs:7768` (`crate::batch::BatchMode::Download`) — so no print job could be dispatched by either, and nothing asserts one was not. The behaviour is correct: `src/batch.rs:118-119` returns `batch_invalid` before any `PrintUnit` is constructed, so a print-mode batch dispatches nothing [verified]. This is a defect in the record, not in the code, and the change folder archives permanently under `openspec/changes/archive/`. The `create_fake_printer` harness that would exercise it (`src/lib.rs:5520`) lives in `mod http_tests`, not in `mod auth_http_tests` where the new test sits, so the cheap fix is to reword 6.3 to the assertions actually made; adding a print-mode case in `http_tests` is the other. Same class as round 4's finding 1 and round 3's finding 1.

## Minor

**2. The new HTTP test leaks its temp directory.** `src/lib.rs:10610-10620` hand-rolls `/tmp/labeler-quarantine-test-<pid>-<n>` with a function-local `static COUNTER` and never removes it. Every other temp-dir test in the crate cleans up (`src/lib.rs:1099, 1178, 1448`; `src/templates.rs` throughout). `mod auth_http_tests` has no temp-dir helper of its own, so hand-rolling is defensible; not deleting is not.

## Verified as correct

The single-box collapse matches Typst 0.15.1: `layout_box` defaults `inset` to zero and clips the body before `fill_and_stroke` prepends (`typst-layout-0.15.1/src/inline/box.rs:26,65-72`), and `Frame::clip` wraps existing content into a group (`typst-library-0.15.1/src/layout/frame.rs:356-358`) while `prepend_multiple` puts fill then stroke outside it (`shapes.rs:675-679`), so the paint order is background → stroke → children and the container's own paint survives its own clip, exactly as the two requirements state. `clip_rect` halves each thickness and builds from inner control points (`shapes.rs:626-658`), so the stroke and radius reversals are real. No `#rect` or `#circle` remains in `src/`.

`fixed_by_template` is set in `source_of` alone and matches the delta's table row for row, including the shrinking `to` (`layout-sizing`'s "author, conditionally" row) reading `false`. `resolve` ignores `cap` for an `Author` extent (`src/resolver.rs:189`) and `arrange_flow` places packed children at their own resolved extents without cross-axis stretch (`src/resolver.rs:663-677`), so a box fixed by the template resolves identically at load and at render, including under `max_w` and inside a flow — the delta's central claim. `validate_circle_containers` deliberately walks `self.layout` rather than `instantiated.layout` (`src/templates.rs:1163`), which is what keeps a `"{param}"` reference from being laundered into a literal by `instantiate_item_defaults` (`src/templates.rs:1722-1724`) and wrongly judged at load.

`avery5163_asset_tag.yaml:43-50` is still the only pre-existing stroked container and declares `items: []`; every other `stroke:` in the repository is on a `line`, and no template sets `rounded` — so "nothing renders differently" holds here [verified]. The load message carries no JSON path, which matches the surrounding convention (`src/templates.rs:1966` and every other `validate_layout_item` message), and `shape`'s `skip_serializing_if` and derive set match its neighbour `Overflow` (`src/models.rs:822`), so `GET /api/templates/{id}` is unchanged for existing templates and `proposal.md`'s Impact stays true. `ui/src/` models no layout items, so no UI change was owed.

VERDICT: REVISE
