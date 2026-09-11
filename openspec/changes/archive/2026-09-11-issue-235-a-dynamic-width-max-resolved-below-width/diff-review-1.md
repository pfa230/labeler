TREE_SHA256: a883b459e2f163b37de2861f832b415bebfe93248f64154757ab13f27d74859a
SPECS_SHA256: e951b0908685c67bcf20ca14fb2b86892baa3a4335c063b22f63d6c17900a4d6

# Diff review: issue-235-a-dynamic-width-max-resolved-below-width

## What I checked

Guard, constructor, reason slug and the five HTTP tests against the delta at `specs/layout-sizing/spec.md`, `design.md` decisions, `tasks.md`, the codex plan review's three required changes, and AGENTS.md.

Gates, run in this worktree [verified]: `cargo fmt --check` exit 0, `cargo clippy --all-targets --all-features` exit 0, `cargo test` 911 passed / 0 failed (including `errors::tests::spec_documents_every_reason_and_invents_none`).

Red state, task 1.7 [verified]: I copied the tree to `/tmp`, removed only the `if min_w > max_w` block, and ran the five new tests. All five fail with exactly what the tasks require: the four inverted cases with `500` (the `f32::clamp` panic at `core/src/num/f32.rs:1566` surfaces in the run), the precedence case with `422` (`src/lib.rs:17873`), the batch case with `500` against an expected `422`. The test-comment claims are earned.

## Contract conformance [verified]

- Guard placement matches the design's chosen option: `src/render/mod.rs:788-789` (both `check_dimension_limit`), guard at `:791-803`, `RenderContext::new` / `measure_items` at `:805-812`, clamp at `:813` untouched. Strict `>`, so `min == max` renders (test asserts `200`).
- Parameter naming: `min.as_ref()` / `max.as_ref()` matched on `DynamicValue::Ref` at `:792-799`, `None` for `Literal`, exactly the design's provenance-free shape. The unit is `&template.unit`.
- Constructor `src/errors.rs:296-315` wraps `invalid_request(Reason::WidthBoundsInverted, ...)`, so `code` is `InvalidRequest` by construction; message names `format.width.max <v> <unit>` and `format.width.min <v> <unit>` with `(parameter '<name>')` only for references, "is below" between them. No structured details beyond `reason`, as decided.
- `src/reason.rs:85` places the slug under the `// InvalidRequest` block after `LineSpacingParamInvalid`.
- Ordering pinned from both sides by tests: `max_width: 0` asserts `422 dimension_exceeds_limit` (a guard placed before the dimension check would answer `400`), and `max_width: 5, w: 0` asserts `400 width_bounds_inverted` while `max_width: 40, w: 0` asserts `422 size_invalid`.
- All three codex-required plan changes landed in the tests: supplied-overrides-default `200` (`http_render_dynamic_width_defaults`), precedence pair, and the batch assertion as status + JSON content type + parsed `BatchInvalid` envelope with exactly one failure at index 1.
- Every rendering path funnels through `compile_label_source` (`src/render/mod.rs:727, :878, :890`); no UI, OpenAPI or docs site enumerates reason slugs (`rg line_spacing_param_invalid` hits only `src/` and specs), so nothing else needs registering.
- Tests sit in `auth_http_tests` beside the `line_spacing_param_invalid` tests and use the same `// Task N.N` comment style that module already has (`src/lib.rs:17339`).

## Findings

None blocking. Two observations, neither requiring a code change before this lands:

1. **Equal-bounds scenario is only partly asserted.** The delta's "Equal bounds render" scenario says "the label is `10` units wide"; the test (`src/lib.rs:17651-17653`) asserts `200` only. `tasks.md` 1.2, which the plan review approved, asked for exactly `200`, so the implementer met the task. The width claim in the scenario is unverified by any test. [verified]

2. **The load-time follow-up is named but not filed.** `proposal.md` (Out of scope) and `design.md` (Provenance decision) both say refusing inverted literal or default bounds at load "needs its own issue" / "is filed as its own issue". `gh issue list --state all --search "inverted"` returns no such issue [verified]. AGENTS.md: "Work you won't do now becomes an issue, never a TODO in code or docs." The driver should file it before the merge; it is not a defect in this diff.

Substring checks like `msg.contains("5")` are weak as message assertions, but they are what the tasks prescribe and the `reason` field is the machine-readable contract, so I am not counting that against the change.

VERDICT: APPROVE
