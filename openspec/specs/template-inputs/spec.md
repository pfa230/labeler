# template-inputs Specification

## Purpose

Defines the input list: the set of controls an operator must be offered for one label, derived by the
service from the template and the values the label already carries. It is the service's answer to
"what does this label still need", so that no client has to walk a layout, evaluate a `when:`
condition, or reproduce how a parameter value is coerced before it is compared.

## Requirements

### Requirement: An input list describes the controls one label needs

An **input list** is an ordered list of entries, one per distinct name the operator may be asked for.
Each entry carries everything needed to render one control and to decide whether the label is
complete:

| Field | Meaning |
| --- | --- |
| `name` | The declared parameter the control fills, which is the request `data` key that carries it. |
| `control` | `text`, `textarea`, `integer`, `number`, `select`, `checkbox`, `date`, `datetime`, `image`, or `list`. |
| `slider` | For `integer` and `number`, whether both bounds are declared so the control is a slider. False otherwise. |
| `required` | Whether the label is incomplete without a value. |
| `default` | The value the service would use if the label omitted this name: the parameter's declared default **as it resolves for this request**, after the same coercion a supplied value takes. Absent when the parameter declares no default, and absent when its declared default failed to resolve. |
| `default_error` | Why the declared default could not be resolved, as `{ reason, message, token?, value? }` — the read-only projection `param-resolution` defines, which names no parameter because the entry it hangs on already does. Absent when the parameter declares no default and when its default resolved. |
| `values` | For `select`, the allowed values in declared order. Absent otherwise. |
| `min`, `max` | For `integer` and `number`, the declared bounds. Absent otherwise. |
| `unit` | For a `length` parameter, the template's unit, for display beside the control. Absent otherwise. |
| `description` | The parameter's declared description. Absent when it declares none. |
| `interpolated` | Whether some active item reads this name **as a value**: a `text` or `qr` token, an `image` `name:`, or an interpolated `image.src`. False for a name present only because it gates an item or resolves a layout attribute. |
| `truncated_elsewhere` | Whether some `wrap: false` `text` item anywhere in the template, in any branch, reads this name. **The name is historical.** It meant that such an item would render only the value's first line; since #251 every segment of a value enters layout regardless of `wrap`, so the flag no longer reports a loss. It is still computed, still returned, and the print form still renders its note from it — a warning about a truncation that no longer happens. Issue #269 removes the field, the computation and the note together, because deleting a response field is a change of its own. |

An entry SHALL be present for a name the label's render will read, and for no other name. In
particular an entry SHALL be present for a parameter read only as a `when:` key, and for one read
only by a layout attribute, so the operator keeps the control that selects a branch or sizes a box.

**Every entry names a parameter the template declares.** A template reads only names it declares
(`interpolation-tokens`), so there is no other kind of entry and no branch of these rules for one. The
entries are still narrower than `params:`: a declared parameter no active item, condition or attribute
reads has no entry.

A name resolved by the service SHALL NOT appear: a `{vars.<key>}` reference, a `{sys.now}` token, and
any name no active item reads. The retired `{datetime}` and `{datetime.<format>}` spellings this
sentence used to name are gone: `{datetime.<format>}` fails when the template loads, and `{datetime}`
is an ordinary bare token, so it is an entry when the template declares that parameter and a load-time
refusal when it does not (`interpolation-tokens`).

**`control` is decided by the declaration**, which preserves how the print form renders a declared
parameter today. It follows the declared type: `select` for `enum`; `checkbox` for `boolean`; `date` or
`datetime` for `datetime` according to its `time` flag; `integer` for `integer`; `number` for `length`
and `number`; `textarea` for a `string` declaring `multiline: true`, and `text` for a `string`
otherwise. The one override is `image`: a `string` parameter that any active `image` item binds through
its `name:` gets `image`, since the value it carries is a data URI.

A parameter declared `type: list` gets `list`. It is reported on exactly the terms every other type
is, and this capability states no exception for it: the entry carries the name, the control, whether
a value is required, and the resolved `default` when there is one, which for a `list` is an array of
strings. `values`, `min`, `max` and `unit` are absent, as they are for every type that declares none.

`integer` and `number` are distinct controls, not one numeric control, because they are stepped and
parsed differently: a client steps an `integer` by 1 and reads a whole number, and steps a `number`
freely and reads a decimal. Collapsing them would force the client back to the declared type to
tell them apart. `slider` then says whether the control is presented as a slider, which is true
exactly when both `min` and `max` are declared.

**No layout flag decides a control.** The rule that read `wrap: true` as a `textarea` applied to a name
the template did not declare, and that case no longer exists, so the leftover this capability was
keeping until #269 goes with it. A declared `string` read by a `wrap: true` text item but declared
`multiline: false` keeps its single-line control, as it already did.
`truncated_elsewhere` still reports the reverse pairing, but
it no longer describes a loss: since #251 every `\n` segment of a value enters layout under either
control and under either flag. Only the authored `overflow` policy may then shorten a line, drop lines,
or — under `overflow: fail` — reject the render with `text_does_not_fit`; a shortened or dropped line is
marked.

`default` SHALL carry the value the render path would use for this parameter had the label omitted it:
the declared `default:` interpolated against the request's snapshot and then coerced, by the one
resolution `param-resolution` defines. It is not the declared text. A `length` declaring
`default: "80mm"` therefore publishes `80`, a `string` declaring `default: "{vars.base}"` publishes what
`vars.base` holds, and a `datetime` publishes the render path's own rendering of the instant it resolves
to. Every published default is a value a client may submit unchanged and a value the label would print.

`default` SHALL be absent when the parameter declares none, and SHALL be absent when its declared
default failed to resolve — a token in it naming nothing, or a resolved value the parameter's own
declaration forbids. In the second case `default_error` SHALL carry that failure, with the same
`reason`, `message`, `token` and `value` the render path reports for it (`param-resolution`), so a
client rendering only an input list can say why the control is empty without a second request.

`required` SHALL be false exactly when the entry publishes a `default`, and true otherwise. It means
"this parameter has no usable resolved default", which covers a parameter declaring none and a
parameter whose declared default failed to resolve. A parameter whose default failed is
`required: true` because an operator must supply a value for the print to succeed. Neither type nor the
presence of a `default:` in the template decides it: a `boolean`, an `enum` and a `datetime` declaring no
default are each `required: true`, and so is one declaring `default: "yes"` on a `boolean`, because the
coercion that rejects the request value `"yes"` rejects that default too.

Resolution is per request, so an entry's `default`, `default_error` and `required` may differ between two
requests for the same template when the variables store or the instant differs. The report on the
template detail is keyed on the same resolution, and the two SHALL agree entry for entry.

**A client SHALL tolerate a `control` it cannot draw.** `list` is that control today: this change
reports the entry and #318 builds the editor for it, so between the two a screen is told a `list` input
exists and has no widget for it. Such a screen SHALL omit that entry's control and SHALL NOT fail,
break its layout, or drop the rest of the form. It is the one UI-visible obligation this change carries,
and it is stated here rather than left to be discovered because a screen that renders every reported
entry unconditionally is the shape a new control breaks.

The consequence, stated rather than discovered: until #318 lands a `list` parameter is suppliable only
by an API caller, so a print screen for a template reading one submits without it and the render is
`422 MissingField` naming a field it showed no control for. A resolvable `default:`, `default: []`
included, avoids that.

Entries SHALL be ordered by name, ascending. Ascending rather than "as written" because `params` is an
ordered map keyed by name and a template's authoring order is not retained. There is no second
ordering group: every entry names a declared parameter, so nothing is ordered by where the layout first
reads it, and the layout-order bookkeeping that group needed has no other reader.

#### Scenario: A gated field is absent from the list

- **WHEN** a template reads `{subtitle}` only inside a container gated on `orientation: horizontal`
  and a label selects `orientation: vertical`
- **THEN** the label's input list holds no entry for `subtitle`

