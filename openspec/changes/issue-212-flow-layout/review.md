## Review Metadata

- **Round**: 5
- **Prior round**: REVISE (3 Critical, 1 Moderate)

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/, design.md; AGENTS.md; openspec/specs/layout-sizing/spec.md; docs/adr/0080-unify-size-resolution.md, 0081-size-vocabulary-content-and-fill.md, 0082-text-overflow-policy.md, docs/adr/README.md; src/resolver.rs, src/models.rs, src/raw.rs, src/convert.rs, src/templates.rs, src/render/mod.rs
- **Issue**: #212

## Findings

### Critical (blocking)

1. **The arrangement still collapses `fill`'s upward claim and downward box into one extent, contradicting the governing sizing protocol.** The governing delta itself says a frame extent reports `min(intrinsic, max_*, available)` upward while taking the available extent downward (`specs/layout-sizing/spec.md:42-44`); the implementation likewise gives `Frame` different results in `resolve` and `claim` (`src/resolver.rs:165-169`, `183-191`). Nevertheless, flow packing says it advances by the drawn box and “There is no second number” (`specs/flow-layout/spec.md:177-178`), defines container intrinsic size from those child extents (`:196-204`), and designs the packer around “resolved extents” (`design.md:61-65`) while later claiming it takes claims (`design.md:203-204`). Those are observably different for `fill`. A content-sized flow container containing a `fill` child must assemble the child’s upward claim to determine its intrinsic size, then resolve the child’s final downward box against that size; assembling the provisional resolved box instead makes the container consume its entire provisional frame. The plan must explicitly preserve both quantities and define which is used for intrinsic aggregation, final positioning, wrapping, slot occupancy, and overflow.

2. **`trim` and content-sized flow containers form an unresolved sizing cycle, despite the design claiming no staging.** The container’s intrinsic size is its assembled extent (`specs/flow-layout/spec.md:194-209`), but a trimmed child is excluded from that extent (`:387-388`) and whether it is trimmed depends on the container’s resolved inner box (`:345-359`). For example, an unwrapped content-width container capped at 20 with two 12-unit children and gap 2 first assembles to 26, resolves to 20, trims the second child, and then—under the stated contract—has an assembled intrinsic width of 12 rather than 26. The artifacts specify neither that evaluation order nor whether packing is rerun against 12. The existing pipeline measures a container’s intrinsic before resolving its final placement (`src/render/mod.rs:1030-1104`, `1323-1366`), then resolves the final box only during rendering (`:1460-1486`, `1493-1509`). Contrary to that evidence, `design.md:185-192` says no staging or repetition is needed, and its first sentence still says “Refusing `fill` removes the staging problem.” A deterministic staged or fixed-point contract is required, including caps, wrapped cross-axis overflow, nested flow containers, and what intrinsic extent a trimmed container reports.

3. **The load-time arrangement carve-out contradicts an unmodified governing requirement, so the four MODIFIED blocks are not the complete required set.** The governing capability promises that load’s available-extent substitution is an upper bound such that “nothing accepted at load can overflow at render for want of a measurement” (`openspec/specs/layout-sizing/spec.md:836-839`). The new capability deliberately does not arrange at load because measured extents are unavailable (`specs/flow-layout/spec.md:314-329`), then allows those measured extents to accumulate into render-time `item_out_of_frame` (`:345-359`). That is precisely a template accepted at load and overflowing at render because load lacked measurements. Calling this “the load/render division `layout-sizing` already draws” (`design.md:71-81`) does not resolve the contradiction. The complete governing “Load-time validation and render-time resolution are one algorithm” requirement must be MODIFIED, or the flow contract must preserve its existing guarantee.

