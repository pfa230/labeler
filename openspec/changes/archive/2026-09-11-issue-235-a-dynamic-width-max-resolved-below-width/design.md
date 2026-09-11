## Context

See `proposal.md` for motivation and `specs/layout-sizing/spec.md` for the contract. What shapes the
approach is the one place the two bounds meet:

- **The pre-pass.** `compile_label_source` (`src/render/mod.rs:735`) is the single entry every
  rendering path funnels through: `render_single_label*` and `render_thumbnail_png` via
  `compile_label_doc` (`:859`), and the batch, print and CSV paths through the same functions per
  label. On a dynamic-width `single` it resolves `max` then `min` with `resolve_dynamic_value_f32`
  (`:776-786`), runs `check_dimension_limit` on each (`:788-789`), measures the items against a frame
  of `max_w` (`:793-799`), and clamps the requirement with `root_w_req.clamp(min_w, max_w)` (`:800`).
  That clamp is `f32::clamp`, which panics on `min > max` or on a NaN bound.
- **What the dimension check already refuses.** `check_dimension_limit` (`:644-658`) rejects a bound
  that is `!is_finite()`, `<= 0.0`, or over `max_label_dimension_mm`, as `422 UnsupportedLayoutItem`
  with `dimension_exceeds_limit`. So NaN is unreachable at the clamp and the only reachable panic
  precondition is `min > max` between two positive, finite, capped values.
- **What load checks.** `Template::validate` runs on an `instantiate_with_defaults` copy and, for a
  dynamic width, checks only that both bounds are present (`src/templates.rs:1105-1118`). The
  `min <= max` check nearby (`:1939-1943`) is on an item's `max_w`/`max_h`, not on the format.
  Nothing orders the format's pair at load, whether the bounds are literals or defaults.
- **How a sibling refusal is shaped.** `line_spacing_param_invalid` (`src/errors.rs:285-294`,
  `src/render/mod.rs:1695`) is a render-time bound on a resolved parameter value: a dedicated
  `AppError` constructor wrapping `invalid_request` with its own `Reason`, a message naming the
  parameter and the value, no structured `details` beyond `reason`. `text-line-spacing` states that
  the bound reads values rather than provenance and why.
- **Where a slug is documented.** The reason-completeness test in `src/errors.rs` accepts a slug
  written in backticks in an active change's delta under `openspec/changes/*/specs/`, so the delta
  written here is what lets `cargo test` pass before archive.

## Goals / Non-Goals

**Goals:**

- No input reaches `f32::clamp` with `min > max`; the worker never panics on this path.
- The refusal is a `400 InvalidRequest` with a stable reason, built by one constructor, naming the
  parameter and both bounds, and it lands on every rendering path at once because it lives in the
  one pre-pass.
- The refusals that run before the guard, resolution and the dimension check, keep their order and
  their answers. Measurement runs after it, and the precedence that creates is documented rather than
  avoided; see the first decision.

**Non-Goals:**

- Enforcing a parameter's declared `min`/`max` at render (#238).
- Refusing a template whose literal or default bounds are inverted at load. Named in the proposal as
  a gap for its own issue; the decision below records why it is not folded in here.
- Any change to `resolve_dynamic_value_f32`, `check_dimension_limit`, the measure pass, or the
  clamp itself.

## Decisions

### The guard is a comparison in the pre-pass, between the dimension check and the measure pass

The check is `if min_w > max_w { return Err(...) }`, placed after both `check_dimension_limit` calls
and before `probe.measure_items`. Three placements were considered:

- **Before the dimension check.** Would let a `max` of `-5` reach the ordering check first and answer
  `width_bounds_inverted` for a value that is not a usable width at all, and would have to handle NaN
  itself. Rejected: the dimension check is the existing classification of "not a width", and running
  it first is what lets the new reason mean exactly "two usable widths, in the wrong order", which the
  spec states.
- **At the clamp, replacing `f32::clamp` with a hand-written min/max.** Would silently produce a
  width from an impossible pair, which is the "clamp into range" outcome `text-line-spacing` refused
  for the same kind of value. Rejected: the request asked for a label the renderer cannot produce, and
  the right answer is a refusal, not a guess.
- **After the dimension check, before measuring** (chosen). Wastes no measurement on a request that
  will be refused, keeps the clamp's precondition provable from the two lines above it, and adds
  nothing to the measure pass or to `resolver.rs`, whose sources-and-bounds model is about item
  extents and has no business with the label's own bounds.

This placement changes one precedence, and it is stated rather than hidden. Today `measure_items`
(`:793-799`) runs before the clamp, so a request whose bounds are inverted **and** whose items fail
measurement answers the item's `422` (for an authored extent resolving to zero, `size_invalid` via
`violation_error`, `:1204-1230`) and never reaches the panic. With the guard ahead of measurement the
same request answers `400 width_bounds_inverted`. Placing the guard after measurement would keep that
`422` but would measure every item against a frame of `max_w` that the label can never be clamped
into, and would leave the clamp's precondition depending on the measure pass having happened to fail
first. The label's bounds are judged before anything inside them; the proposal records this as the
error-contract change it is, and the delta pins both orders with scenarios.

### One constructor, `AppError::width_bounds_inverted`, and a message rather than structured details

The constructor takes the two resolved values, the unit, and, for each bound, the parameter name if
that bound was a reference. It builds `invalid_request(Reason::WidthBoundsInverted, message)`, the
shape `line_spacing_param_invalid` and `color_param_invalid` already use, so the `code` string is
`InvalidRequest` by construction and the slug is written in `reason.rs` once.