#### Scenario: The parameter that selects a branch is an input

- **WHEN** a template reads `orientation` nowhere except as a `when:` key on two containers
- **THEN** every label's input list holds an entry for `orientation`, with control `select`

#### Scenario: A parameter that only sizes something is an input

- **WHEN** a template declares `width` as a `length` parameter read only by `format.width`
- **THEN** every label's input list holds an entry for `width`, with control `number` and the
  template's unit

#### Scenario: A declared control ignores a conflicting use

- **WHEN** a template declares `title` as a `string` with `multiline: false` and a `wrap: true` `text`
  item reads `{title}`
- **THEN** its entry carries control `text` and `truncated_elsewhere` false, since no `wrap: false`
  item reads it

#### Scenario: An image binding overrides a string declaration

- **WHEN** a template declares `logo` as a `string` and an active `image` item carries `name: "logo"`
- **THEN** its entry carries control `image`

#### Scenario: An undeclared name read by a multiline item gets a textarea

- **WHEN** a `wrap: true` `text` item reads `{body}` and `body` is not declared under `params:`
- **THEN** the template is quarantined, so no input list is derived for it and no entry is produced
  under any control
- **AND** where the same template declares `body: { type: string, multiline: true }` with no
  `default:`, its entry carries control `textarea` and `required` true, decided by that declaration
  rather than by the item's `wrap`

*This scenario's name records the rule it replaces. A `MODIFIED` requirement carries every scenario
name the spec already has, so the name stays while the behaviour under it does not.*

#### Scenario: An interpolated image source is an input

- **WHEN** an `image` item carries `src: "{asset_path}"` and the template declares
  `asset_path: { type: string }` with no `default:`
- **THEN** the input list holds an entry for `asset_path` with control `text`, since the value names a
  bundled asset rather than carrying image bytes

#### Scenario: A resolved name is never asked for

- **WHEN** a template declaring `id: { type: string }` interpolates `{vars.base_url}/{id}` in a `qr`
  item and `{sys.now:iso_date}` in a `text` item
- **THEN** the input list holds an entry for `id` and none for `base_url` or `sys.now`

#### Scenario: A parameter the template never reads is not an input

- **WHEN** a template declares a parameter that no item, condition or attribute reads
- **THEN** no input list holds an entry for it

#### Scenario: An enum carries the value it would resolve to

- **WHEN** a template declares `orientation` with `values: [horizontal, vertical]` and
  `default: vertical`
- **THEN** its entry carries `values: [horizontal, vertical]`, `default: vertical` and
  `required: false`

#### Scenario: Entries are ordered by name, then by first use

- **WHEN** a template declares `zebra`, `alpha` and `mid`, and a `text` item reads `{zebra}` before
  `{mid}` and `{alpha}`
- **THEN** the list runs `alpha`, `mid`, `zebra`, ordered by name alone: the second group this
  scenario's name records is gone, because every entry names a declared parameter

#### Scenario: A tokened default is published as the value it resolves to

- **WHEN** a template declares `url: { type: string, default: "{vars.base}" }` and the store holds
  `base = https://example.test`
- **THEN** its entry carries `default: "https://example.test"`, `required: false` and no `default_error`

#### Scenario: A default naming an absent variable publishes its diagnostic instead

- **WHEN** the same template is read with no `base` in the store
- **THEN** its entry carries no `default`, `required: true`, and a `default_error` whose `reason` is
  `param_default_unresolvable` and whose `token` names `vars.base`

#### Scenario: A published default is post-coercion

- **WHEN** a template declares `width: { type: length, default: "80mm" }`
- **THEN** its entry carries `default` equal to the number `80`, which is the value the render path
  uses, rather than the text `80mm` that no numeric control can hold

#### Scenario: A default a request could not have sent is not published

- **WHEN** a template declares `bold: { type: boolean, default: "yes" }`
- **THEN** its entry carries no `default`, `required: true`, and a `default_error` whose `value` names
  `yes`, rather than a default a client would submit and be rejected for

#### Scenario: A default resolving through a datetime format is published

- **WHEN** a template declares `printed_on: { type: datetime, default: "{sys.now:iso_date}" }` and the
  store's `datetime_formats` maps `iso_date` to `%Y-%m-%d`
- **THEN** its entry carries `default` equal to the request's date rendered `%Y-%m-%d`, which is what the
  render path resolves it to, and `required: false`
- **AND** the entry could not have been computed without the formats map, so a derivation lacking one
  cannot produce this list

#### Scenario: A list parameter is reported like any other

- **WHEN** a template declares `tags: { type: list, default: [CONSUMABLE] }` and an active item renders
  `{tags:join(', ')}`
- **THEN** the input list holds a `tags` entry with control `list`, `default` equal to the JSON array
  `["CONSUMABLE"]`, `required: false`, and no `values`, `min`, `max` or `unit`

#### Scenario: An undefaulted list is required

- **WHEN** the same template declares `tags: { type: list }`
- **THEN** its entry carries `required: true` and no `default`

#### Scenario: A screen skips a control it cannot draw

- **WHEN** the print form renders the inputs for that template before #318 lands
- **THEN** it renders every other entry, renders no control for `tags`, and does not fail
- **AND** submitting is `422 MissingField` naming `tags` when the parameter declares no default
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
the code that path already returns: an out-of-range `enum` is `422 InvalidOptionValue`; an uncoercible
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
- **THEN** it is rejected with `422 InvalidOptionValue`, unchanged from today

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

### Requirement: The template detail carries the lists a client needs before it has a label

`GET /api/templates/{id}` SHALL include:

- `inputs.default`: the input list for a label carrying no `data`. A client renders its first form
  from this without a second round trip.
- `inputs.all`: the union of every entry any label could produce, one per distinct name, ignoring
  every `when:` condition. It is what the thumbnail and the template preview fill their sample values
  from, for the closure reason given in the thumbnail requirement, and what a view describing the
  template rather than a label reads.
- `variables`: the `{vars.<key>}` keys the layout reads, as a list of keys without the prefix,
  ascending.
- `param_defaults`: what each declared default resolves to for this request, under the requirement
  "The template detail reports what each declared default resolves to". It is keyed on the declared
  parameters rather than on either input list, so it covers a parameter no branch reads.

Every other response carrying the same template-detail body SHALL include them too, `param_defaults`
included. That is every response of this shape, not only `GET /api/templates/{id}`: creating a template,
replacing one and moving one between groups each return it, and each is read by a client that will seed
a form from it.

An entry in `inputs.all` SHALL carry the same fields as one in `inputs.default`, decided by the same
rule. That rule yields one `control` per name from the declaration alone, and every name is declared,
so two branches cannot disagree about an entry and the union needs no rule for widening one. The one
override use decides, `image` for a `string` an `image` item binds, applies to the union whenever
**any** branch binds it, so the union never offers a control that cannot hold what some branch needs.

#### Scenario: The detail lists inputs from every branch

- **WHEN** a template reads `{subtitle}` only under `orientation: horizontal` and `{tracking_url}`
  only under `orientation: vertical`
- **THEN** `inputs.all` holds both, and `inputs.default` holds only the one the default selection
  reads when `orientation` declares a `default:`; when it declares none it is absent under
  `param-resolution`, neither branch is active, and `inputs.default` holds neither

#### Scenario: The union prefers the wider control for an undeclared name

- **WHEN** undeclared `{title}` is read by a `wrap: true` `text` item in one branch and a `wrap: false`
  one in another
- **THEN** the template is quarantined, so there is no union to widen
- **AND** where a declared `string` `logo` is bound by an `image` item's `name:` in one branch and
  printed by a `text` item in another, its `inputs.all` entry carries control `image`, which is the
  only widening left and is decided by use in every branch

#### Scenario: Variables are listed separately

- **WHEN** a `qr` item interpolates `{vars.base_url}/{id}`
- **THEN** `variables` holds `base_url`, and neither input list holds an entry for it

