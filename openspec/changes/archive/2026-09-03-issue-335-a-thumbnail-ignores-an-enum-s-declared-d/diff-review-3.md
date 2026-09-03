TREE_SHA256: eff360bb3ad12613881c33f87ba4a41d826a6bfada4556be667dc5baeb57cab9

Reviewed the diff against `proposal.md`, `design.md`, both spec deltas, `tasks.md`, and `AGENTS.md`, plus the two prior diff-review rounds.

## What I verified independently

Gates are green in this worktree: `cargo fmt --check` exit 0, `cargo clippy --all-targets --all-features` clean, `cargo test` 815 passed / 0 failed, `openspec validate --all --strict` 24/24. [verified]

`SPECS_SHA256` recomputes to `f95902798420…`, matching `review.md:23`, so `specs/` is untouched since the approving plan verdict. [verified]

The core mechanism is right. The new `Select` arm (`src/templates.rs:183-194`) sits inside the `interpolated && required` guard at `src/templates.rs:163`; `required` is true for "no default" or "broken default", and `default_error` separates them (`src/templates.rs:415-421`), so `if input.default_error.is_some() { continue; }` is exactly D1's condition. [verified] Both `panic!`s are unreachable: `InputControl::Select` is produced only for `ParamType::Enum` (`src/templates.rs:391`), `values` is `Some` for exactly that type (`src/templates.rs:423-427`), and an empty enum is refused at load (`src/templates.rs:1280-1283`, quarantining at `:820`) and at create (`src/api.rs:644`). An `image_bound` enum takes `InputControl::Image` (`src/templates.rs:387`) and never reaches the arm. [verified]

The `option` deletion is complete and production-safe: the only remaining callers pass `None` (`src/api.rs:1254,2677,2681`), no request model carries an option field, and `normalize_option` / `RenderContext.selected_option` are untouched per D2 (`src/render/mod.rs:665,1247,1326`). [verified] `catalog/` holds five YAML files and none declares an `enum`. [verified]

Round 2's three findings are genuinely closed: the colour/dimension `{ref}` breaking class is now listed in `proposal.md:22-31`, described in `design.md:177-184`, and pinned by `thumbnail_enum_colour_ref_without_default_fails` (`src/templates.rs:6718-6774`), which fails pre-change because the deleted option map supplied `palette`; the avery test's shadowed resolver is gone (`src/render/mod.rs:7420-7461`).

I checked each new test against the base behavior. Seven of the nine fail pre-change, including both byte-comparison tests and all three `unwrap_err()` tests. One does not.

## Findings

**1. BLOCKING — `thumbnail_enum_gate_without_default_is_absent_via_http` cannot fail** (`src/lib.rs:1457-1502`). Its three assertions are `status == 200` (`:1497`), `content-type == image/png` (`:1498`) and PNG magic (`:1500`). Against the base commit the handler built `{outline: "yes"}` from `default_option_selection`, `normalize_option` accepted it against `values: [yes]`, the merge inserted it, the gate matched and the container drew — and the response was still `200` with a PNG. Post-change the container is absent and the response is still `200` with a PNG. Every assertion holds identically on both sides, so the test passes against the code it is named for catching. The comment at `:1494-1496` states the behavior ("succeeds as an empty label") that nothing asserts.

This test exists to answer round 2's finding 2, which asked for a direct HTTP-level pin on `src/api.rs:1254`'s `None` for the gate-drop case, and it does not provide one. The two unit-level tests it was meant to back up have the same property for a different reason: `thumbnail_enum_only_gate_without_default_is_absent` (`src/templates.rs:6544-6588`) and `thumbnail_enum_only_gate_with_default_is_present` (`src/templates.rs:6592-6636`) both call `resolve_parameters(…)` and `render_thumbnail_png(…, None, …)` themselves, so they assert what the library does when handed no option map, not that the handler hands it none — they pass pre-change too. The only thing pinning the handler is the byte comparison in `thumbnail_enum_with_default_shows_declared_default_via_http`, which covers the printed-enum case and not the gate case.

The fix is the technique the change already uses two tests earlier: write a second template with the container's `when:` removed (or with no items) alongside `enum_gate.yaml`, fetch both, and assert the bodies are equal — that assertion fails pre-change, where the gated container draws. Renaming the test to what it checks is the other honest option; leaving it named for an assertion it does not make is the one to reject.

**2. `design.md:172-175` and `proposal.md:69-71` name one affected fixture; there are two.** Both say `tests/fixtures/templates/avery5163_asset_tag.yaml` is "the one fixture" declaring an `enum`. `tests/fixtures/templates/container_circle_gated.yaml:11-14` declares `enabled: { type: enum, values: ["yes", "no"], default: "no" }` and gates its circle container on `when: { enabled: "yes" }`. Pre-change `default_option_selection` forced `enabled: "yes"` ahead of the declared default, so the circle drew in that fixture's thumbnail; post-change the declared `"no"` wins and it does not. [verified by code reading: `enabled` is a gate key only, so `interpolated` is false, `placeholder_data` never fills it, and `resolve_parameters` applies `"no"`.]

That is the change working as intended, so it is not a behavior defect — but the sentence is an exhaustive claim used to bound blast radius, and it is wrong, which is the same class round 2 blocked on. Two concrete consequences follow from it: `every_template_renders` (`src/render/mod.rs:5500-5510`) silently stops exercising the stroked circle container it used to render, and the fixture the archived #287 review discusses now behaves differently in a way no artifact records. Correcting both files costs nothing — they are context, not contract, and do not touch `SPECS_SHA256`.

**3. Minor — the render-dump harness lost the avery `outline` branch.** `dump_all_template_renders` (`src/render/mod.rs:5977-5998`) now starts each variant from `test_placeholder_data`, which omits the undefaulted `outline` (`tests/fixtures/templates/avery5163_asset_tag.yaml:20,45`). The deleted comment explicitly kept `outline: yes` in effect so the harness dumped the outlined variant. Task 3.2 only asked to vary `orientation` through `data`, so this follows the task, but the harness is an engine-upgrade visual baseline and it just stopped covering that container. Inserting `outline: "yes"` into `base_data` would restore it in one line; if the intent is that the harness show exactly what thumbnails show, say so where the comment used to be.

VERDICT: REVISE
