# Tasks

## 1. Extract the flow line

- [x] 1.1 In `src/resolver.rs`, add `FlowLine` with `lead`, `wraps_before`, `add` and `remainder` as design.md describes, and rewrite `arrange_flow`'s membership and placement walks to use it in place of `line_box_primary`, `line_has_occupying` and `cursor`. Verify by comparing with `arrange_flow` on `origin/main` that the wrap test and every addition keep their operand order, that a zero-primary child's lead follows the same rule, and that `arrange_flow`'s signature, `FlowChildInput` and `FlowResult` are unchanged, so `git diff origin/main -- src/render/mod.rs` is empty.
- [x] 1.2 Add the resolver unit test from design.md for `remainder` (row, gap 2, inner 30: 30 empty, 18 after 10, 0 after a further 18). Verify it fails when `remainder` is temporarily changed to omit the pending gap and when the clamp is removed, then restore it.

## 2. Verification

- [x] 2.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test`, keeping the output in `.agent-runs/`. Verify `git diff origin/main --stat` lists only `src/resolver.rs` and the change folder, and that the diff of `src/resolver.rs` under `mod tests` only adds the new test, so every existing flow, wrap, epsilon, zero-primary, overflow and trim expectation passed unedited.
- [x] 2.2 Run `npx --no-install openspec validate --all --strict --no-interactive` and verify it passes with this change's `skip_specs: true`.