#### Scenario: The addition does not disturb the rest of the body

- **WHEN** a client reads `GET /api/templates/{id}`
- **THEN** every other field of the response is unchanged, and the response still carries no
  `options` key

#### Scenario: A newly written template comes back with its report

- **WHEN** a client creates or replaces a template through `POST`/`PUT`, or moves one between groups
- **THEN** the `TemplateDetail` it receives carries `param_defaults` for the template it just wrote,
  computed on the same terms `GET /api/templates/{id}` computes it
### Requirement: The thumbnail renders the default selection from placeholder data

*This requirement supersedes the `GET /templates/{id}/thumbnail` bullet of `docs/SPEC.md` §2.0
("Template management") and restates its complete post-change contract. The rest of §2.0 is unchanged
and remains authoritative.*

`GET /api/templates/{id}/thumbnail` renders a representative PNG for the template using placeholder
data. For a `single` template it renders the one label. For a `sheet` template it renders a single
label slot, not a full sheet, so the preview is label-sized regardless of format.

The placeholder data SHALL be built from `inputs.all`, not from `inputs.default`, so that whichever
branch the finished label selects, every name that branch reads already has a value. Where the frozen
contract said every field is filled with its field name, the post-change rule is narrower in *which*
names it fills but stays ungated in *which branches* it covers: the thumbnail SHALL invent a value
only for an entry satisfying both of

1. `interpolated` is true, so some active item reads the name as a value;
2. `required` is true, so the service has no value of its own for it;

and SHALL invent by the entry's `control`: a 1×1 PNG data URI for `image`, the entry's own name for
`text` and `textarea`, for `integer` and `number` the entry's `min` when it declares one and `1`
otherwise, `false` for `checkbox`, the request's captured instant for `date` and `datetime`, and for
`list` a one-element list holding the entry's own name.

The `list` fill is the `text` rule applied to the type that has no other sensible one, so
`{tags:join(', ')}` renders `tags` on a thumbnail exactly as `{title}` renders `title`. It is legal for
the parameter, it is visibly a placeholder to anyone looking at the image, and like every other fill
here it never reaches a render a caller asked for.

The numeric case has to be filled and has to be filled with a *number*. A required `length`,
`number` or `integer` resolves to nothing when omitted — as, after `param-resolution`, does every other
type without a declared default — so leaving it empty makes an active `{width}` token or a dynamic `size`
fail with `MissingField`. Filling it with the entry's own name, which is what the walker this requirement
replaces does, fails coercion instead. A declared bound, or `1`, is coercible and inside any declared
range.

A parameter whose declared default **fails to resolve** is `required: true` by that rule, so the
thumbnail SHALL invent for it on exactly the terms it invents for a parameter declaring no default at
all. The preview therefore renders where a caller's render of the same template would be
`422 TemplateInvalid`. That is the uniform rule applied and not a carve-out: every value a preview
prints is one the service chose, a preview never reaches a render a caller asked for, and it has never
claimed that a caller's render would succeed. What says the default is broken is `param_defaults`, which
is on the same response the catalog grid already reads.

`checkbox`, `date` and `datetime` join that list because `param-resolution` removed the fallbacks that
used to make them resolve on their own. A `boolean` without a declared `default:` is no longer `false`
and a `datetime` without one is no longer the render instant; each is now `required: true`, so a
thumbnail that did not invent for them would fail with `MissingField` on every template that reads one.
What they get is a preview-only placeholder on exactly the terms `text` gets its own name: it is chosen
by the service because no caller supplied anything, it never reaches a render a caller asked for, and it
is not a default. The instant SHALL be the one the request already captured, so a thumbnail reads the
clock once (`interpolation-tokens`).

An entry whose control is `select` SHALL never be invented for here, because the default option
selection supplies it: the first allowed value of every declared `enum`, which is what the frozen
contract's "default option selection is used automatically" meant before every option became an enum
parameter. That selection is preview-only and is unchanged by `param-resolution`.

Every name not invented for and not supplied by that selection SHALL take the value the service resolves
for it, which after `param-resolution` is its declared `default:` and nothing else. A name with no
declared default and no invented value is absent, and an absent name makes a gate naming it false rather
than raising.

Drawing is still gated: the renderer evaluates each item's `when:` against the placeholder label, so
one branch appears and the rest do not. Only the *filling* is ungated, and it has to be. A value the
thumbnail invents is part of the request, so it can decide a gate: a required `integer` that some
item prints and some container gates on `1`, filled with `1` because it declares no `min`, activates
that branch. Building the fill set from `inputs.default` would then leave that branch's own names
unfilled and the render would fail for missing data. Filling from `inputs.all` closes the rule under
its own injections, and it is what the walker this requirement replaces already did, since that
walker ignored gates entirely. A name only an unselected branch reads costs an unread key in the
request, which the renderer ignores.

**A gate whose value is its own parameter's name is a malformed template, and this requirement
records it rather than endorsing it.** The fill for a required, interpolated `text` or `textarea`
entry is the entry's own name, so a container gated on `mode: mode` is satisfied by the placeholder
and its branch draws, in every thumbnail, on the rule above. That is what the renderer does and this
requirement does not change it. What the shape *is*, is a gate nobody else reaches: no declared
default satisfies it unless the default is the parameter's own name, and no caller satisfies it
without typing `mode` into the `mode` field. An author who wants a branch chosen by a named
alternative declares `mode` an `enum` instead, which the thumbnail never invents for, whose value the
default option selection supplies, and which an operator can choose. Nothing in this requirement
offers `mode: mode` as an authoring form.

**Two such gates on packed siblings can fail the thumbnail, and the fault is the template's.** A
`flow` container packs its active children in order and accumulates their extents (`flow-layout`), so
two children of a 60mm-wide row that each resolve to 50mm wide and are each gated on their own name
both activate under the placeholder data, and the second one's trailing edge lands outside the padded
inner box. Under the default `overflow: fail` the thumbnail is then `422 UnsupportedLayoutItem` with
`details.reason` of `item_out_of_frame`, naming that child, on a template that renders for every
caller sending ordinary values: only a caller deliberately sending each involved parameter's own name
as its value activates both gates and reproduces the same overrun. The thumbnail SHALL NOT avoid this
by withholding a fill, by ignoring a gate, or by relaxing the `flow` overflow policy for a preview:
the fill rule is what makes a preview cover the branch its own fill activates, and a preview drawing
a layout the container refuses would show a label the renderer cannot produce. The refusal is returned
as it is for any other template, and what fixes it is the template.

Both conditions are load-bearing, because a name present in a request's `data` beats the
parameter's declared default and an invented value is rarely a legal one:

- Without (1), a gate key would be filled with the literal text of its own name. For a `string`
  parameter gating a container on its own default that silently selects the wrong branch; for an
  `enum` it rejects the render outright. `interpolated` separates a name an item prints from a name
  that only decides whether an item exists, and it is the distinction the walker this requirement
  replaces already drew by collecting only value tokens.
- Without (2), a declared parameter that resolves on its own would be overridden by a stand-in that is
  usually invalid: an `enum` printed as `{orientation}` would be filled with `orientation`, which is
  not a member of its `values`. This is the defect reported as #215 for the preview, which fills by
  the same rule through the same walker.

A name read through an interpolated `image.src` names a bundled asset rather than carrying image
bytes, so it takes the `text` fill: the entry's own name. That resolves when an asset of that name
exists under the assets root and otherwise fails with the asset error that path already returns,
unchanged from today. The thumbnail SHALL NOT invent a data URI for such a name, which could never
resolve as a path. Nothing in `catalog/` or `tests/fixtures/templates/` uses an interpolated `src`.

Variables (`{vars.X}`) SHALL resolve from the store; an undefined variable reference SHALL be `422`.

