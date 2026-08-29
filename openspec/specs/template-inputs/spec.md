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
| `name` | The request `data` key the control fills. |
| `control` | `text`, `textarea`, `integer`, `number`, `select`, `checkbox`, `date`, `datetime`, or `image`. |
| `slider` | For `integer` and `number`, whether both bounds are declared so the control is a slider. False otherwise. |
| `required` | Whether the label is incomplete without a value. |
| `default` | The value the service would use if the label omitted this name. Absent when there is none. |
| `values` | For `select`, the allowed values in declared order. Absent otherwise. |
| `min`, `max` | For `integer` and `number`, the declared bounds. Absent otherwise. |
| `unit` | For a `length` parameter, the template's unit, for display beside the control. Absent otherwise. |
| `description` | The parameter's declared description. Absent when it declares none. |
| `interpolated` | Whether some active item reads this name **as a value**: a `text` or `qr` token, an `image` `name:`, or an interpolated `image.src`. False for a name present only because it gates an item or resolves a layout attribute. |
| `truncated_elsewhere` | Whether some `wrap: false` `text` item anywhere in the template, in any branch, reads this name. **The name is historical.** It meant that such an item would render only the value's first line; since #251 every segment of a value enters layout regardless of `wrap`, so the flag no longer reports a loss. It is still computed, still returned, and the print form still renders its note from it — a warning about a truncation that no longer happens. Issue #269 removes the field, the computation and the note together, because deleting a response field is a change of its own. |

An entry SHALL be present for a name the label's render will read, and for no other name. In
particular an entry SHALL be present for a parameter read only as a `when:` key, and for one read
only by a layout attribute, so the operator keeps the control that selects a branch or sizes a box.

A name resolved by the service SHALL NOT appear: a `{vars.<key>}` reference, a `{datetime}` or
`{datetime.<format>}` token, and any name no active item reads.

**`control` is decided by declaration first, use second**, which preserves how the print form renders
a declared parameter today:

- For a name the template declares under `params:`, `control` follows the declared type: `select` for
  `enum`; `checkbox` for `boolean`; `date` or `datetime` for `datetime` according to its `time` flag;
  `integer` for `integer`; `number` for `length` and `number`; `textarea` for a `string` declaring
  `multiline: true`, and `text` for a `string` otherwise. The one override is `image`: a `string`
  parameter that any active `image` item binds through its `name:` gets `image`, since the value it
  carries is a data URI.

  `integer` and `number` are distinct controls, not one numeric control, because they are stepped and
  parsed differently: a client steps an `integer` by 1 and reads a whole number, and steps a `number`
  freely and reads a decimal. Collapsing them would force the client back to the declared type to
  tell them apart. `slider` then says whether the control is presented as a slider, which is true
  exactly when both `min` and `max` are declared.
- For a name the template does not declare, `control` follows use: `image` when an active `image`
  item binds it through `name:`; otherwise `textarea` when an active `wrap: true` `text` item reads
  it; otherwise `text`. That a layout flag decides a text control at all is a leftover this capability
  keeps only until #269: `wrap` says whether the renderer soft-wraps a line, which is not a statement
  about what a caller types into.

These two rules are total and mutually exclusive, so a name with several uses has exactly one
`control`. A declared `string` read by a `wrap: true` text item but declared `multiline: false`
therefore keeps its single-line control. `truncated_elsewhere` still reports the reverse pairing, but
it no longer describes a loss: since #251 every `\n` segment of a value enters layout under either
control and under either flag. Only the authored `overflow` policy may then shorten a line, drop lines,
or — under `overflow: fail` — reject the render with `text_does_not_fit`; a shortened or dropped line is
marked.

`required` SHALL be false for a declared parameter that resolves when omitted, which after
`param-resolution` means exactly one carrying a declared `default:`, and true otherwise, including for
every undeclared name. Type decides nothing: a `boolean`, an `enum` and a `datetime` without a declared
default are each `required: true`, because the service no longer supplies a value for them.

