1. **MAJOR — The error mapping conflates load-time, direct-render, and batch failures.** The delta says fixed non-square circles fail validation and are quarantined (`specs/shape-paint/spec.md:75-77`), but its mapping then assigns every non-square circle `422 UnsupportedLayoutItem` and requires that top-level response shape (`specs/shape-paint/spec.md:89-101`). Template validation instead retains the existing `TemplateInvalid` behavior, while batch rendering must return top-level `BatchInvalid` with nested per-label failures (`docs/SPEC.md:129-132`, `docs/SPEC.md:199-203`). The proposal itself describes the new reason as render-time only (`proposal.md:82-83`).

2. **MAJOR — The unconditional render check contradicts conditional visibility.** The delta requires every circle to be checked at render “with no exception” (`specs/shape-paint/spec.md:70-80`), and the design repeats that requirement (`design.md:125-128`). Frozen §5 excludes an inactive item and its children from both measurement and rendering (`docs/SPEC.md:560-565`). A dynamically non-square circle behind a false `when` therefore has no rendered box to validate and must not fail that request.

3. **MAJOR — The rounded-stroke scenario claims two geometrically different boundaries are the same curve.** The delta says a stroked rectangular container clips children at the stroke’s inner edge, half the thickness inside the box (`specs/shape-paint/spec.md:22-28`), and the design confirms Typst constructs that clip from the inner control points (`design.md:34-40`). Nevertheless, the radius requirement says the authored radius bounds the clip unchanged and that a container with `stroke.thickness: 0.3` clips a child on the “same 2.0-unit curve” as its paint (`specs/shape-paint/spec.md:274-280`, `specs/shape-paint/spec.md:294-299`). The inner edge of a centred stroke is not the same curve as the painted boundary.

4. **MAJOR — The emitter-quantum justification is mathematically false.** The contract says any dimensional difference at or below `0.0001` cannot be drawn and therefore paints as a circle (`specs/shape-paint/spec.md:45-53`; `design.md:176-180`). Independently formatting two values to four decimals does not guarantee equal output: `1.00004` and `1.00006` differ by only `0.00002` but format to `1.0000` and `1.0001`. The chosen epsilon remains unambiguous, but the plan cannot claim it guarantees identical emitted dimensions.

### Required changes

The author must apply all changes below; no further review follows.

1. Scope `circle_box_not_square` explicitly to render-time failure. State separately that:

   - load-time non-square fixed circles follow existing template-validation/quarantine behavior;
   - direct render failures return `422 UnsupportedLayoutItem` with `circle_box_not_square`;
   - batch failures return `422 BatchInvalid`, with each affected failure entry carrying code `UnsupportedLayoutItem`, reason `circle_box_not_square`, and the path-bearing message, while producing no artifact or print job.

2. Replace “every circle” and “no exception” with “every active circle that reaches measurement/rendering.” State that a false `when` excludes request-dependent squareness resolution and cannot cause a render failure, while ordinary load-time structural validation remains unaffected. Add a scenario where a parameter-sized circle resolves non-square but is gated off and the request succeeds without rendering it.

3. Rewrite the radius/clip language to distinguish the authored painted boundary from the clip at a centred stroke’s inner edge. With no stroke, the rounded clip and painted boundary coincide; with a stroke, children follow the corresponding inner corner curve. Change the combined `rounded: 2.0` plus `stroke` scenario so it no longer calls that inner curve the same 2.0-unit curve.

4. Retain the chosen `abs(width - height) <= BOUNDS_EPSILON` rule, but remove claims that four-decimal emission guarantees equal formatted dimensions. State instead that this is the service’s deliberate bounds-aligned tolerance and may accept independently formatted dimensions one emitter quantum apart.

VERDICT: APPROVE_WITH_CHANGES