The response SHALL carry `ETag`, a quoted SHA-256 of the rendered PNG bytes so it moves with the
template, the renderer, the interpolated variables and the datetime formats alike, and
`Cache-Control: no-cache`. A caller sending `If-None-Match` with a matching ETag SHALL receive
`304 Not Modified`. Error codes are `404 TemplateNotFound` for an unknown id and `422` for
render or interpolation failures.

#### Scenario: A thumbnail draws one branch

- **WHEN** a template's `orientation` defaults to `horizontal` and its vertical branch reads
  `{tracking_url}`
- **THEN** the thumbnail renders the horizontal branch alone, though it may carry a placeholder for
  `tracking_url` in its request

#### Scenario: A placeholder that decides a gate does not strand its own branch

- **WHEN** a template declares `copies` as a required `integer` with no `min`, an unconditional `text`
  item reads `{copies}`, and a container gated on `copies: 1` reads `{subtitle}`
- **THEN** the thumbnail fills `copies` with `1` and `subtitle` with its own name, the gated container
  renders, and the render does not fail for missing data

#### Scenario: A gate naming its own parameter is activated by the placeholder, and by a caller only through the same-name value

- **WHEN** a template declares `mode` as a required `string`, an unconditional `text` item reads
  `{mode}`, and a container gated on `mode: mode` reads `{subtitle}`
- **THEN** the thumbnail fills both names and the gated container renders
- **AND** a caller's render of that template reaches that branch only by sending `mode` as the value
  of `mode`, which is what makes the gate a malformed one rather than an authoring form

#### Scenario: Two gates naming their own parameters overrun a packed row

- **WHEN** a `flow` `row` container 60mm wide packs two containers that each resolve to 50mm wide,
  each gated on its own required `string` parameter's own name and each reading that parameter
- **THEN** both children are active under the placeholder data, the second child's trailing edge falls
  outside the padded inner box, and the thumbnail is `422 UnsupportedLayoutItem` with `details.reason`
  of `item_out_of_frame` for that child

#### Scenario: A required numeric input gets a coercible sample

- **WHEN** a template declares `width` as a required `length` with `min: 10` and a `text` item reads
  `{width}`
- **THEN** the thumbnail fills `width` with `10` and renders, rather than failing for a missing or
  uncoercible value

#### Scenario: A required numeric input with no bounds still renders

- **WHEN** a template declares `scale` as a required `number` with no `min` and a dynamic `size` reads
  `"{scale}"`
- **THEN** the thumbnail fills `scale` with `1` and renders

#### Scenario: An asset that exists renders

- **WHEN** an `image` item carries `src: "{logo}"`, the template declares `logo: { type: string }` with
  no `default:`, and `logo` exists under the assets root
- **THEN** the thumbnail renders that asset

#### Scenario: A thumbnail still shows field names

- **WHEN** a thumbnail is rendered for a template reading `{title}` unconditionally, where `title` is
  declared as a `string` with no `default:`
- **THEN** the label shows the literal text `title`

#### Scenario: A thumbnail still shows an image placeholder

- **WHEN** a template binds an `image` item through `name: "logo"`
- **THEN** the thumbnail renders the 1×1 PNG rather than the text `logo`

#### Scenario: A gated thumbnail is not broken by its own gate key

- **WHEN** a template gates two containers on an `enum` `orientation` defaulting to `horizontal`, so
  `orientation` is reported as an input with control `select`
- **THEN** the thumbnail invents no value for `orientation`, the default applies, and the horizontal
  branch renders

#### Scenario: A string gate key is not invented for either

- **WHEN** a template declares `orientation` as a `string` with `default: horizontal` and gates a
  container on `orientation: horizontal`, so its entry carries control `text` and `interpolated` false
- **THEN** the thumbnail invents no value for `orientation`, and the gated container renders

#### Scenario: A printed enum keeps its default in a thumbnail

- **WHEN** a `text` item interpolates `{orientation}` for an `enum` parameter, so its entry carries
  `interpolated` true and `required` false
- **THEN** the thumbnail invents no value for it, and the label shows the first of its `values`, supplied
  by the default option selection — which is its declared `default` wherever the two agree, and never the
  literal text `orientation`

#### Scenario: A printed enum shows the option selection where the two differ

- **WHEN** a thumbnail is rendered for a template printing `{orientation}` where `orientation` declares
  `values: [horizontal, vertical]` and `default: vertical`
- **THEN** the label shows `horizontal`, because the default option selection is merged into the request
  data before any declared default is consulted

#### Scenario: A sheet template previews one slot

- **WHEN** a thumbnail is rendered for a `sheet` template
- **THEN** one label slot is rendered, not a full sheet

#### Scenario: A matching ETag returns 304

- **WHEN** a caller sends `If-None-Match` carrying the ETag of the current thumbnail
- **THEN** the response is `304 Not Modified`

#### Scenario: An undefined variable still fails

- **WHEN** a thumbnail is rendered for a template reading `{vars.missing}` with no such variable
  stored
- **THEN** the response is `422`

#### Scenario: A broken default is invented for rather than failing the preview

- **WHEN** a thumbnail is rendered for a template declaring `title: { type: string, default: "{vars.base}" }`
  with no `base` stored, where an unconditional `text` item reads `{title}`
- **THEN** the entry is `required: true`, the thumbnail fills `title` with its own name and renders,
  while a caller's render of the same template omitting `title` is still
  `422 TemplateInvalid` with reason `param_default_unresolvable`

#### Scenario: A joined list is filled with its own name

- **WHEN** a thumbnail is rendered for a template whose active layout prints `{tags:join(', ')}` and
  `tags` declares `type: list` with no `default:`
- **THEN** the thumbnail renders and reads `tags`

#### Scenario: A list declaring a resolvable default is not invented for

- **WHEN** the same template declares `tags: { type: list, default: [CONSUMABLE, KIDS] }`
- **THEN** the thumbnail reads `CONSUMABLE, KIDS`, because the service has a value for that parameter

#### Scenario: An empty list default renders empty

- **WHEN** the same template declares `default: []`
- **THEN** the thumbnail renders with that text empty, because `[]` is a value the service has and not
  an absence it stands in for
### Requirement: One derivation serves the thumbnail and the catalog index

The placeholder data a thumbnail renders from, and the field list the catalog index publishes, SHALL
both be derived from the same walk that builds an input list, so no second field walker exists in the
service.

The catalog index's field list SHALL be the names in `inputs.all` whose `required` is true, which
preserves its rule that an entry advertises only what a caller must supply.

The index is generated outside any install, so there is no variables store and no stored
`datetime_formats` to resolve a declared default against. It SHALL therefore resolve against an **empty**
variables set, the built-in `datetime_formats`, and one instant captured for the run. A default naming
`{vars.…}` does not resolve there, so its parameter is `required` and the index lists it, which is what
is true of an install that has not set the variable; a default resolving from `{sys.…}` alone does
resolve, so its parameter is not listed. There is no third derivation: this is the same resolution the
endpoints run, given the context this caller has.

#### Scenario: A catalog entry lists a parameter whose default needs a variable

- **WHEN** a catalog template declares `url: { type: string, default: "{vars.base}" }` and an active
  item reads `{url}`
- **THEN** the index lists `url` as a field, because no install's variables are known when the index is
  generated

#### Scenario: The catalog index still lists only supplied fields

- **WHEN** a template reads `{vars.base_url}`, `{printed_on}` for a declared `datetime` parameter, and
  `{id}`
- **THEN** the catalog index lists `id` and `printed_on`, because a `datetime` declaring no `default:`
  is now `required` like any other undefaulted parameter, and `{vars.base_url}` is not a request field

### Requirement: A screen renders the reported inputs and decides nothing else

A screen that collects label data SHALL render exactly the entries the service reports for the label
it is about to submit, using each entry's `control` and seeding it from `default`, and SHALL treat a
label as incomplete exactly when some entry marked `required` has no value. It SHALL NOT inspect the
template's layout, evaluate a `when:` condition, or normalize a value in order to decide any of this.