`default` SHALL carry the declared `default` when there is one and its text contains neither `{` nor `}`,
and SHALL be absent otherwise. It is the value the template declares, published as the model holds it;
this capability does not canonicalise it, coerce it, or withhold one the render would reject. A default
carrying interpolation syntax is absent because no value for it exists until a request resolves it. In
both cases `required` stays as the rule above sets it, which for a parameter declaring any `default:` is
`false`.

What a client can *do* with a published default — whether it can seed the control the entry names, and
whether a default the render would reject should be withheld rather than handed over — is #262's subject.
Two shapes make that concrete and neither is settled here: a `length` declaring `default: "80mm"`, which
the render path coerces and an `<input type="number">` cannot hold as written, and a `datetime` declaring
an RFC 3339 default, which the service accepts and neither date control can display.

Entries SHALL be ordered by declared parameters first, in ascending name order, then names the
template does not declare, in the order the layout first reads them. Ascending rather than "as
written" because `params` is an ordered map keyed by name and a template's authoring order is not
retained.

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
- **THEN** its entry carries control `textarea` and `required` true

#### Scenario: An interpolated image source is an input

- **WHEN** an `image` item carries `src: "{asset_path}"` and `asset_path` is not declared under
  `params:`
- **THEN** the input list holds an entry for `asset_path` with control `text`, since the value names a
  bundled asset rather than carrying image bytes

#### Scenario: A resolved name is never asked for

- **WHEN** a `qr` item interpolates `{vars.base_url}/{id}` and a `text` item interpolates
  `{datetime.iso_date}`
- **THEN** the input list holds an entry for `id` and none for `base_url` or `datetime`

#### Scenario: A parameter the template never reads is not an input

- **WHEN** a template declares a parameter that no item, condition or attribute reads
- **THEN** no input list holds an entry for it

#### Scenario: An enum carries the value it would resolve to

- **WHEN** a template declares `orientation` with `values: [horizontal, vertical]` and
  `default: vertical`
- **THEN** its entry carries `values: [horizontal, vertical]`, `default: vertical` and
  `required: false`

#### Scenario: Entries are ordered by name, then by first use

- **WHEN** a template declares `zebra` and `alpha` and reads undeclared `{second}` before
  undeclared `{first}`
- **THEN** the list runs `alpha`, `zebra`, `second`, `first`

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

**Lenient resolution.** Resolution on this path differs from rendering in exactly one way: a value
that cannot be coerced to its declared type SHALL be treated as though the label did not carry that
name at all. Everything downstream then follows the ordinary omission rules, so the parameter takes
its declared `default` if it has one and is otherwise absent (`param-resolution`), and gates naming it
are evaluated against that — an absent parameter making its gate false rather than raising. This
endpoint SHALL NOT reject a request because of a value's content.

A default carrying interpolation syntax SHALL NOT be resolved on this path at all. Resolving one needs
the variables store and a request instant, and this derivation has neither: it is reached both from an
endpoint and from a synchronous conversion with no application state. Such a parameter is therefore
reported as `required: false` with no `default`, exactly as the requirement above says, and its value is
absent for the purpose of evaluating a gate that names it — so a branch gated on a parameter whose
default carries a token is reported inactive here while a render would resolve it and may activate it.
That divergence is the price of a list built without a store, it is bounded to gates naming a tokened
default, and closing it is #262's subject, not this capability's. It follows that no default can fail to
resolve on this path, so this endpoint has no failure to report for one.

`required` is a property of the declaration and SHALL NOT change with the value: an `enum` declaring a
`default:` stays `required: false` whether the label carries a valid value, an invalid one, or none, and
one declaring none stays `required: true` in all three cases.

Rendering is unchanged. The same value that this endpoint absorbs still fails a render, with the code
that path already returns: an out-of-range `enum` is `422 InvalidOptionValue`; an uncoercible
`integer`, `number`, `length` or `boolean` is `400 InvalidRequest`; an unparseable `datetime` is
`400 InvalidRequest` with reason `datetime_param_invalid` (`datetime-params`); and a per-label failure
inside `POST /api/batch` is reported as `422 BatchInvalid` carrying that label's own code.

