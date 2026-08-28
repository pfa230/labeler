# Diff review: issue-263-packed-children (round 2)

Gates re-run here: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features` clean, `cargo test` exit 0. `.workflow/review-gate-check.sh --plan-only` passes and `specs-digest.sh` still equals `review.md`'s `SPECS_SHA256`. [verified]

Round 1's nine findings are genuinely addressed, not papered over: the measurement-path index now maps through `active_children[act_idx]` (`src/render/mod.rs:1401-1405`) with a regression test (`flow_overflow_in_measurement_with_gated_sibling_names_correct_child_index`); `render_items` and the intrinsic arm both call `resolve_packed` rather than inlining `source_of` + `resolve` (`:1477`, `:1374`); the row child's `y` is no longer `.max(0.0)`-clamped; `flow:` with a null value is refused via `deserialize_present_typed` (`src/raw.rs:247`) with tests for both `flow:` and `flow: null`; the duplicate packed-`line` refusal is gone; ADR-0083 no longer claims auto-wrapping; the headline spec scenarios now have real assertions, including the strongest one in the change — `headline_flow_spec_scenarios_automated_assertions` asserts the rotated flow container renders byte-identically to a hand-placed absolute equivalent.

## Blocking

**1. A trailing zero-extent packed child is placed one `gap` past the last occupying child and bounds-checked there, so every `content`-sized flow container with an empty trailing child fails to render.**

`src/resolver.rs:588-596`: for an active, non-occupying child the lead is `cursor + flow.gap` whenever any occupying child preceded it, and that speculative position is immediately checked with `fits_frame(primary_axis, lead, 0.0, inner_primary)`. When no occupying child follows, no gap will ever be laid down, and the check rejects a position the arrangement invented.

Failure: a `row` flow container with `size: [content, h]`, `gap: 4`, children `AAA` (20 wide), `CCC` (20 wide), then a `content`-width text whose value is empty for this request. The assembled extent is `20 + 4 + 20 = 44`, so the container resolves to a padded inner width of exactly 44 and `cursor` after `CCC` is exactly 44. The trailing child gets `lead = 48`; `fits_frame(0, 48.0, 0.0, 44.0)` returns `AnchorBeyondFrame` → `item_out_of_frame`, and the whole label fails.

This is not a narrow edge case: a `content`-sized flow container *always* ends with `cursor == inner_primary`, so the bug fires for every trailing empty child under any non-zero `gap`. A fixed-size container whose children plus gaps exactly fill it fails the same way. The existing test only puts the empty child in the middle (`tests/acceptance_issue_263.rs`, "An empty value leaves no double gap"), where `lead = 24 < 44` and the check passes, so the suite is green over it.

It contradicts two clauses of the delta directly: `specs/flow-layout/spec.md` — "a container SHALL carry no leading or trailing gap", and "It advances nothing and consumes no `gap`, because a `gap` is the space between two adjacent children and a child with no extent along that axis separates nothing from nothing." Placing it a gap past the trailing edge *is* a trailing gap, and bounds-checking that gap turns "occupies nothing" into a render failure. The spec's "at the leading edge the next occupying child would take" is undefined when there is no next occupying child; the implementation guesses, and then enforces the guess.

## Non-blocking

**2. `render_items` now resolves every non-flow item's placement box before rendering any item, changing which error an already-failing existing template returns.** `src/render/mod.rs:1511-1534` builds `placed_boxes` for the whole active list, and the render loop at `:1536` only then resolves content. Previously each arm called `resolve_placement_box` inside its own iteration, so item 0's content error surfaced before item 1's placement error. Failure: a fixed-width label whose item 0 is a `text` bound to `{missing}` and whose item 1 has `size: ["{w}", 10]` with a request supplying `w: 500` returned `MissingField`; it now returns `UnsupportedLayoutItem` / `item_out_of_frame`. Same 422, different `code`, `reason` and `details`. Only the flow branch needs a pre-pass; keeping the `None` branch lazy costs nothing and preserves `proposal.md`'s Goal "no response that is served today changes".

**3. `src/templates.rs:1567-1581` re-implements `precheck`'s anchor-free authored-extent loop instead of calling `precheck`.** With `frame: None`, `precheck` (`src/resolver.rs:355-376`) guards *every* anchor resolution behind `if let Some(frame)`, so it is already safe for `Anchor::Absent` and does exactly what the inline copy does. This is a second copy of a resolver rule in the validation layer — the precise duplication ADR-0080 and #150/#155 exist to prevent, and `design.md` decision 1a's "One implementation, no second copy". No live divergence today; it is one edit away from one.

**4. `FlowChildInput.active` is never `false` in production.** Both call sites hardcode `active: true` (`src/render/mod.rs:1393`, `:1487`) because `is_item_active` filters upstream. `arrange_flow` (`src/resolver.rs:544-547`) silently emits no rect for an inactive entry, so `rects` becomes shorter than `children` and the caller's `active_items[act_idx]` mapping only holds because the field is always true. The resolver unit test `flow_row_arrangement_with_gaps_and_zero_extent` exercises the `active: false` branch — a path the service never takes. Either drop the field (the filter is the caller's job) or make the contract explicit.

**5. `src/resolver.rs:23-25`: `Anchor::is_absent` is added and never called anywhere in `src/`.** Dead public API.

**6. `src/resolver.rs:122-125`: `source_of` gained a `panic!` on an `Extent::To` with no anchor.** Unreachable only because `convert.rs:32-37` refuses `to` on a packed child; unlike `Anchor::resolve`'s panic, which task 1.1 asked for deliberately, this one is incidental and sits on the request path.

**7. Two delta scenarios have no automated assertion.** `specs/flow-layout/spec.md` — "a `text` child whose value interpolates a field the request does not supply still fails with `MissingField` whether its extent resolved to zero or not": `tests/acceptance_issue_263.rs` case 3 asserts only the `item_out_of_frame` half. And "the same two containers spelling `size: [content, content]` pack side by side and render": only the `[fill, fill]` collision half is tested. Tasks 5.2 and 5.4 are checked over both.

**8. Scope: the `.gitignore` entries (`.agy-*`, `.agent-*`) are a fix for round-1 finding 5 and are justified, but the hunk ends with a trailing blank line.** Trivial.

VERDICT: REVISE
[?25h