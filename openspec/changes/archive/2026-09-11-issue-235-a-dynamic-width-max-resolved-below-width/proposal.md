## Why

Implements GitHub issue **#235**: a dynamic `format.width.max` resolved below `format.width.min`
panics the render worker.

On a dynamic-width `single`, the label's width is the content requirement clamped into
`[width.min, width.max]`. Either bound may be a `"{param}"` reference, so a request can resolve `max`
below `min`, and the clamp then panics (`min > max`). Since #248 that unwind is a covered panic under
`panic-envelope`, so the caller gets `500 Internal`, the process survives, and the payload lands in
the log at error level. What remains wrong is the attribution: `Internal` means "not attributable to
the request", and this failure is exactly attributable to the request, by a value the caller sent.
`check_dimension_limit` already refuses an over-large or non-positive bound a few lines above the
clamp; an inverted pair is the one geometry failure on that path that still gets to a panic.

## What Changes

- A resolved `format.width.min` strictly greater than the resolved `format.width.max` refuses the
  render with `400 InvalidRequest` and a new `details.reason`, `width_bounds_inverted`. The message
  names both resolved values with the template unit and, for each bound that is a parameter
  reference, the parameter it resolved from. `min == max` is not inverted and renders at that width.
- The refusal reads the two resolved values after the existing dimension check and before anything
  is measured. So a bound that is non-positive, non-finite or over the `max_label_dimension_mm` cap
  keeps its existing answer, `422 UnsupportedLayoutItem` with `dimension_exceeds_limit`, and an
  inverted pair of otherwise-legal bounds is the only input that gets the new reason. No request
  reaches the clamp with `min > max`, on any path that renders a label: single render, batch, print,
  and thumbnail all share the one pre-pass.
- On the batch path a refused label is a failing label under `batch-validation`: the request is
  `422 BatchInvalid`, produces nothing, and the label's entry in `details.failures` carries
  `InvalidRequest` with reason `width_bounds_inverted`.
- The refusal cannot see where a value came from. It judges the values resolved for that render: a
  supplied value is used, and a declared `default` only when the request omits the parameter. A
  bound that resolved from a `default` meets it exactly as a supplied value does and is reported the
  same way, which is the split `text-line-spacing` already states for a pitch default and for the
  same reason.
- The guard runs before the items are measured, so it precedes every refusal measurement can raise.
  **This changes the error contract for one class of request:** a request whose bounds are inverted
  and whose items would also fail measurement (an authored item extent resolving to zero, say)
  answers `422 UnsupportedLayoutItem` today, because measurement runs before the clamp and the panic
  is never reached; after this change it answers `400 width_bounds_inverted`. Under the pre-`1.0`
  rule that a change altering behavior breaks what came before, that is the finished job: the
  label's bounds are judged before anything inside them, and no second ordering is kept.
- The reason is a new `AppError` constructor, so the `code` string is `InvalidRequest` by
  construction and the slug lives in one place.
- No request that renders today renders differently. Every input this change affects answers an
  error today: the inverted pair alone answers `500 Internal` and now answers `400`, and the inverted
  pair with a measurement failure answers that failure's `422` and now answers `400`, per the bullet
  above.

Out of scope, named so the boundary is explicit:

- **Enforcing a parameter's declared `min`/`max` on the render path** is #238, which owns it for
  every parameter. The two do not overlap: #238 would refuse `max_width: 5` because the parameter
  declares a bound; this change refuses it because the resolved geometry is impossible, which a
  template with no declared bounds can still reach.
- **A template whose two bounds are both literals, or whose instantiated defaults invert the pair,
  loads today** (`Template::validate` checks that both bounds are present, `src/templates.rs:1107-1118`,
  and nothing on the format checks their order; the `min <= max` check at `src/templates.rs:1939-1943`
  is for an item's `max_w`/`max_h`). A literal pair in the wrong order panics on every render today
  and will answer `400 width_bounds_inverted` on every render after this change; an inverted default
  does so on exactly the renders that omit the parameter, since a supplied value overrides the
  default, and a render supplying an ordered value succeeds. Refusing either at load, as the frozen
  §3.1 sentence "parameter defaults are instantiated to validate default geometry bounds" already
  suggests, is a load-time gap in its own right and needs its own issue; it is not part of the panic
  this change closes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `layout-sizing`: gains one `ADDED` requirement, "A dynamic width's resolved bounds are ordered",
  holding the complete contract for what happens when `format.width.min` and `format.width.max`
  resolve out of order at render. It supersedes the frozen `docs/SPEC.md` §3.1 auto-length paragraph
  so far as that paragraph concerns the bounds the label is clamped into, and nothing else in §3.1.
  `ADDED` rather than `MODIFIED` because no published requirement covers the case: the existing
  "An item requires of its frame the smallest extent that contains it" states the clamp into
  `[width.min, width.max]` and assumes the pair is ordered; this requirement is what makes that
  assumption hold.

## Impact

- `src/reason.rs`: one new `Reason` under the `InvalidRequest` group, `width_bounds_inverted`.
- `src/errors.rs`: one new `AppError` constructor building the `InvalidRequest` envelope for it.
- `src/render/mod.rs`: the guard in the dynamic-width pre-pass, between the two
  `check_dimension_limit` calls and the measure pass, and no other behavior change.
- `src/lib.rs`: HTTP tests over the issue's repro at the status-code level, the supplied-overrides-
  default case, the precedence over a measurement failure in both orders, and a batch test for the
  failure entry.
- No template schema, layout model, coordinate, UI or OpenAPI schema change. The reason
  completeness test in `src/errors.rs` reads this change's delta for the new slug.
