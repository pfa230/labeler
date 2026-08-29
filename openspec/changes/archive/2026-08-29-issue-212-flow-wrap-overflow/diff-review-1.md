# Code Review: `issue-212-flow-wrap-overflow`

## 1. Review Scope & Artifacts Evaluated
The implementation diff on branch `issue-212-flow-wrap-overflow` was audited adversarially against:
- **Proposal & Design:** [`proposal.md`](file:///home/pfa/projects/labeler/.worktrees/issue-212/openspec/changes/issue-212-flow-wrap-overflow/proposal.md), [`design.md`](file:///home/pfa/projects/labeler/.worktrees/issue-212/openspec/changes/issue-212-flow-wrap-overflow/design.md)
- **Specification Delta:** [`openspec/changes/issue-212-flow-wrap-overflow/specs/flow-layout/spec.md`](file:///home/pfa/projects/labeler/.worktrees/issue-212/openspec/changes/issue-212-flow-wrap-overflow/specs/flow-layout/spec.md)
- **Tasks & Plan:** [`tasks.md`](file:///home/pfa/projects/labeler/.worktrees/issue-212/openspec/changes/issue-212-flow-wrap-overflow/tasks.md)
- **Repository Standards & Architecture:** [`AGENTS.md`](file:///home/pfa/projects/labeler/.worktrees/issue-212/AGENTS.md), ADR-0083, and new [`docs/adr/0089-wrapping-and-the-overflow-policy.md`](file:///home/pfa/projects/labeler/.worktrees/issue-212/docs/adr/0089-wrapping-and-the-overflow-policy.md)
- **Implementation Diff:**
  - Schema & Models: [`src/raw.rs`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/raw.rs#L228-L241), [`src/models.rs`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/models.rs#L602-L660), [`src/openapi.rs`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/openapi.rs#L15-L25), [`src/convert.rs`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/convert.rs#L142-L170)
  - Validation & Traversal: [`src/templates.rs`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/templates.rs#L1983-L1999)
  - Layout & Sizing Resolver: [`src/resolver.rs`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/resolver.rs#L530-L702)
  - Integration Tests & Docs: [`src/lib.rs`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/lib.rs#L8830-L9042), [`docs/AUTHORING.md`](file:///home/pfa/projects/labeler/.worktrees/issue-212/docs/AUTHORING.md#L583-L649), [`docs/adr/README.md`](file:///home/pfa/projects/labeler/.worktrees/issue-212/docs/adr/README.md#L92-L98)

---

## 2. Detailed Verification & File:Line Evidence

### A. Two-Stage Parsing & Schema Consistency
- **`deny_unknown_fields` and Wire Structs:**
  In [`src/raw.rs:228-241`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/raw.rs#L228-L241), `FlowRaw` adds `wrap: bool`, `line_gap: Option<f32>`, and `overflow: Option<FlowOverflow>` under `#[serde(deny_unknown_fields)]`.
- **Domain Model & OpenAPI:**
  In [`src/models.rs:602-660`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/models.rs#L602-L660), `FlowOverflow` implements `Fail` (default) and `Trim`, along with `FlowOverflow::Invalid` as an internal parse sentinel that is marked with `#[serde(skip)]` so that it is never published to OpenAPI schema or serialized. `FlowOverflow` is registered in [`src/openapi.rs:118-125`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/openapi.rs#L118-L125).
- **Validation & JSON Paths:**
  [`src/convert.rs:145-163`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/convert.rs#L145-L163) validates local block invariants (`line_gap` finite and $\ge 0$, `overflow` matching `fail` or `trim`), attaching JSON path `flow.line_gap` and `flow.overflow`. This is verified by tests in [`src/templates.rs:4953-4987`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/templates.rs#L4953-L4987).

### B. Derived Frame Restrictions (Load-Time Quarantine)
- **Primary Axis Resolution for `wrap: true`:**
  In [`src/templates.rs:1986-1993`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/templates.rs#L1986-L1993), `flow.wrap` checks `child_axes_resolved[primary_axis]` (where `primary_axis` is 0 for `Row` and 1 for `Column`). An unresolved primary axis immediately errors with a message naming `wrap`.
- **Dual Axis Resolution for `overflow: trim`:**
  In [`src/templates.rs:1994-1998`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/templates.rs#L1994-L1998), `FlowOverflow::Trim` checks `child_axes_resolved.iter().any(|resolved| !resolved)`. If either axis is unresolved, it immediately errors with a message naming `overflow`.
- **Author Space & Rotation Swaps:**
  `child_axes_resolved` in [`src/templates.rs:1980-1985`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/templates.rs#L1980-L1985) is computed via [`resolver::container_inner_axes_resolved`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/resolver.rs#L249-L276), which swaps axes for 90° and 270° quarter turns. This ensures `wrap` and `trim` test the correct physical axis matching the author-space direction. Verified by comprehensive test matrix in [`src/templates.rs:5077-5161`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/templates.rs#L5077-L5161).

### C. Arrangement & Geometry Mechanics (`arrange_flow`)
- **Box vs Requirement Separation:**
  - Line breaking in [`src/resolver.rs:553-562`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/resolver.rs#L553-L562) tests `line_box_primary + pending_gap + ext_p > inner_primary + BOUNDS_EPSILON`, using the child's `resolved_box` extent `ext_p`.
  - Next line vertical/horizontal lead in [`src/resolver.rs:694`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/resolver.rs#L694) advances by `line_box_secondary + flow.line_gap`, using the maximum secondary *box* of drawn children on the line.
  - Container assembled extent in [`src/resolver.rs:680-690`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/resolver.rs#L680-L690) accumulates `line_requirement_secondary` (max requirement per line) and `line_requirement_primary` (sum of requirements of occupying children plus gaps). Verified in [`src/resolver.rs:1296-1324`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/resolver.rs#L1296-L1324).
- **Line Membership for Zero-Primary-Extent Items:**
  In [`src/resolver.rs:543-574`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/resolver.rs#L543-L574), zero-primary items (`ext_p == 0.0`) never trigger a line break and never join a subsequent line created by a later child. In [`src/resolver.rs:634-638`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/resolver.rs#L634-L638), zero-primary items are placed at `cursor + flow.gap` when occupying siblings follow on the same line, or at `cursor` when at the line end. Verified in [`src/resolver.rs:1326-1364`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/resolver.rs#L1326-L1364).
- **Check 1 vs Check 2 Ordering:**
  [`resolver::resolve_packed`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/resolver.rs#L461-L513) checks each child's own box against the inner frame (`fits_frame`) before `arrange_flow` is called. Thus, a single child exceeding the container dimensions fails unconditionally with `item_out_of_frame` before the `overflow` policy is evaluated in Check 2.
- **`overflow: trim` Policy:**
  In [`src/resolver.rs:644-652`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/resolver.rs#L644-L652), if Check 2 detects a frame violation and `overflow` is `Trim`, `trim_here = true; break;` stops layout on the current line and immediately terminates `'lines`. The failing child and all subsequent items are omitted from `rects` and excluded from `assembled` calculation.
- **Rendering & Evaluation Semantics:**
  In [`src/render/mod.rs:1642-1678`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/render/mod.rs#L1642-L1678), all active children are evaluated and measured into `measured` before `arrange_flow`. Drawing iterates `flow_res.rects.into_iter().zip(measured.iter().zip(&active_items))`, meaning trimmed items are evaluated for template errors (such as `MissingField` on dynamic text) but skipped during Typst generation. Verified at HTTP level in [`src/lib.rs:8954-9041`](file:///home/pfa/projects/labeler/.worktrees/issue-212/src/lib.rs#L8954-L9041).

### D. Documentation, ADRs & Workflow
- **ADR-0089 & Index:** [`docs/adr/0089-wrapping-and-the-overflow-policy.md`](file:///home/pfa/projects/labeler/.worktrees/issue-212/docs/adr/0089-wrapping-and-the-overflow-policy.md) correctly details the amendments to ADR-0083. [`docs/adr/README.md:92,98`](file:///home/pfa/projects/labeler/.worktrees/issue-212/docs/adr/README.md#L92-L98) updates the index table accordingly.
- **Authoring Guide:** [`docs/AUTHORING.md:583-649`](file:///home/pfa/projects/labeler/.worktrees/issue-212/docs/AUTHORING.md#L583-L649) provides the worked example of wrapped rows with `line_gap`.
- **Workflow Gates:** `.workflow/review-gate-check.sh`, `.workflow/archive-merge-check.sh`, `.workflow/gate-tests.sh`, `.workflow/apply-tests.sh`, `cargo fmt --check`, `cargo clippy --all-targets --all-features`, and `cargo test` all pass cleanly with 0 warnings/failures.

---

## 3. Findings Summary
No blocking defects, specification deviations, or design inconsistencies were found. The implementation strictly adheres to the OpenSpec requirements and architectural guidelines in `AGENTS.md`.

VERDICT: APPROVE