#### Scenario: One request answers several labels

- **WHEN** two labels are sent, one selecting `orientation: horizontal` and one `orientation: vertical`
- **THEN** two input lists come back in that order, the first holding `subtitle` and not
  `tracking_url`, the second the reverse

#### Scenario: A blank enum falls back and still answers

- **WHEN** a label carries `orientation: ""` for an `enum` declaring
  `values: [horizontal, vertical]` and no default
- **THEN** the response is `200`, the `orientation` entry carries `required: true` and no `default`, and
  the list is computed with `orientation` absent, so a branch gated on it is reported inactive

#### Scenario: A gate on a tokened default is reported inactive

- **WHEN** a template declares `mode: { type: string, default: "{vars.mode}" }`, a container carries
  `when: { mode: full }`, and the store holds `mode = full`
- **THEN** the input list reports that container's contents as absent, because this path does not resolve
  a tokened default, while a render of the same label resolves `mode` to `full` and draws it

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

Every other response carrying the same template-detail body SHALL include them too.

An entry in `inputs.all` SHALL carry the same fields as one in `inputs.default`, decided by the same
declaration-first rule. That rule already yields one `control` per name, so branches cannot disagree
about a declared parameter. For a name the template does not declare, where different branches use it
differently, `image` SHALL win over `textarea`, and `textarea` over `text`, so the union never offers
a control that cannot hold what some branch needs.

#### Scenario: The detail lists inputs from every branch

- **WHEN** a template reads `{subtitle}` only under `orientation: horizontal` and `{tracking_url}`
  only under `orientation: vertical`
- **THEN** `inputs.all` holds both, and `inputs.default` holds only the one the default selection
  reads when `orientation` declares a `default:`; when it declares none it is absent under
  `param-resolution`, neither branch is active, and `inputs.default` holds neither

#### Scenario: The union prefers the wider control for an undeclared name

- **WHEN** undeclared `{title}` is read by a `wrap: true` `text` item in one branch and a `wrap: false`
  one in another
- **THEN** its `inputs.all` entry carries control `textarea` and `truncated_elsewhere` true

#### Scenario: Variables are listed separately

- **WHEN** a `qr` item interpolates `{vars.base_url}/{id}`
- **THEN** `variables` holds `base_url`, and neither input list holds an entry for it

#### Scenario: The addition does not disturb the rest of the body

- **WHEN** a client reads `GET /api/templates/{id}`
- **THEN** every other field of the response is unchanged, and the response still carries no
  `options` key

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
otherwise, `false` for `checkbox`, and the request's captured instant for `date` and `datetime`.

The numeric case has to be filled and has to be filled with a *number*. A required `length`,
`number` or `integer` resolves to nothing when omitted — as, after `param-resolution`, does every other
type without a declared default — so leaving it empty makes an active `{width}` token or a dynamic `size`
fail with `MissingField`. Filling it with the entry's own name, which is what the walker this requirement
replaces does, fails coercion instead. A declared bound, or `1`, is coercible and inside any declared
range.

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
thumbnail invents is part of the request, so it can decide a gate: a required `string` that some item
prints and some container gates on, filled with its own name, activates the branch it names. Building
the fill set from `inputs.default` would then leave that branch's own names unfilled and the render
would fail for missing data. Filling from `inputs.all` closes the rule under its own injections, and
it is what the walker this requirement replaces already did, since that walker ignored gates
entirely. A name only an unselected branch reads costs an unread key in the request, which the
renderer ignores.

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

- **WHEN** a template declares `mode` as a required `string`, an unconditional `text` item reads
  `{mode}`, and a container gated on `mode: mode` reads `{subtitle}`
- **THEN** the thumbnail fills both `mode` and `subtitle`, the gated container renders, and the render
  does not fail for missing data

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

