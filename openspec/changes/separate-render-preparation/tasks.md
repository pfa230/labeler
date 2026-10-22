# Tasks

## 1. Prepare, then emit

- [ ] 1.1 In `src/render/mod.rs`, add the prepared node from design.md and `prepare_items`, which replaces the box resolution and child walk in `render_items`, `render_flow_child` and `render_single_item`: flow arrangement and trim marker, `resolve_placement_box`, `check_stock`, `resolve_point`/`check_line`, font weight, QR payload and SVG, image source and bytes, container frames and child context. Verify by comparing each moved call with `render_items`, `render_flow_child` and `render_single_item` on `origin/main` that it runs in the same order relative to the others, with the same path and arguments.
- [ ] 1.2 Rewrite `render_items` as `prepare_items` followed by emission, and change `render_text_item`, `render_qr_item`, `render_image_item`, `render_line_item` and `render_container_item` to draw from a prepared node; image registration with `ImageCollector` stays in emission. Verify by reading the emission functions that none of them resolves interpolation, places, arranges, fits text or checks a box, and that their only fallible calls are `format_length`, `format_length_up`, `derived_leading_pt` and `writeln!`; and verify `render_items` keeps its signature, so `git diff origin/main -- src/templates.rs` is empty and no test code changes.

## 2. Verification

- [ ] 2.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test`, keeping the output in `.agent-runs/`. Verify `git diff origin/main --stat` lists only `src/render/mod.rs` and the change folder, and that the diff of `src/render/mod.rs` touches no line under `mod tests`, so every existing expectation, including the error-order, flow overflow and trim-marker tests, passed unedited.
- [ ] 2.2 Run `npx --no-install openspec validate --all --strict --no-interactive` and verify it passes with this change's `skip_specs: true`.
