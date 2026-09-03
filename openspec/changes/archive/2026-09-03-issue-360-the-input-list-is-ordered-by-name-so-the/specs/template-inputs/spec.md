# Delta: template-inputs — declaration order and params sequence

## ADDED Requirements

### Requirement: Template params are declared as a sequence and published as an array

*This requirement is ADDED and supersedes the opening declaration/container example of
`docs/SPEC.md` §3.0 ("Parameters (`params:`)") as repartitioned from `datetime-params` and
`interpolation-tokens` (see those capabilities), and restates the sequence-declaration and wire-array
publication contract for that portion. The per-entry/type table of §3.0 remains superseded by
`datetime-params: A datetime parameter names an instant, not a rendering`, the namespace rules by
`interpolation-tokens: A bare name is a bare name, and no word is reserved`, and the top-level field
table entry for `params` remains superseded by `template-groups` as modified herein. All other frozen
sections remain authoritative.*

A template SHALL declare its parameters as a **sequence** under the top-level `params:` key, each entry carrying its name and its declaration:

```yaml
params:
  - name: title
    type: string
    default: Untitled
  - name: code
    type: string
```

`params:` SHALL be a YAML sequence. Each element SHALL be an object with a required `name` and the parameter attributes for its `type` (`type`, `default`, `values`, `min`, `max`, `multiline`, `time`, `description` as the type permits; see `datetime-params`, `list-params`). `params:` MAY be omitted, which is the same as an empty sequence, but `params: null` (explicit YAML null) SHALL be refused at load as a parse error naming the file and the `params` path, quarantining the file under `template-registry` while the service still starts; the same content arriving through a template write SHALL be refused with `422 TemplateInvalid` and `details.reason` `template_parse_failed`. A mapping-shaped `params:` (keys as names) SHALL be refused at load as a parse error naming the file and the `params` path, quarantining the file; the same content arriving through a template write SHALL be refused with `422 TemplateInvalid` and `details.reason` `template_parse_failed`.

`name` SHALL be required, non-empty, and match `^[a-zA-Z0-9_-]+$`; a value otherwise SHALL be refused at load naming the parameter entry and the file. Two entries sharing a `name` SHALL be refused at load during raw-to-domain conversion with a validation error naming the file and the duplicate name, quarantining the file; the same content arriving through a write SHALL be refused with `422 TemplateInvalid` and `details.reason` `template_parse_failed` naming the duplicate, consistently with the conversion-stage precedent (`list-params`). There SHALL be no second spelling for `params:`.

On the wire, every response carrying a template's `params` SHALL publish them as a **JSON array** of `ParamSpec` entries carrying `name`, in **declaration order** — the order the sequence declares. This applies to `GET /api/templates` (each `TemplateSummary.params`), to `GET /api/templates/{id}` (the `TemplateDetail.params`), and to every other response carrying that body (create/replace/move). An omitted or empty `params:` SHALL be published as an empty array; the field SHALL be present as `[]` and never omitted nor published as an object. The order on summary and detail SHALL be identical for one template.

Where validation or conversion would report an error for more than one parameter declaration, the error SHALL be reported for the declaration-order first such parameter. No path that iterates `params` (including `src/templates.rs:1008`, `src/convert.rs:743`, `src/render/mod.rs:230`) SHALL report errors in name order; declaration order is the only permitted order for surfacing the first error.

A template with no `params:` key, an empty sequence, or any superseded top-level spelling (`id:`, `group:`, `options:`, `container.option:`) remains governed by `template-groups` and `template-registry`; this requirement adds no new top-level key and removes `params:` map alias.

#### Scenario: Params are declared as a sequence

- **WHEN** a template file carries `params:` as `- name: title` then `- name: code`
- **THEN** the template loads, and both `GET /api/templates` and `GET /api/templates/{id}` publish `params` as `[{name:"title",...},{name:"code",...}]` in that order

#### Scenario: A mapping-shaped params is refused

- **WHEN** a template file carries
  ```yaml
  params:
    title: { type: string }
    code: { type: string }
  ```
- **THEN** the template fails to parse with an error naming the file and `params`, the file is quarantined while the service still starts, and the same content arriving through `PUT /api/templates/{id}` is refused with `422 TemplateInvalid` and `details.reason` `template_parse_failed`