Requiredness SHALL be read from `required` and SHALL NOT be re-derived from whether the entry carries a
`default`. The two agreed while `required` meant `default.is_none()`; they no longer do, because a
parameter whose declared default failed to resolve carries no `default` and is `required: true`, and a
screen that inferred requiredness from the absent `default` would reach the same answer for the wrong
reason and would diverge the moment either rule moves.

The value a screen seeds is the published `default` **as published**. A screen SHALL NOT reshape it, and
SHALL NOT read a default out of the raw parameter declaration to seed with. One adaptation is permitted
and only one, because the control cannot hold the published form: a `datetime` entry seeded into a
date-and-time control SHALL widen a bare `YYYY-MM-DD` to `YYYY-MM-DDT00:00`, which names the same instant
the service resolved. The published value itself is unchanged by the widening, so what the screen shows
beside the entry and what `param_defaults` reports stay the one value, in the one form the render path
produced.

**An entry carrying `default_error` is an entry with no usable default.** Its control SHALL be empty, it
SHALL be treated as required — which is what `required` already says — and the screen SHALL surface the
error's `message` against that entry when the list arrives, so an operator is told what is wrong rather
than left with a blank control and no reason. The operator SHALL still be able to supply a value and
print: the template's broken default is not a reason to block a label that never needed it.

An entry with no `default` SHALL be rendered in a visibly unset state, distinguishable from any value the
control could hold. This is newly load-bearing: a `checkbox` and a `select` could previously not be unset,
because the service published `false` and the first allowed value for every one, and `param-resolution`
stops it doing so. An unset `checkbox` SHALL NOT read as `false` and an unset `select` SHALL NOT read as
its first option; both SHALL show that nothing is chosen, and SHALL be flagged incomplete under the rule
above until an operator chooses. A range presentation cannot express "nothing is chosen" at all — a slider
always sits somewhere — so an entry with no `default` SHALL NOT be presented as one, whatever its `slider`
flag says; it takes the plain numeric control until it has a value.

**Deferring to a declared default.** The print form SHALL offer, for every entry carrying a `default`,
a checkbox by which the operator says that the template decides this one. Its visible label SHALL read
`Use default` and SHALL name the entry's **published** `default` beside it, as text. Its accessible name
SHALL contain the entry's `name`, which is unique within a list, so two entries are distinguishable even
when they share a `description` and a default; it MAY also carry the description and the default. The
checkbox SHALL NOT share a label element with the entry's value control. The checkbox SHALL be checked
whenever the entry first appears.

**Deferral changes what a screen submits, and nothing else.** While an entry is deferred its value
control SHALL be disabled and its name SHALL NOT appear in the `data` the screen submits, so the service
resolves it under `param-resolution`.

What that disabled control *displays* is whatever the seeding rule above already puts there, and a
published default is now a value that control can hold: it is post-coercion, so `"80mm"` reaches a
`number` control as `80`, and a `datetime` reaches a date-and-time control widened by the one adaptation
above. The exception is `image`, whose value is a data URI and whose control is a file chooser the
browser owns; a published default for one is named by the checkbox's label and shown nowhere else.

The checkbox's label names the entry's published default as text, which every control's value can be
rendered as. What it names is the value the render path resolved **for the snapshot the list was
computed against**, which is a far stronger claim than the declared text it named before, and still not a
promise about the print: a default reading `{sys.now}` or a variable re-resolves against the render's own
snapshot, so a screen left open across midnight, or across an edit to the variables store, may print a
value the label no longer names. An entry whose declared default failed to resolve publishes none, so it
offers no checkbox at all.

Because deferral never empties a control, it cannot make an entry incomplete, and the meaning of
`required` is untouched by this rule.

Clearing the checkbox SHALL enable the control and SHALL leave whatever the seeding rule gave it in
place; whatever it then holds is submitted like any other value, including a value equal to the default.
Re-checking SHALL restore deferral and SHALL discard any value entered while it was cleared, returning
the control to the state the seeding rule defines for it; for a control the browser owns rather than the
screen, such as the file chooser an `image` entry renders, re-checking SHALL also clear that chooser's
own selection, so a shown filename cannot outlast the value it stood for.

Deferral is offered for every `control`, and for most of them it is the only gesture that reaches
omission at all. The pruning rule below drops an empty value for `integer`, `number`, `select`,
`checkbox`, `date` and `datetime`, but a screen can only send an empty value for a control that can be
emptied: a `checkbox` presentation toggles between two booleans, a `select` offers only its declared
options, and a bounded numeric entry is a slider that always sits somewhere. Emptying is therefore a
real gesture on an unbounded numeric entry and on a `date` or `datetime` control, and on nothing else.
For every remaining control, including `text`, `textarea` and `image` where an empty value is a value,
the checkbox is the first way an operator can say it.

The CSV import grid and the connector grid SHALL NOT offer deferral, and SHALL keep seeding and
submitting each entry's `default`, which is now the resolved value rather than the declared text — so a
cell seeded from a `length` declaring `"80mm"` holds and submits `80`, which the row could not submit
before. #242 tracks the deferral affordance for them.

The print form SHALL render `inputs.default` for its first paint and SHALL then request a list for
the label it would actually submit, before treating that label as complete. "Would actually submit"
means the same map submission would carry: the values it holds, pruned by the rules below, and without
any name it is deferring. A deferred name reaches that derivation as an omission, which is what it will
be at render time, so the branch the list reports is the branch the render takes. That now holds for a
tokened default too, **as long as it resolves**: the derivation resolves one against the same snapshot a
render would, so it can no longer report a branch the render will not take.

Parity stops at a default that does **not** resolve, and the two paths part company by design rather than
by accident. The read-only list absorbs the failure: the parameter is absent, a gate naming it is false,
and the response is `200`. A render does not absorb it: it resolves every declared parameter before it
evaluates any `when:`, so a request that omits that parameter fails with
`422 TemplateInvalid` / `param_default_unresolvable` and never reaches a branch at all
(`param-resolution`). The list is therefore reporting what the operator must fill in, not predicting a
render that will succeed — and once the operator supplies that parameter, the default is never reached
and the render takes the branch the list reported. What still requires the re-request is that the branch
depends on the values the form holds, which change as the operator types.

It SHALL request a fresh list when a value changes, debounced, and SHALL keep rendering the previous
list until the new one arrives, so controls do not flicker while the operator types.

A grid is the one screen whose columns and whose controls are not the same set. Its **columns** are
the union of the names across the rows present, so the table has a stable shape while rows select
different branches. Its **cells** follow each row's own list: a cell whose name is not in that row's
list SHALL be inert, meaning not editable, not validated, and not submitted for that row. A value the
cell held before the row deactivated the name SHALL be retained and SHALL become editable again when
the name returns to the row's list. That is how a union column and a per-row list coexist without
either rule bending.

A grid cell's **editor** follows that same reported `control`. A cell whose control is `textarea`
SHALL be edited in a control that accepts a line break, and its keys SHALL be: Enter commits the
edit, Shift+Enter inserts a newline, Escape abandons it, and moving focus away commits. Enter SHALL
NOT insert a newline. A grid owns Enter for commit, an operator reaches for it out of habit, and one
that silently broke the line instead would cost a row of typing with no way back. A newline the
operator enters SHALL reach the submitted `data` unaltered.

A cell SHALL show that its value holds a line break rather than rendering it as one collapsed line,
so that a two-line value is distinguishable from the same words written with a space, and so that an
operator can tell the cell holds more than the row has room to show. This applies to every cell whose
value holds a newline, not only to one whose control is `textarea`: a CSV import and a connector both
deliver such a value into a cell the operator never opens.

No screen SHALL treat a label as complete, or allow it to be submitted, while the list for that
label's current values has been requested and not yet received. A stale list would otherwise report
one branch's names as satisfied while the render followed another. The only exception is the failure
path below.

