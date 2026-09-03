## MODIFIED Requirements

### Requirement: The service computes an input list for a given label

`POST /api/templates/{id}/inputs` SHALL accept `{ "labels": [ { "data": { ... } }, ... ] }`, the same
label shape `POST /api/batch` accepts, and SHALL return `{ "inputs": [ [ ... ], ... ] }`, one input
list per label, in the order the labels were given.

A key other than `data` on a label SHALL be ignored, exactly as the render paths ignore it today. In
particular an `option` key has no effect here, because it has none on `POST /api/render/label` or
`POST /api/batch` either: those paths read only `data`. Retiring the vestigial `option` map from the
UI is #214 and is not part of this capability.

An empty `labels` array SHALL return an empty `inputs` array with `200`. More labels than
`POST /api/batch` accepts SHALL be refused with the same status and `code` that endpoint uses for the
same condition. A body that is not valid JSON of this shape SHALL be `400 InvalidRequest`, as
elsewhere. An unknown template id SHALL be `404 TemplateNotFound`.

Which entries appear SHALL be decided by the same rule the renderer applies: the label's parameters
are resolved, each item's `when:` is evaluated against them, and an inactive item, together with
everything nested inside an inactive container, reads nothing.

**Lenient resolution.** Resolution on this path differs from rendering in exactly two ways, and in no
other.

1. **A value that cannot be coerced** to its declared type SHALL be treated as though the label did not
   carry that name at all. Everything downstream then follows the ordinary omission rules, so the
   parameter takes its declared `default` if it has one and is otherwise absent (`param-resolution`),
   and gates naming it are evaluated against that — an absent parameter making its gate false rather
   than raising.
2. **A key naming no declared parameter** SHALL be ignored, and the entry list SHALL be exactly the one
   the same label would produce without it. The paths that render or print refuse such a key
   (`request-data-keys`); this one SHALL NOT. The Import screen posts a CSV row's whole `data` map here
   to learn which of its columns the chosen template can read, so a spreadsheet carrying a column the
   template never declares must still get a list back rather than an error — the list is what the
   screen prunes the row down to before it submits.

This endpoint SHALL NOT reject a request because of a value's content or because of a name the
template does not declare.

**A declared default is resolved here exactly as a render resolves it.** This path SHALL read the
variables store and capture one instant per request, and SHALL resolve every declared default against
that snapshot by the one resolution `param-resolution` defines. A parameter whose default **resolves**
therefore has the value a render would give it, both for what the entry publishes and for evaluating a
gate that names it, so for that parameter this endpoint and a render of the same label select the same
branch.

A default that fails to resolve SHALL NOT reject the request. The parameter is absent for the purpose of
evaluating a gate that names it, exactly as an unsupplied parameter is, and its entry carries no
`default`, `required: true` and the `default_error` describing the failure. The failure is the
template's and this endpoint only reports it; the request supplied nothing to fix.

**Absorbing it is this path's rule and not the render's**, so branch parity is claimed only for defaults
that resolve. A render resolves every declared parameter before it evaluates any `when:`, so a label
omitting a parameter whose default fails is `422 TemplateInvalid` with reason `param_default_unresolvable`
and selects no branch at all (`param-resolution`). This endpoint answers a different question — what must
the operator still supply — and answering it with a `422` would leave a client with no list at all for a
template it can still print from. Once a label supplies that parameter the default is never reached, and
the branch this endpoint reported for that label is the branch its render takes.

`POST /api/templates/{id}/inputs` and `GET /api/templates/{id}` SHALL resolve by the same rule and from
the same three sources — the variables the store holds, the effective `datetime_formats`, and one instant
captured by the request being served. **The endpoint SHALL NOT be a source of difference**: two requests
whose snapshots agree SHALL publish the same `default`, `default_error` and `required` for a given
parameter, whichever of the two they were. A screen that mixes rows from both in one form is the reason;
a parameter that defaulted differently by call site would seed one control and not another.

Two requests whose snapshots differ MAY publish different values, and that is not a violation of the
rule above. Each request reads the store when it is served and captures its own instant, so a default
reading `{sys.now}` across midnight, or a variable edited between two calls, legitimately resolves
differently — exactly as it would for two renders. What is guaranteed is the rule and the sources, not a
value frozen across requests. Within **one** request nothing may differ: `inputs.default`, `inputs.all`
and `param_defaults` on one response, and every label's list in one `POST /api/templates/{id}/inputs`
body, SHALL all report the one resolution that request **published**. Whether the service resolves a
default a second time while deciding which entries are active is an implementation matter this
capability does not constrain, provided every published field comes from that one published resolution
and any further resolution uses the same captured context.

`required` SHALL NOT change with the value the label carries: an `enum` whose declared `default:`
resolves stays `required: false` whether the label carries a valid value, an invalid one, or none, and
one declaring no `default:` stays `required: true` in all three cases. This is what the lenient rule
above would otherwise put at risk, since it absorbs an uncoercible value by falling back to the default.

What `required` does depend on, besides the declaration, is whether that declared default **resolves**
for this request, under the rule the first requirement states. A parameter whose declared default fails
to resolve is `required: true`, and it is so for every label in the request alike, because the failure
depends on the template and the request's snapshot and on no label's data. The two rules do not
compete: no label's data can move `required`, and the snapshot that can is fixed for the whole request.