#### Scenario: An explicit null params is refused

- **WHEN** a template file carries `params: null`
- **THEN** the template fails to parse with an error naming the file and `params`, the file is quarantined while the service still starts, and the same content arriving through `PUT /api/templates/{id}` is refused with `422 TemplateInvalid` and `details.reason` `template_parse_failed`

#### Scenario: A duplicate name is refused

- **WHEN** a template file carries `params:` with two entries both `name: title`
- **THEN** the template fails validation naming the file and `title`, the file is quarantined, and the same content on write is refused with `422 TemplateInvalid` and `details.reason` `template_parse_failed` naming `title`

#### Scenario: The wire order is declaration order on both endpoints

- **WHEN** a template declares `params:` in order `title`, `subtitle`, `code`
- **THEN** `GET /api/templates`'s summary for it and `GET /api/templates/{id}`'s detail both carry `params` as `[title, subtitle, code]` in that order

#### Scenario: The Parameters card follows the wire order

- **WHEN** the template of the previous scenario is shown in the UI
- **THEN** the Parameters card (`ui/src/pages/TemplateDetail.tsx:286`) renders `title`, `subtitle`, `code` in that order

## MODIFIED Requirements

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
| `interpolated` | Whether some active item reads this name **as a value**: a `text` or `qr` token, an `image` `name:`, an interpolated `image.src`, or a `repeat:` key, whose value decides how many instances of a container are drawn. False for a name present only because it gates an item or resolves a layout attribute. |
| `truncated_elsewhere` | Whether some `wrap: false` `text` item anywhere in the template, in any branch, reads this name. **The name is historical.** It meant that such an item would render only the value's first line; since #251 every segment of a value enters layout regardless of `wrap`, so the flag no longer reports a loss. It is still computed, still returned, and the print form still renders its note from it — a warning about a truncation that no longer happens. Issue #269 removes the field, the computation and the note together, because deleting a response field is a change of its own. |

An entry SHALL be present for a name the label's render will read, and for no other name. In
particular an entry SHALL be present for a parameter read only as a `when:` key, for one read
only by a layout attribute, and for one named only by a `repeat:` key, so the operator keeps the
control that selects a branch, sizes a box, or decides how many instances a strip holds.

**A `repeat:` key reads its parameter as a value, so `interpolated` is true for the name it names**,
whether or not anything inside the repeated subtree also prints the element. The line this flag draws
is not between structure and content but between a name whose absence the render survives and one
whose absence it does not: a gate naming an absent parameter is false and the label still draws
(`param-resolution`), a layout attribute resolves without one, and a `repeat:` naming an absent
parameter is `422 MissingField` (`repetition`), exactly as a token is. `interpolated` is what tells the
thumbnail below, and a client building its own preview, which names they must invent a value for, so
reporting a repeat as structural would leave a strip of fixed-content instances with no value for the
one name that decides its count: the preview would then fail with `MissingField` for a control the same
response reported. A token inside the repeated subtree that reads the bound element is an ordinary
`text` or `qr` token read of that name, and sets the same flag by the row above.

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

**A client SHALL tolerate a `control` it cannot draw.** Such a screen SHALL omit that entry's control
and SHALL NOT fail, break its layout, or drop the rest of the form. The obligation is not about any one
control and does not lapse when the control that prompted it is drawn: it is stated here rather than
left to be discovered because a screen that renders every reported entry unconditionally is the shape a
new control breaks.