The CSV import grid and the connector grid SHALL request lists for their rows in one request, and
SHALL block a run while any row's list is unresolved. The grid's columns are the union of the names
across the rows present.

The template preview fills sample values by the thumbnail's rule with one deliberate difference, over
the same `inputs.all` set and for the same reason: a sample value is part of the request and can
decide a gate, so a set drawn from `inputs.default` would not cover the branch its own samples
activate. The difference is the `select` control. A thumbnail leaves one to the default option
selection it passes alongside the data,
which covers **every declared `enum`** whether or not a token reads it. A client has no option map to
pass, so it SHALL put the first allowed value in the request `data` instead, and SHALL do so on the same
terms — for every declared `enum` in `inputs.all`, including one only a `when:` key names, which carries
`interpolated: false` and which the fill rule would otherwise skip. A `date` or `datetime` entry it fills
SHALL carry an RFC 3339 value with an explicit offset or `Z`, not a bare date and not an offset-free
local spelling: a bare date parses as midnight, and an offset-free spelling is read as *server*-local
while a browser builds it from browser-local parts, so only an offset-bearing value names the same
instant on both sides.

**A sample equal to its own parameter's name decides a gate, and such a gate is a malformed
template.** The sample for a required, interpolated `text` or `textarea` entry is the entry's own
name, so a container gated on `mode: mode` is satisfied by the preview's own sample and its branch
draws. The fill rule is unchanged by this and SHALL stay the thumbnail's. What the shape *is*, is a
gate no operator reaches without typing `mode` into the `mode` field, and an author who wants a branch
chosen by a named alternative declares `mode` an `enum`, whose value the fill rule takes from `values`
rather than inventing and which a screen offers as a choice.

**The preview does not work around what such a gate can cost.** Two containers gated on their own
names and packed as siblings of a `flow` container both activate under the sample data and accumulate
past the padded inner box, so the render the preview requests fails, on a template that renders for
every caller sending ordinary values: only a caller deliberately sending each involved parameter's own
name as its value activates both gates and reproduces the same overrun. **How it fails depends on the
endpoint the preview used**, which is decided by the template's format and not by this rule. A `single`
template's preview renders through `POST /api/render/label` and fails with `422 UnsupportedLayoutItem`
and `details.reason` of `item_out_of_frame` (`flow-layout`). A `sheet` template's preview renders
through `POST /api/batch`, which captures that same refusal as a row failure rather than raising it, so
the response is `422 BatchInvalid` carrying `details.failures` and no top-level `details.reason`. A
screen SHALL NOT depend on either shape to detect the condition, and a screen that reports only the
message it was given reports the batch message in the sheet case. A screen SHALL NOT withhold a
sample, drop a gate key, or request a relaxed overflow policy to avoid it, and SHALL surface that
failure as it surfaces any other failed render: the template is what is wrong, and hiding the refusal
would show a label the renderer cannot produce.

The Connect field-mapping palette SHALL offer the names in `inputs.all`, so a mapping can be built
before any row exists and can target a name only some branch reads.

A value already entered for a name a later list omits SHALL be retained in the screen's own state, so
that reselecting the branch restores it, and SHALL NOT be included in the `data` the screen submits.
A screen SHALL submit exactly the names in its current list, less any name it is deferring.

Deferral is per entry and SHALL follow the entry, not the position: an entry that newly appears in a
list because another value activated its branch SHALL arrive deferred if it publishes a `default`, and
an entry that leaves the list SHALL keep its deferral state, restored if it returns, on the same terms
the rule above retains its value.

Selecting a different template SHALL reinitialise **both** the screen's values and its deferral state
from the new template's `inputs.default`, overriding the retention rule above, which governs branch
changes within one template only. A name the two templates share carries nothing across: not its value,
not its deferral. Without this a shared name would display the previous template's value in a disabled
control while the render resolved the new template's default.

This is not an optimization. Rendering resolves and coerces every declared parameter before it
evaluates any `when:`, so a value that fails coercion rejects the render whether or not the item that
reads it is active. Submitting a value from a deactivated branch would therefore let a field the
operator can no longer see fail a label the form reports as complete. `docs/SPEC.md` §5's laziness
covers an *omitted* required parameter, not a *supplied* invalid one.

For the same reason a screen SHALL omit, rather than submit as an empty string, a name whose value is
empty and whose control is `integer`, `number`, `select`, `checkbox`, `date` or `datetime`. An empty
value for a `text`, `textarea` or `image` control is a value and SHALL be submitted. Both rules are
decided from `control` alone, so no screen needs the declared type to apply them.

When a request for a list fails, a screen SHALL keep the last list it received, or `inputs.all` when
it has received none, and SHALL surface the failure rather than silently blocking the operator.

#### Scenario: Switching a parameter changes what the print form asks for

- **WHEN** the operator switches `orientation` from `horizontal` to `vertical` on a template whose
  horizontal branch reads `{subtitle}` and whose vertical branch reads `{tracking_url}`
- **THEN** the form stops showing and requiring `subtitle` and starts showing and requiring
  `tracking_url`

#### Scenario: Controls do not flicker while a list is in flight

- **WHEN** the operator types into a field and a new list has been requested but not yet received
- **THEN** the form keeps rendering the previous list

#### Scenario: A preview sample that decides a gate does not strand its own branch

- **WHEN** a template declares `copies` as a required `integer` with no `min`, an unconditional `text`
  item reads `{copies}`, and a container gated on `copies: 1` reads `{subtitle}`
- **THEN** the preview fills `copies` with `1` and `subtitle` with its own name, and the preview
  renders

#### Scenario: A single template's preview surfaces the overrun two self-named gates cause

- **WHEN** a `single` template packs two containers resolving to 50mm wide as siblings of a 60mm-wide
  `flow` `row`, each gated on its own required `string` parameter's own name and each reading that
  parameter
- **THEN** the preview's render request to `POST /api/render/label` fails with
  `422 UnsupportedLayoutItem` and `details.reason` of `item_out_of_frame`, and the screen surfaces that
  failure rather than dropping a sample value or redrawing without the gate

#### Scenario: A sheet template's preview surfaces the same overrun in the batch shape

- **WHEN** the same layout is declared by a `sheet` template, whose preview renders one slot through
  `POST /api/batch`
- **THEN** the request fails with `422 BatchInvalid` carrying `details.failures`, because batch
  rendering captures the underlying refusal as a row failure, and the screen surfaces that failure on
  the same terms rather than dropping a sample value or redrawing without the gate

#### Scenario: A grid cell for a name inactive on its row is inert

- **WHEN** the grid shows a `subtitle` column because one row selects `orientation: horizontal`, and
  another row selects `vertical`, for which `subtitle` is not reported
- **THEN** that row's `subtitle` cell is not editable, is not validated, and is absent from the `data`
  that row submits

#### Scenario: An inert cell keeps its value and comes back

- **WHEN** a row holding a `subtitle` value switches to `orientation: vertical` and then back to
  `horizontal`
- **THEN** the cell is inert in between, and the value is there and editable again afterwards

#### Scenario: A pending list blocks submission

- **WHEN** the operator switches `orientation` from `horizontal` to `vertical` and the list for the
  new values has not yet arrived
- **THEN** the form is not submittable until it does, so no label is sent against the horizontal
  branch's completeness check

#### Scenario: A value survives switching away and back

- **WHEN** the operator enters a value for `subtitle`, switches `orientation` to `vertical`, then
  switches back
- **THEN** the value entered for `subtitle` is still there

#### Scenario: Two grid rows selecting different branches require different inputs

- **WHEN** one import row sets `orientation = horizontal` and another sets `orientation = vertical`
- **THEN** the first row is invalid only for a missing `subtitle` and the second only for a missing
  `tracking_url`, while the grid shows a column for each

#### Scenario: An integer and a number are distinguishable controls

- **WHEN** a template declares `copies` as an `integer` with `min: 1` and `max: 9`, and `scale` as a
  `number` with no bounds
