# Tasks

## 1. Evaluate one candidate

- [ ] 1.1 In `src/render/mod.rs`, add the candidate argument struct and the candidate operation from design.md, moving the marker scope, `measure_item_instance`, `flow_child_input`, candidate `arrange_flow`, rect-count rejection and prefix rect copy out of `arrange_flow_children`. Verify by comparing with `arrange_flow_children` on `origin/main` that each moved call keeps its order, arguments, path and error mapping, and that `FlowArrangement`, `MarkerInstance`, `prepare_items` and `intrinsic` are unchanged.
- [ ] 1.2 Rewrite the selection in `arrange_flow_children` as a loop from the longest prefix to the shortest that returns the first accepted candidate and keeps today's `ItemOutOfFrame` error when none is accepted. Verify by reading that the loop holds no arrangement or measurement logic of its own and that `repeating_container_trim_marker_distinguishes_overflow_from_an_exact_fit` and `repeating_container_trim_marker_keeps_earlier_siblings_in_place` pass unedited.

## 2. Verification

- [ ] 2.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test`, keeping the output in `.agent-runs/`. Verify `git diff origin/main --stat` lists only `src/render/mod.rs` and the change folder, and that the diff of `src/render/mod.rs` touches no line under `mod tests`.
- [ ] 2.2 Run `npx --no-install openspec validate --all --strict --no-interactive` and verify it passes with this change's `skip_specs: true`.