`list` is still that control on three screens. The batch grid (#271), the CSV import grid (#320) and
the connector mapping screen (#348) each omit a `list` entry rather than draw one, and each has an open
issue for the spelling it lacks. The print form does draw it, under the editor rule this capability's
screen requirement states for that screen alone, so the obligation binds those three and no longer
describes every screen.

The consequence for a screen that omits the control: a `list` parameter is suppliable there only by an
API caller, so a row built on one of those three carries no value for it, and a run whose template
reads that parameter in an active item fails. **The failure arrives in the batch envelope**, because
each of those three screens submits through `POST /api/batch`: the response is `422 BatchInvalid` and
`details.failures` holds an entry for that row carrying code `MissingField` (`batch-validation`), not a
top-level `MissingField`. A resolvable `default:`, `default: []` included, avoids that. None of this
describes the print form, which draws the editor and always submits a value, `[]` included.

Entries SHALL be ordered by **declaration order**: the order the `params:` sequence declares, from first to last. There is no second ordering group: every entry names a declared parameter, so nothing is ordered by where the layout first reads it, and the layout-order bookkeeping that group needed has no other reader. Where a `params:` entry is not read by the current label, it has no entry; where it is, its position is its declaration position, regardless of name or first use.

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

- **WHEN** a template declares `params:` as `title`, `subtitle`, `code` in that order, and `code` appears first in the layout, then `subtitle`, then `title`
- **THEN** the list runs `title`, `subtitle`, `code` in declaration order, regardless of alphabetical order or layout first use

*This scenario's name records the rule it replaces. A `MODIFIED` requirement carries every scenario name the spec already has, so the name stays while the behaviour under it does not.*

#### Scenario: The print form preserves input-list order

- **WHEN** a template declares `params:` as `title`, `subtitle`, `code` in that order and `code` appears first in the layout, then `subtitle`, then `title`
- **THEN** the print form (`ui/src/pages/print/FieldForm.tsx:61`) renders the three controls as `title`, `subtitle`, `code` in declaration order, without re-sorting by name or by layout first use

#### Scenario: The Import grid preserves input-list order

- **WHEN** the same template is loaded in the Import grid (`ui/src/pages/Import.tsx:136`)
- **THEN** the grid walks the `POST /api/templates/{id}/inputs` result and renders columns or validation in `title`, `subtitle`, `code` order, without re-sorting

#### Scenario: The Connect grid preserves input-list order

- **WHEN** the same template is loaded in the Connect grid (`ui/src/pages/Connect.tsx:153`)
- **THEN** the grid walks the input list and renders or validates in `title`, `subtitle`, `code` order, without re-sorting

#### Scenario: Conversion errors surface in declaration order

- **WHEN** a template declares `params:` as `zebra: { type: string }` then `alpha: { type: integer, min: "bad" }` (a non-numeric `min` that fails conversion) in that order, so `alpha` sorts before `zebra` alphabetically but `zebra` is declared first and both would be invalid if reached
- **THEN** a more discriminating template declares `params:` as `zebra` with an invalid `type` and `alpha` with an invalid `type` where `zebra` is the declaration-order first invalid entry, and the error reported names `zebra` rather than the alphabetically first `alpha`

#### Scenario: Template validation errors surface in declaration order

- **WHEN** a template declares `params:` as `zebra` with a `format:` attribute (forbidden on every type) then `alpha` with a `format:` attribute, so `alpha` is alphabetically first but `zebra` is declaration-order first
- **THEN** the error reported names `zebra`

#### Scenario: Render-time coercion errors surface in declaration order

- **WHEN** a template declares `params:` as `zebra: { type: integer }` then `alpha: { type: integer }`, both read by active items, and a label supplies `zebra: "bad"` and `alpha: "bad"` (both fail integer coercion)
- **THEN** the first error reported names `zebra`, the declaration-order first failing parameter, not the alphabetically first `alpha`

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

#### Scenario: A repeat is an interpolated read even when nothing prints the element

- **WHEN** a template declares `tags: { type: list }` and a packed container carries `repeat: tags`
  whose only child is a `text` reading a fixed string
- **THEN** the input list holds a `tags` entry with control `list`, `required: true` and
  `interpolated: true`
- **AND** the thumbnail invents `["tags"]` for it by the rule below and draws one instance, rather
  than failing with `422 MissingField` for a name it reported

#### Scenario: A gate read stays uninterpolated

- **WHEN** that same container instead carries `when: { show: "yes" }` over a declared `show` parameter
- **THEN** `show` has an entry with `interpolated: false`, unchanged by this requirement

#### Scenario: A screen skips a control it cannot draw

- **WHEN** the CSV import grid renders the inputs for that template, having no editor for a `list`
- **THEN** it renders every other entry, renders no column for `tags`, and does not fail
- **AND** running the import, which submits through `POST /api/batch`, answers `422 BatchInvalid` with
  an entry in `details.failures` for that row carrying code `MissingField`, when the parameter declares
  no default

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
- **THEN** each summary carries the declared `params` as an array in declaration order, with no report and no resolved default

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