The message is the deliverable the issue asks for: "naming the parameter and the two bounds". It reads
along the lines of `format.width.max 5 mm (parameter 'max_width') is below format.width.min 10 mm`,
with the parenthetical present for each bound that was a reference and absent for a literal. Both
values are always named, whichever side the caller moved, because the caller cannot tell from one
number which bound they crossed.

Structured `details` fields (`min`, `max`, `parameter`) were considered and not added. The two sibling
parameter-value refusals carry `reason` only, the acceptance criterion is satisfied by the message, and
a `parameter` field would need to be a list or two fields to cover both bounds being references. That
is contract surface with no caller asking for it; it can be added under `MODIFIED` if one does.

### The refusal is strict: `min == max` renders

`f32::clamp` accepts `min == max` and returns that value, so equal bounds are not a panic precondition
and are a legitimate request for a fixed width through the dynamic spelling. Refusing them would turn
a working input into an error for no gain. The spec states equality as ordered and has a scenario
for it.

### Provenance is not tracked, and load is not changed

A bound that resolved from a declared `default`, or that is a literal in the template, reaches the
guard as an `f32` indistinguishable from a supplied value, and is refused as `400` like one. A default
is used only when the request omits the parameter (`resolve_parameters` starts from the supplied
`data` and fills absent names from `resolve_and_coerce_default`, `src/render/mod.rs:227-232`,
`:391-396`), so an inverted default is refused on the renders that omit the parameter and a supplied
ordered value renders. Two alternatives were considered:

- **Track provenance and answer `422 TemplateInvalid` when neither bound came from the request.**
  Would need `resolve_dynamic_value_f32` or `resolve_parameters` to report where each value came from,
  which is a change to shared resolution for one call site, and `text-line-spacing` already rejected
  the same machinery for the same reason.
- **Add a load-time ordering check on the instantiated pair.** Small, and probably right: the frozen
  §3.1 already says defaults are instantiated at load to validate geometry bounds, and a template
  whose literals are inverted can never render. But it is a second change to a second layer, it
  changes which templates load, and the issue settles scope as "the panic, and nothing else". It also
  does not remove the need for the render guard, because a template with a reference and no default
  loads with `min` instantiated to `0` (`resolve_f32_default`, `src/templates.rs:1708-1720`) and can
  still invert at render. So it is filed as its own issue rather than folded in.

The chosen shape mirrors the split `text-line-spacing` publishes for a `0` pitch default: the bound
lives where the value is used, cannot see the value's origin, and the spec says so rather than
smoothing it over.

### Tests are at the status-code level, in `src/lib.rs`

The issue's acceptance says the repro is covered by an HTTP test and not a unit test one layer below
the status code. `src/lib.rs` already holds the `oneshot`-driven HTTP tests for
`line_spacing_param_invalid` (`:17339-17360`) and for panic survival (`:9576-9600`), and the new tests
follow those: the repro template written to the test config dir, `POST /api/render/label?format=png`
with `max_width: 5`, asserting `400`, `InvalidRequest`, `width_bounds_inverted`, and the message's
parameter and both values; the same with `max_width: 10` asserting `200`; `max_width: 0` asserting
the existing `422 dimension_exceeds_limit`; a template whose `max_width` defaults to `5` against a
literal `min: 10`, with otherwise valid geometry, rendered with `max_width: 20` and asserting `200`;
a template with legal inverted bounds and a `text` item whose authored width `"{w}"` is supplied as
`0`, asserting `400 width_bounds_inverted`, and the same template with ordered bounds asserting the
existing `422 size_invalid`; and a two-label `POST /api/batch` asserting status `422`, a JSON content
type, a parsed `BatchInvalid` envelope whose `failures` holds exactly the one entry at index 1 with
`InvalidRequest` and `width_bounds_inverted`, and no PDF or ZIP payload. A follow-up `GET /api/health`
on the same app proves the service kept serving, which the `400` already implies but which costs one
request to make explicit.

What would have to break for these to fail: removing the guard turns the `400` assertions into a `500`
from the panic envelope; misordering it against the dimension check turns the `422
dimension_exceeds_limit` assertion into a `400`; placing it after measurement turns the
`width_bounds_inverted`-over-`size_invalid` assertion into a `422`; comparing with `>=` turns the
equal-bounds `200` into a `400`; resolving defaults ahead of supplied values turns the
supplied-overrides-default `200` into a `400`.

## Risks / Trade-offs

- [A literal-inverted template now answers `400` on every render, attributing a template fault to
  the request] → Stated in the spec as a known split, and the load-time refusal is named in the
  proposal as a gap for its own issue. Today the same template answers `500` on every render, so no
  caller is worse off.
- [The guard is a fourth check in a pre-pass that already has three, and a future edit could reorder
  them] → The spec makes the order observable and pins it from both sides: the dimension check's
  `422` before the guard's `400`, and the guard's `400` before measurement's `422`. A reorder fails a
  test rather than silently changing which reason a caller reads.
- [A request that fails both ways changes from `422` to `400`] → Documented in the proposal, the
  design and the delta as the error-contract change it is; a caller switching on `reason` reads
  `width_bounds_inverted`, which names the failure that has to be fixed first.
- [The message format is prose, and a client could parse it] → `reason` is the machine-readable
  field and the message is documented as naming, not as a grammar. Same trade-off every sibling
  reason already makes.