- **THEN** the `copies` entry carries control `integer` with `slider` true, and the `scale` entry
  carries control `number` with `slider` false

#### Scenario: A value from a deactivated branch is not submitted

- **WHEN** the operator enters a value for `subtitle`, then switches `orientation` to `vertical`, and
  the new list omits `subtitle`
- **THEN** the submitted `data` carries no `subtitle`, and the value is still there on switching back

#### Scenario: A cleared numeric field is omitted rather than sent blank

- **WHEN** the operator clears an `integer` control that declares a default
- **THEN** the submitted `data` omits that name, and the label renders with the declared default

#### Scenario: A cleared text field is sent as an empty string

- **WHEN** the operator clears a `text` control
- **THEN** the submitted `data` carries that name with an empty string

#### Scenario: A run waits for its rows' lists

- **WHEN** rows have been edited and their lists have not come back
- **THEN** the run is blocked until they do

#### Scenario: The mapping palette offers every branch's names

- **WHEN** a Connect mapping is built for a template whose branches read `{subtitle}` and
  `{tracking_url}`
- **THEN** both names are offered, before any row has been selected

#### Scenario: A failed request does not strand the operator

- **WHEN** the request for a fresh list fails
- **THEN** the last list received keeps rendering, the failure is surfaced, and the operator can still
  submit

#### Scenario: A newline typed into a grid cell reaches the request

- **WHEN** the operator edits a cell whose control is `textarea` and presses Shift+Enter between two
  words
- **THEN** the cell's value carries a newline at that point, and the row submits it unaltered

#### Scenario: Enter commits a grid cell rather than breaking the line

- **WHEN** the operator presses Enter while editing a cell whose control is `textarea`
- **THEN** the edit is committed and no newline is inserted

#### Scenario: Escape abandons a grid cell edit

- **WHEN** the operator types into a cell whose control is `textarea` and presses Escape
- **THEN** the cell holds the value it held before the edit

#### Scenario: A cell holding a newline is distinguishable from one holding a space

- **WHEN** one row's `message` holds `line one`, a newline, then `line two`, and another row's holds
  `line one line two`
- **THEN** the two cells render differently, and the first shows that its value continues past what
  the cell displays

#### Scenario: A declared default starts deferred and is not sent

- **WHEN** the print form first paints a template whose `title` entry publishes `default: "Untitled"`
- **THEN** `title` shows a checked checkbox whose visible label reads `Use default` and names
  `Untitled`, whose accessible name includes `title`'s own label, its value control is disabled, and
  submitting sends `data` with no `title` key

#### Scenario: Two entries sharing a description and a default stay distinguishable

- **WHEN** a template publishes `title` and `subtitle`, both with `description: "Line"` and
  `default: "Untitled"`
- **THEN** the two checkboxes have different accessible names, each containing its entry's `name`

#### Scenario: A coerced default reaches its control

- **WHEN** an entry for a `length` parameter declaring `default: "80mm"` publishes `default: 80` with
  control `number`
- **THEN** the control holds `80`, shows a checked checkbox naming it, and submitting sends no key for it

#### Scenario: An image default defers without being displayed

- **WHEN** an entry publishes a `default` with control `image`
- **THEN** it shows a checked checkbox naming its published default as text, its file chooser shows no
  selection, and submitting sends no key for it

#### Scenario: A datetime default is widened for the control that needs it

- **WHEN** an entry with control `datetime` publishes `default: "2026-09-01"`
- **THEN** the date-and-time control holds `2026-09-01T00:00`, while the checkbox's label and
  `param_defaults` both name `2026-09-01`

#### Scenario: A broken default is shown as a diagnostic, not a value

- **WHEN** an entry carries `default_error` because its declared default names an absent variable
- **THEN** the control is empty and enabled, no checkbox is offered, the error's message is surfaced
  against that entry, and the entry is flagged incomplete until the operator supplies a value

#### Scenario: An operator can print past a broken default

- **WHEN** the operator types a value into that entry's control and submits
- **THEN** the label is submitted with that value and prints, because the default is never reached

#### Scenario: Editing the template's default changes what a deferred entry prints

- **WHEN** that template's `title` default becomes `Draft`, the templates are reloaded, and the form is
  opened again with `title` left deferred
- **THEN** the label prints `Draft`, with no edit to the form

#### Scenario: Clearing the checkbox submits whatever the control holds

- **WHEN** the operator clears `title`'s checkbox
- **THEN** the control becomes editable, and submitting sends `title` as whatever it then holds

#### Scenario: Re-checking discards the edit

- **WHEN** the operator clears the checkbox, types `Kitchen`, and re-checks it
- **THEN** the control is disabled, no longer holds `Kitchen`, and submitting sends no `title` key

#### Scenario: Re-checking clears a chosen file

- **WHEN** the operator clears an `image` entry's checkbox, chooses a file, and re-checks it
- **THEN** the file chooser shows no selection and submitting sends no key for that entry

#### Scenario: Switching templates carries nothing across a shared name

- **WHEN** template A and template B both publish a `title` entry with different defaults, the operator
  edits `title` under A and then selects B
- **THEN** `title` is deferred again, its control holds nothing carried from A, and submitting sends no
  `title` key

#### Scenario: A text entry can defer, which emptying it cannot express

- **WHEN** an entry with control `text` publishes a `default` and is left deferred
- **THEN** the submitted `data` carries no key for it, rather than the empty string an emptied `text`
  control would submit

#### Scenario: The list request omits a deferred name

- **WHEN** the form requests a list for a label with `orientation` deferred
- **THEN** the request body's `data` carries no `orientation` key, and the returned list is the one for
  the branch the declared default selects

#### Scenario: An entry appearing later arrives deferred

- **WHEN** switching `orientation` brings a `subtitle` entry publishing a `default` into the list for
  the first time
- **THEN** `subtitle` appears with its checkbox checked and its name absent from the submitted `data`

#### Scenario: An entry that leaves and returns keeps its deferral state

- **WHEN** the operator clears `subtitle`'s checkbox, switches the branch away and back
- **THEN** `subtitle` returns with its checkbox still cleared

#### Scenario: An entry with no published default offers no deferral

- **WHEN** an entry publishes no `default`, whether the parameter declares none or its declared default
  failed to resolve
- **THEN** no checkbox is rendered for it, and it is required and unset like any other entry with no
  usable default

#### Scenario: A tokened default now seeds its control

- **WHEN** the print form loads a template declaring `url: { type: string, default: "{vars.base}" }` and
  the store holds `base = https://example.test`
- **THEN** the control holds `https://example.test`, the checkbox is checked and naming it, and
  submitting sends no `url` key so the service resolves it again

#### Scenario: A grid keeps seeding and submitting

- **WHEN** the CSV import grid and the connector grid render a column whose entry publishes a `default`
- **THEN** each cell is seeded with that default and submitted, and neither grid offers a checkbox

### Requirement: The CSV Import screen offers the parameters the chosen template can read

*This requirement supersedes the paragraph of `docs/SPEC.md` "CSV import" describing the web UI's CSV
Import screen (the paragraph beginning "The web UI's CSV Import screen") and restates its complete
post-change contract. The rest of that section, covering `POST /import/csv`, is unchanged and remains
authoritative.*

The web UI's CSV Import screen (`/import`, ADR-0014, ADR-0022, ADR-0055) is a client-side path
separate from `POST /import/csv`: it parses and edits the CSV in the browser and posts resolved
labels to `POST /api/batch`. It does not call `/api/import/csv`, which remains the self-contained
automation endpoint.

A CSV MAY be loaded before any template is chosen: data columns show, and parameter columns and
validation activate once a template is selected. The loaded CSV SHALL survive switching templates,
including values for columns the newly chosen template does not read.

Every parameter the chosen template can read SHALL be available for mapping, with batch-default
fallback controls seeded from each entry's `default`. A parameter is available for a row when the
service reports it as an input for that row; a parameter the template declares but never reads, and
one read only inside a branch the row's own values deactivate, SHALL NOT be offered for that row.