- **WHEN** an `image` item carries `src: "{logo}"`, `logo` is undeclared, and `logo` exists under the
  assets root
- **THEN** the thumbnail renders that asset

#### Scenario: A thumbnail still shows field names

- **WHEN** a thumbnail is rendered for a template reading `{title}` unconditionally, where `title` is
  not declared under `params:`
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

### Requirement: One derivation serves the thumbnail and the catalog index

The placeholder data a thumbnail renders from, and the field list the catalog index publishes, SHALL
both be derived from the same walk that builds an input list, so no second field walker exists in the
service.

The catalog index's field list SHALL be the names in `inputs.all` whose `required` is true, which
preserves its rule that an entry advertises only what a caller must supply.

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

What that disabled control *displays* is whatever the seeding rule above already puts there. This
requirement adds no claim that a published default can be shown in its control, held by it, or edited
from it: that subject is reserved above, where `"80mm"` in a `length` and an RFC 3339 `datetime` are
named as unsettled, and it is #262's. What the checkbox's label gives the operator is the entry's
published default, named as text so no control need be able to hold it, and an `image` entry's file
chooser could not. The label SHALL NOT be read as naming what the label will print: the published
default is what the list carried when the screen rendered it, a published default may be one the render
rejects rather than prints, and this requirement promises only that the label names it.

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
submitting each entry's `default` as they do today. #242 tracks the affordance for them.

The print form SHALL render `inputs.default` for its first paint and SHALL then request a list for
the label it would actually submit, before treating that label as complete. "Would actually submit"
means the same map submission would carry: the values it holds, pruned by the rules below, and without
any name it is deferring. A deferred name reaches that derivation as an omission, which is what it will
be at render time, so the branch the list reports is the branch the render takes. A published default
the render would reject is not made safe by this: it resolves leniently here and strictly there,
exactly as `param-resolution` already specifies for any omitted name, and this rule claims nothing
more. This matters because the form now seeds a `datetime` control from what the list
publishes for it (`datetime-params`), which is the same value the list was computed with, so the
date-boundary divergence this rule once guarded against cannot arise. What remains, and still requires
the re-request, is that a parameter whose declared default carries interpolation syntax is absent to that
derivation and resolved by a render, so the two can select different branches, and that a `datetime`
parameter may be a `when:` key.

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
across the rows present. The template preview fills sample values by the thumbnail's rule with one deliberate difference, over the same `inputs.all`
set and for the same reason: a sample value is part of the request and can decide a gate, so a set
drawn from `inputs.default` would not cover the branch its own samples activate. The difference is the
`select` control. A thumbnail leaves one to the default option selection it passes alongside the data,
which covers **every declared `enum`** whether or not a token reads it. A client has no option map to
pass, so it SHALL put the first allowed value in the request `data` instead, and SHALL do so on the same
terms — for every declared `enum` in `inputs.all`, including one only a `when:` key names, which carries
`interpolated: false` and which the fill rule would otherwise skip. A `date` or `datetime` entry it fills
SHALL carry an RFC 3339 value with an explicit offset or `Z`, not a bare date and not an offset-free
local spelling: a bare date parses as midnight, and an offset-free spelling is read as *server*-local
while a browser builds it from browser-local parts, so only an offset-bearing value names the same
instant on both sides.

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

- **WHEN** a template declares `mode` as a required `string`, an unconditional `text` item reads
  `{mode}`, and a container gated on `mode: mode` reads `{subtitle}`
- **THEN** the preview fills both `mode` and `subtitle`, and the preview renders

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

#### Scenario: A default no control can hold still defers

- **WHEN** an entry publishes `default: "80mm"` with control `number`, and another publishes a default
  with control `image`
- **THEN** each shows a checked checkbox naming its published default as text, each control is
  disabled, and submitting sends no key for either, whatever the controls display

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
  carries interpolation syntax
- **THEN** no checkbox is rendered for it and it behaves exactly as it does today

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
