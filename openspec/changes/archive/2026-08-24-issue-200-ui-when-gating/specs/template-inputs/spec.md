## Purpose

Defines the input list: the set of controls an operator must be offered for one label, derived by the
service from the template and the values the label already carries. It is the service's answer to
"what does this label still need", so that no client has to walk a layout, evaluate a `when:`
condition, or reproduce how a parameter value is coerced before it is compared.

## ADDED Requirements

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
| `truncated_elsewhere` | Whether some single-line `text` item anywhere in the template, in any branch, reads this name, so a multiline value would show only its first line. |

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
  item binds it through `name:`; otherwise `textarea` when an active `multiline` `text` item reads
  it; otherwise `text`.

These two rules are total and mutually exclusive, so a name with several uses has exactly one
`control`. A declared `string` read by a `multiline` text item but declared `multiline: false`
therefore keeps its single-line control, and `truncated_elsewhere` is what warns about the mismatch,
exactly as today.

`required` SHALL be false for a declared parameter that resolves when omitted, namely one carrying a
`default` and one of type `boolean`, `enum` or `datetime`, and true otherwise, including for every
undeclared name.

`default` SHALL carry the declared `default` when there is one, `false` for a `boolean` without one,
and the first entry of `values` for an `enum` without one. It SHALL be absent for a `datetime`, whose
value is the request's instant and whose control a client seeds from the browser
(`datetime-params`), and for any other name with no fallback.

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

- **WHEN** a template declares `title` as a `string` with `multiline: false` and a `multiline` `text`
  item reads `{title}`
- **THEN** its entry carries control `text` and `truncated_elsewhere` false, since no single-line item
  reads it

#### Scenario: An image binding overrides a string declaration

- **WHEN** a template declares `logo` as a `string` and an active `image` item carries `name: "logo"`
- **THEN** its entry carries control `image`

#### Scenario: An undeclared name read by a multiline item gets a textarea

- **WHEN** a `multiline` `text` item reads `{body}` and `body` is not declared under `params:`
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
its declared `default`, or `false` for a `boolean`, the first entry of `values` for an `enum`, and the
request's instant for a `datetime`, and gates naming it are evaluated against that. This endpoint
SHALL NOT reject a request because of a value's content.

`required` is a property of the declaration and SHALL NOT change with the value: an `enum` stays
`required: false` whether the label carries a valid value, an invalid one, or none.

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
- **THEN** the response is `200`, the list is the one for `orientation: horizontal`, and the
  `orientation` entry carries `required: false` and `default: horizontal`

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
  reads

#### Scenario: The union prefers the wider control for an undeclared name

- **WHEN** undeclared `{title}` is read by a `multiline` `text` item in one branch and a single-line
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
only for an entry satisfying all three of

1. `interpolated` is true, so some active item reads the name as a value;
2. `required` is true, so the service has no value of its own for it;

and SHALL invent by the entry's `control`: a 1×1 PNG data URI for `image`, the entry's own name for
`text` and `textarea`, and for `integer` and `number` the entry's `min` when it declares one and `1`
otherwise.

The numeric case has to be filled and has to be filled with a *number*. A required `length`,
`number` or `integer` resolves to nothing when omitted, since only `boolean` and `enum` have a type
fallback, so leaving it empty makes an active `{width}` token or a dynamic `size` fail with
`MissingField`. Filling it with the entry's own name, which is what the walker this requirement
replaces does, fails coercion instead. A declared bound, or `1`, is coercible and inside any declared
range.

An entry whose control is `select`, `checkbox`, `date` or `datetime` SHALL never be invented for; each
resolves on its own and is `required: false`. Every name not invented for SHALL take the value the
service resolves for it: a declared `default`, `false` for a `boolean`, the
first entry of `values` for an `enum`, and the current instant for a `datetime`, which is what the
frozen contract's "default option selection is used automatically" meant before every option became an
enum parameter.

Drawing is still gated: the renderer evaluates each item's `when:` against the placeholder label, so
one branch appears and the rest do not. Only the *filling* is ungated, and it has to be. A value the
thumbnail invents is part of the request, so it can decide a gate: a required `string` that some item
prints and some container gates on, filled with its own name, activates the branch it names. Building
the fill set from `inputs.default` would then leave that branch's own names unfilled and the render
would fail for missing data. Filling from `inputs.all` closes the rule under its own injections, and
it is what the walker this requirement replaces already did, since that walker ignored gates
entirely. A name only an unselected branch reads costs an unread key in the request, which the
renderer ignores.

All three conditions are load-bearing, because a name present in a request's `data` beats the
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
- Without (3), a numeric or date entry would receive text that cannot be coerced.

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
- **THEN** the thumbnail invents no value for it and the label shows the enum's default, not the
  literal text `orientation`

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
- **THEN** the catalog index lists `id` alone

### Requirement: A screen renders the reported inputs and decides nothing else

A screen that collects label data SHALL render exactly the entries the service reports for the label
it is about to submit, using each entry's `control` and seeding it from `default`, and SHALL treat a
label as incomplete exactly when some entry marked `required` has no value. It SHALL NOT inspect the
template's layout, evaluate a `when:` condition, or normalize a value in order to decide any of this.

The print form SHALL render `inputs.default` for its first paint and SHALL then request a list for
the label it would actually submit, including any value it seeded itself, before treating that label
as complete. This matters because the form seeds a `datetime` control from the browser
(`datetime-params`) while `inputs.default` was computed against the service's instant, so across a
date boundary the two can select different branches, and because a `datetime` parameter may be a
`when:` key.

It SHALL request a fresh list when a value changes, debounced, and SHALL keep rendering the previous
list until the new one arrives, so controls do not flicker while the operator types.

A grid is the one screen whose columns and whose controls are not the same set. Its **columns** are
the union of the names across the rows present, so the table has a stable shape while rows select
different branches. Its **cells** follow each row's own list: a cell whose name is not in that row's
list SHALL be inert, meaning not editable, not validated, and not submitted for that row. A value the
cell held before the row deactivated the name SHALL be retained and SHALL become editable again when
the name returns to the row's list. That is how a union column and a per-row list coexist without
either rule bending.

No screen SHALL treat a label as complete, or allow it to be submitted, while the list for that
label's current values has been requested and not yet received. A stale list would otherwise report
one branch's names as satisfied while the render followed another. The only exception is the failure
path below.

The CSV import grid and the connector grid SHALL request lists for their rows in one request, and
SHALL block a run while any row's list is unresolved. The grid's columns are the union of the names
across the rows present. The template preview fills sample values by the same rule a thumbnail uses, over the same `inputs.all`
set and for the same reason: a sample value is part of the request and can decide a gate, so a set
drawn from `inputs.default` would not cover the branch its own samples activate.

The Connect field-mapping palette SHALL offer the names in `inputs.all`, so a mapping can be built
before any row exists and can target a name only some branch reads.

A value already entered for a name a later list omits SHALL be retained in the screen's own state, so
that reselecting the branch restores it, and SHALL NOT be included in the `data` the screen submits.
A screen SHALL submit exactly the names in its current list.

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