#### Scenario: A CSV loads before a template is chosen

- **WHEN** a CSV is loaded with no template selected
- **THEN** its data columns show, and no parameter column or validation is active

#### Scenario: Every parameter the template reads is offered

- **WHEN** a template is chosen whose parameters are all read unconditionally
- **THEN** every one of them is available for mapping, with its batch-default fallback control

#### Scenario: A parameter the template never reads is not offered

- **WHEN** a chosen template declares a parameter that no item, condition or attribute reads
- **THEN** that parameter is not offered for mapping

#### Scenario: A CSV value survives a template switch

- **WHEN** a CSV carrying a column the newly chosen template does not read is loaded and the template
  is switched
- **THEN** the column's values are retained

### Requirement: The template detail reports what each declared default resolves to

`GET /api/templates/{id}` SHALL carry `param_defaults`: an object keyed by parameter name, holding one
entry for **every parameter the template declares a `default:` for** and no entry for any other. An
absent key therefore means "this parameter declares no default", and never "this endpoint did not
resolve one". Every other response carrying a template-detail body SHALL carry it on the same terms.

The report SHALL be keyed on the template's declared parameters, which is the set the render path
resolves, and not on either input list. A render resolves every declared parameter before it evaluates
any `when:`, so a default that cannot be resolved fails that render whether or not an item reads the
parameter, and whether or not the branch that reads it is active. An input list holds an entry only for
a name the layout collects, so a report keyed on inputs would omit precisely the two cases whose failure
is hardest to find otherwise: a parameter no branch reads, and one only a branch the current values
deactivate reads.

Each entry SHALL carry exactly one of:

- `resolved`: the value the render path would use for this parameter had the label omitted it — the
  declared default interpolated against this request's snapshot and then coerced, which is the same
  value the entry's `default` in an input list carries;
- `error`: `{ reason, message, token?, value? }`, where `reason` is `param_default_unresolvable`,
  `message` is the message the render path reports for that failure, `token` names the token that could
  not be resolved where one did, and `value` names the resolved value the declaration forbids where
  there is one. It is the **read-only projection** of the payload `param-resolution` defines: it shares
  every string with the render path's `422` for the same failure, holds the message as its own field
  because it has no error envelope to hold it, and omits `param`, which the key this object hangs under
  already names. It is byte-identical to the `default_error` an input list publishes for the same
  parameter in the same response.

`resolved` and `error` SHALL come from one resolution per request, projected into `param_defaults`, into
`inputs.default` and into `inputs.all` alike. No published field SHALL be computed independently of that
one: a report disagreeing with what the render path uses is worse than no report, because it shows an
operator a value the printer will not use. This binds what is *published*. A path that resolves a default
again to decide which entries are active is not in breach, because it resolves the same declaration
against the same captured context and so cannot reach a different value.

The response SHALL be `200` whether or not every default resolved. A template with an unresolvable
default is not a broken template: it renders for any caller who supplies the parameter, it is not
quarantined, and the rest of the detail body is exactly what it would otherwise be.

Resolving the report costs one read of the variables store and one of the effective `datetime_formats`
per request. A failure of either read is a `500 Internal` failure of the request, on the same terms the
thumbnail handler already reads them; it is not reported as an unresolvable default, because nothing was
resolved.

**On a request that writes, that read SHALL happen before the write.** A response of this shape is also
what creating a template, replacing one and moving one between groups return, and each of those mutates
the templates directory before it builds the detail. A request that failed to read the store *after*
mutating would report `500` for work that had already landed, leaving a caller unable to tell a refused
write from a completed one. Every such handler SHALL therefore capture its resolution context — the
variables, the formats and its instant — before it mutates anything, so a store failure refuses the
request while nothing has changed, and SHALL build the report from that captured context, which cannot
fail again.

The guarantee is bounded to what this change adds: **capturing the resolution context and building
`param_defaults` SHALL NOT introduce a failure that can follow a mutation.** It is not a claim that a
write is transactional. A write path already does fallible work after the file lands — reloading the
registry, confirming what was written, reading the moved file back — and any of those may still fail
after a successful mutation, exactly as they do today. This requirement neither adds to that set nor
removes from it.

This says nothing about *when* the resolution runs: the report describes the template as written, so it
SHALL be computed from the template the request published, against the context captured before it.

#### Scenario: Every declared default is reported and nothing else is

- **WHEN** a template declares `title` with a `default:`, `subtitle` with none, and `mode` with a
  `default:` no layout item reads
- **THEN** `param_defaults` holds entries for `title` and `mode`, and no key for `subtitle`

#### Scenario: A resolvable default reports its value

- **WHEN** a template declares `url: { type: string, default: "{vars.base}" }` and the store holds
  `base = https://example.test`
- **THEN** `param_defaults.url` carries `resolved: "https://example.test"` and no `error`, and
  `inputs.default`'s `url` entry carries the same value as its `default`

#### Scenario: An unresolvable default reports its diagnostic and still returns 200

- **WHEN** the same template is read with no `base` in the store
- **THEN** the response is `200`, `param_defaults.url` carries an `error` whose `reason` is
  `param_default_unresolvable` and whose `token` names `vars.base`, and the `url` entry in `inputs`
  carries no `default`, `required: true`, and the same payload as its `default_error`

#### Scenario: A parameter no layout item reads is still reported

- **WHEN** a template declares a parameter with a broken default that no item, condition or attribute
  reads
- **THEN** `param_defaults` carries its `error`, and no input list holds an entry for it

#### Scenario: The report is post-coercion

- **WHEN** a template declares `width: { type: length, default: "80mm" }` and
  `bold: { type: boolean, default: "yes" }`
- **THEN** `param_defaults.width` carries `resolved` equal to the number `80`, and `param_defaults.bold`
  carries an `error` whose `value` names `yes`

#### Scenario: The list endpoint is unchanged

- **WHEN** a client reads `GET /api/templates`
- **THEN** each summary carries the declared `params` exactly as before, with no report and no resolved
  default

#### Scenario: A store failure refuses a write rather than following it

- **WHEN** the variables store cannot be read during a `POST` or `PUT` that would write a template
- **THEN** the response is `500 Internal` and no template file has been written, moved or replaced

#### Scenario: Building the report cannot fail after a write lands

- **WHEN** a `PUT` replaces a template, the write succeeds, and the steps that already follow it succeed
- **THEN** the response is the detail body for the template as written, carrying its `param_defaults`,
  built from the context captured before the write and needing no further store read

#### Scenario: The write path's existing post-write failures are untouched

- **WHEN** a step that already runs after the file lands fails — the reload, the written-template
  confirmation, or reading the moved file back
- **THEN** the request fails as it does today, and adding `param_defaults` neither prevents that nor
  adds a failure of its own to that set

### Requirement: The template page shows the declared default and what it resolves to

The web UI's template page documents a template rather than collecting a label, so it SHALL show, for
every parameter declaring a `default:`, both the text the author wrote and what that text resolves to:
the `resolved` value from `param_defaults`, or the `error`'s message where it failed. The author's text
SHALL stay visible in both cases, because the page's subject is the template; the report is what says
whether it works.

A parameter declaring no `default:` SHALL show neither, exactly as it does today.

#### Scenario: A resolvable default shows both forms

- **WHEN** the page renders a parameter declaring `default: "{vars.base}"` and the store holds `base`
- **THEN** it shows `{vars.base}` as the declared default and the resolved value beside it

#### Scenario: A broken default shows the author's text and the diagnostic

- **WHEN** the store holds no `base`
- **THEN** the page still shows `{vars.base}` as declared, and shows the failure's message in place of a
  resolved value

#### Scenario: A parameter with no default shows nothing new

- **WHEN** the page renders a parameter declaring no `default:`
- **THEN** it shows no declared default and no resolved value