Rendering is unchanged by this requirement. Everything this endpoint absorbs still fails a render, with
the code that path already returns: an out-of-range `enum` is `422 InvalidEnumValue`; an uncoercible
`integer`, `number`, `length` or `boolean` is `400 InvalidRequest`; an unparseable `datetime` is
`400 InvalidRequest` with reason `datetime_param_invalid` (`datetime-params`); a key naming no declared
parameter is `400 InvalidRequest` with reason `data_key_unknown` (`request-data-keys`); and a per-label
failure inside `POST /api/batch` is reported as `422 BatchInvalid` carrying that label's own code.

That the two differ here is deliberate and is stated from both sides: the divergence is written into
`request-data-keys`, which scopes its rule to the four paths that render or print, and into this
requirement, which names the one endpoint that ignores what they refuse. Neither is an exception carved
out of the other; they answer different questions. A render asks what to draw, and a key it cannot draw
from is a caller's mistake. This endpoint asks what the operator must still supply, and answering that
for a row it was handed whole is the only way a client can learn which of its columns to keep.

#### Scenario: One request answers several labels

- **WHEN** two labels are sent, one selecting `orientation: horizontal` and one `orientation: vertical`
- **THEN** two input lists come back in that order, the first holding `subtitle` and not
  `tracking_url`, the second the reverse

#### Scenario: A blank enum falls back and still answers

- **WHEN** a label carries `orientation: ""` for an `enum` declaring
  `values: [horizontal, vertical]` and no default
- **THEN** the response is `200`, the `orientation` entry carries `required: true` and no `default`, and
  the list is computed with `orientation` absent, so a branch gated on it is reported inactive

#### Scenario: A gate on a tokened default follows the render

- **WHEN** a template declares `mode: { type: string, default: "{vars.mode}" }`, a container carries
  `when: { mode: full }`, and the store holds `mode = full`
- **THEN** the input list reports that container's contents as present, because this path resolves the
  default exactly as the render of the same label does

#### Scenario: A gate on a default that cannot resolve stays inactive

- **WHEN** the same template is read with no `mode` in the store
- **THEN** the response is `200`, that container's contents are reported absent, and the `mode` entry
  carries `required: true` and a `default_error`

#### Scenario: The render of that same label does not select a branch at all

- **WHEN** that label, still omitting `mode`, is sent to `POST /api/render/label`
- **THEN** it is refused with `422 TemplateInvalid` and reason `param_default_unresolvable`, because a
  render resolves every declared parameter before evaluating any gate

#### Scenario: Supplying the parameter restores parity

- **WHEN** that label carries `mode: full`
- **THEN** the input list reports the container's contents as present and the render draws them, because
  the default is never reached

#### Scenario: The two endpoints agree under one snapshot

- **WHEN** a client reads the entry for a parameter declaring `default: "{vars.base}"` from
  `GET /api/templates/{id}` and from `POST /api/templates/{id}/inputs` for a label that omits it, with
  the variables store unchanged between the two calls
- **THEN** the two entries carry the same `default`, the same `default_error` and the same `required`

#### Scenario: A store edit between two calls legitimately changes the answer

- **WHEN** `base` is edited between those two calls
- **THEN** each response reports what the store held when it was served, and neither is wrong

#### Scenario: One response is internally consistent

- **WHEN** a `POST /api/templates/{id}/inputs` body carries several labels that all omit a parameter
  declaring `default: "{sys.now}"`
- **THEN** every label's entry for it carries the same `default`, because every label's entry is read
  from the one resolution the request published

#### Scenario: An unparseable number does not fail the request

- **WHEN** a label carries `copies_shown: "abc"` for an `integer` parameter declaring `default: 1`
- **THEN** the response is `200`, the list holds `copies_shown` with `default: 1`, and any gate on
  `copies_shown` is evaluated as though the label carried `1`

#### Scenario: The same value still fails a render, with its own code

- **WHEN** the label of the previous scenario is sent to `POST /api/render/label`
- **THEN** it is rejected with `400 InvalidRequest`, unchanged from today

#### Scenario: An out-of-range enum still fails a render with its own code

- **WHEN** a label carries `orientation: "sideways"` and is sent to `POST /api/render/label`
- **THEN** it is rejected with `422 InvalidEnumValue` as defined by `enum-validation`

#### Scenario: A key the template does not declare is ignored

- **WHEN** a label carries `{ "title": "Bolts", "sku_legacy": "X-1" }` for a template declaring `title`
  and not `sku_legacy`
- **THEN** the response is `200` and the input list is exactly the one the same label without
  `sku_legacy` produces

#### Scenario: The same label fails a render

- **WHEN** that label is sent to `POST /api/render/label`
- **THEN** it is refused with `400 InvalidRequest` and `details.reason` `data_key_unknown`, and the two
  outcomes are both correct because the two endpoints answer different questions

#### Scenario: An option key is ignored

- **WHEN** a label carries both `data` and an `option` key
- **THEN** the input list is the one for its `data` alone, matching what `POST /api/batch` renders for
  the same label

#### Scenario: No labels is not an error

- **WHEN** the request carries an empty `labels` array
- **THEN** the response is `200` with an empty `inputs` array

#### Scenario: Too many labels are refused

- **WHEN** more labels are sent than `POST /api/batch` accepts
- **THEN** the request is refused with the same status and `code` `/api/batch` uses for that condition
