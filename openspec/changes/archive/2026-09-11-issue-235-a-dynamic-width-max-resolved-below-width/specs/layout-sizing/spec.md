## ADDED Requirements

### Requirement: A dynamic width's resolved bounds are ordered

On a dynamic-width `single` (`format.width: { min, max }`), each bound is a literal or a `"{param}"`
reference, and both are resolved per render before the label is measured. The resolved `min` SHALL be
less than or equal to the resolved `max`. A render whose resolved `min` is strictly greater than its
resolved `max` SHALL be refused with `400 InvalidRequest` and `details.reason` of
`width_bounds_inverted`, and the message SHALL name both resolved values with the template `unit` and,
for each bound that is a parameter reference, the parameter it resolved from. The render SHALL NOT
clamp the content requirement into the inverted pair, SHALL NOT swap the bounds, SHALL NOT proceed at
either bound alone, and SHALL NOT panic. Equal bounds are ordered: a render whose `min` and `max`
resolve to the same value renders at that width.

**The order of refusals is observable.** Each resolved bound is first judged as a dimension, exactly as
it is today: a bound that is not a finite number greater than zero, or that exceeds the
`max_label_dimension_mm` setting, is refused with `422 UnsupportedLayoutItem` and
`dimension_exceeds_limit`, and the pair is never compared. Only two bounds that are each a legal
dimension reach the ordering check, so `width_bounds_inverted` means exactly that: two usable
widths, in the wrong order. A `max` of `0` or `-5` against a `min` of `10` is therefore
`dimension_exceeds_limit`, not `width_bounds_inverted`.

The ordering check runs **before the items are measured**, so it precedes every refusal measurement
can raise. A request whose bounds are inverted and whose items would also fail measurement, such as an
authored item extent that resolves to zero, is refused with `400 width_bounds_inverted`, and the item
failure is not reached. The same request with ordered bounds is refused by measurement as it is today,
`422 UnsupportedLayoutItem` with `size_invalid` for that example. **This changes the error contract
for one class of request:** today measurement runs first and such a request answers the item's `422`,
never reaching the clamp; after this change it answers `400 width_bounds_inverted`. The label's own
bounds are judged before anything inside them, because an item cannot be measured against a frame
that does not exist.

**The refusal reads values, not provenance.** The check applies to the values resolved **for that
render**: a supplied value for a referenced bound is used, and a parameter's declared `default` is
used only when the request omits it. A bound that resolved from a `default`, or that is a literal in
the template, meets the check exactly as a caller-supplied value does, and is reported the same way.
`param-resolution` still decides a default that cannot be resolved at all (`422 TemplateInvalid`,
`param_default_unresolvable`) before any bound exists to compare. Two consequences follow. A template
whose two bounds are literals in the wrong order is refused with `400 width_bounds_inverted` on every
render, because nothing a request carries can change either value. A template whose declared default
puts a referenced bound out of order is refused on exactly the renders that leave that parameter to
its default, and a render supplying a value that orders the pair succeeds. Whether a template of
either kind ought to be refused when it is loaded is a separate question this requirement does not
answer; what it guarantees is that no render reaches the clamp with the pair inverted.

**Every path that renders a label is covered.** The single-label render, the thumbnail, and each
label of a batch, print or CSV import request resolve the bounds through the one pre-pass, and the
refusal above applies on each. On the batch path a refused label is a failing label under
`batch-validation`: the request SHALL return `422` with `error.code` `BatchInvalid` and one entry in
`error.details.failures` at that label's `index` carrying `code` `InvalidRequest` and `reason`
`width_bounds_inverted`, and SHALL produce nothing.

This requirement supersedes the frozen `docs/SPEC.md` §3.1 auto-length paragraph so far as it
concerns the bounds the label width is clamped into (the words "clamping to `[min, max]`"). It leaves
the rest of that paragraph, the §3.1 load-time sentence about instantiated defaults, and every other
§3.1 rule authoritative, and it does not alter which width an item contributes, which "An item
requires of its frame the smallest extent that contains it" already states.