4. **The no-anchor packed-child model is not reconciled with layout-sizing’s universal available/claim/requirement contract.** A packed child is required to have no `at` (`specs/flow-layout/spec.md:82-85`), and `Placement.at` is planned to become optional (`design.md:165-179`). But the modified governing requirement still defines available extent universally as `frame − resolve(at) − inset` (`specs/layout-sizing/spec.md:20-25`), while the canonical requirement not included in the delta derives every item requirement from its coordinates and enumerates the six placement spellings plus `line` (`openspec/specs/layout-sizing/spec.md:320-372`). Saying the formula “degenerates” (`specs/flow-layout/spec.md:108-114`) does not define what `resolve(at)`, `source_of`, `claim`, or `requirement` do when no anchor exists; current `source_of` directly reads `placement.at` (`src/resolver.rs:75-87`). This is also where the missing distinction in Critical 1 must live. The plan must either define a sizing-only implicit zero anchor and its serialization boundary, or modify the governing requirement blocks to specify anchorless packed children and how their claims contribute.

### Moderate

1. **The `fill` whole-line rule ignores caps and the selected axis.** The flow spec permits `max_w`/`max_h` (`specs/flow-layout/spec.md:104-106`) and says the ordinary cap rules apply (`:108-114`), while the governing requirement says caps bind frame extents (`specs/layout-sizing/spec.md:29-34`). Yet the proposal and spec state without qualification that a `fill` child occupies a whole line (`proposal.md:32-36`; `specs/flow-layout/spec.md:95-99`, `257-261`, `347-350`). A row child with `size: [fill, 4], max_w: 10` in a 30-unit inner width is 10 wide, not a whole line; a `fill` only on the cross axis likewise says nothing about main-axis occupancy. Qualify the rule as an uncapped main-axis `fill`, and cover capped and cross-axis cases in scenarios and acceptance evidence.

2. **A request-resolved authored extent can exceed the whole line, contradicting the “no single child can overflow” premise.** The artifacts correctly say authored extents are checked rather than clamped (`specs/layout-sizing/spec.md:29-31`; `specs/flow-layout/spec.md:108-112`), but later assert every packed child is clamped and no single child can overflow (`specs/flow-layout/spec.md:257-260`, `347-350`; `design.md:117-121`). A parameter-authored extent can fit at its load-time default and exceed the inner box for a request, causing the child’s ordinary placement check to fail before flow overflow can trim it. State that precedence and add the requested single-child-larger-than-line scenario.

3. **Zero-main-axis children have no defined drawing or cross-axis contribution.** The arrangement places only active children with main extent greater than zero and calls those the children occupying slots (`specs/flow-layout/spec.md:164-178`). It never says whether an active zero-main child is still drawn at the cursor, omitted, or evaluated but unplaced. Nor does it say whether its positive cross extent contributes to the line extent, which is currently defined only from children occupying that line (`:196-201`). This matters for a zero-width container with a visible frame or any item with zero main extent and positive cross extent; the empty-text scenario (`:188-192`) tests only gap suppression. Define placement, drawing, cross-axis aggregation, and error behavior for such children.

4. **The proposal’s capability inventory is stale.** It says `layout-sizing` modifies three requirements and lists three (`proposal.md:72-79`), while the delta contains four blocks, including “An extent comes from the author, from the content, or from the frame” (`specs/layout-sizing/spec.md:3-18`). Independently, the exact block diffs show that all four copied source blocks retain their prior contents apart from the visible intended edits; the defect is the incomplete inventory and missing governing blocks identified above, not silent truncation inside the four present blocks.

### Suggestions

- The resolved-axis restriction itself is coherent: `container_inner_axes_resolved` swaps the state for 90/270-degree rotation (`src/resolver.rs:225-252`), and the row, column, and rotated scenarios select the corresponding author-space main axis.
- The plan includes the required ADR-0083 and index-row work (`proposal.md:55`, `101-102`) and a render-and-inspect acceptance loop (`design.md:220-238`).
- Nested flow, a flow container at a sheet slot root, and authored-size versus content-size trim evaluation are explicitly covered. These checks do not resolve the blocking claim, staging, and load-contract defects above.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: REVISE

## Required Changes (APPROVE_WITH_CHANGES only)

CHANGES_APPLIED: no

## Rebuttals

None.