#### Scenario: A supplied `max` below `min` is refused

- **WHEN** a `single` template declares `width: { min: 10.0, max: "{max_width}" }` on a `length`
  parameter `max_width` with `default: 120.0`, and `POST /api/render/label?format=png` is sent with
  `data.max_width` of `5`
- **THEN** the response is `400` with `error.code` `InvalidRequest`, `error.details.reason`
  `width_bounds_inverted`, and a message naming `max_width`, `5`, and `10`

#### Scenario: A supplied `min` above a literal `max` is refused, naming the parameter that moved

- **WHEN** a `single` template declares `width: { min: "{min_width}", max: 60.0 }` and a render
  supplies `min_width` of `80`
- **THEN** the response is `400` with `error.details.reason` `width_bounds_inverted` and a message
  naming `min_width`, `80`, and `60`

#### Scenario: Both bounds are references and both are named

- **WHEN** a `single` template declares `width: { min: "{lo}", max: "{hi}" }` and a render supplies
  `lo` of `30` and `hi` of `20`
- **THEN** the response is `400` with `error.details.reason` `width_bounds_inverted` and a message
  naming `lo`, `hi`, `30`, and `20`

#### Scenario: Equal bounds render

- **WHEN** a `single` template declares `width: { min: 10.0, max: "{max_width}" }` and a render
  supplies `max_width` of `10`
- **THEN** the render succeeds and the label is `10` units wide

#### Scenario: A non-positive `max` is a dimension failure, not an ordering failure

- **WHEN** a `single` template declares `width: { min: 10.0, max: "{max_width}" }` and a render
  supplies `max_width` of `0`
- **THEN** the response is `422` with `error.code` `UnsupportedLayoutItem` and `error.details.reason`
  `dimension_exceeds_limit`, and the ordering check is not reached

#### Scenario: An inverted default is refused on the render that resolves it

- **WHEN** a `single` template declares `width: { min: 10.0, max: "{max_width}" }` on a parameter
  `max_width` with `default: 5.0`, and a render omits `max_width`
- **THEN** the response is `400` with `error.details.reason` `width_bounds_inverted`, naming
  `max_width`, `5`, and `10`

#### Scenario: A supplied value overrides an inverted default

- **WHEN** a `single` template declares `width: { min: 10.0, max: "{max_width}" }` on a parameter
  `max_width` with `default: 5.0` and otherwise valid geometry, and a render supplies `max_width` of
  `20`
- **THEN** the render succeeds

#### Scenario: Inverted bounds are refused before an item is measured

- **WHEN** a `single` template declares `width: { min: 10.0, max: "{max_width}" }` and a `text` item
  whose authored width is `"{w}"` on a parameter `w` with a positive default, and a render supplies
  `max_width` of `5` and `w` of `0`
- **THEN** the response is `400` with `error.details.reason` `width_bounds_inverted`, and the item's
  zero width is not reported

#### Scenario: With ordered bounds the item's own refusal stands

- **WHEN** the template of the previous scenario is rendered with `max_width` of `40` and `w` of `0`
- **THEN** the response is `422` with `error.code` `UnsupportedLayoutItem` and `error.details.reason`
  `size_invalid`, as it is today

#### Scenario: The worker does not panic and the service keeps serving

- **WHEN** the request of the first scenario is sent
- **THEN** the response is the `400` above and not the `500 Internal` a covered panic answers with,
  and a following `GET /api/health` on the same service answers `200`

#### Scenario: A batch carrying an inverted label is atomic

- **WHEN** `POST /api/batch` carries two labels of the template in the first scenario, the first
  supplying `max_width` of `40` and the second supplying `max_width` of `5`
- **THEN** the response is `422` with `error.code` `BatchInvalid`, `error.details.failures` holds
  exactly one entry, at `index` `1`, with `code` `InvalidRequest` and `reason`
  `width_bounds_inverted`, and no PDF or ZIP is produced
