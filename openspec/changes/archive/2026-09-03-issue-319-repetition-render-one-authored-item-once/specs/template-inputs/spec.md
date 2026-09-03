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
### Requirement: The template detail carries the lists a client needs before it has a label

`GET /api/templates/{id}` SHALL include:

- `inputs.default`: the input list for a label carrying no `data`. A client renders its first form
  from this without a second round trip.
- `inputs.all`: the union of every entry any label could produce, one per distinct name, ignoring
  every `when:` condition and **walking the subtree of every `repeat:` exactly once**, whatever the
  parameter it names would resolve to for a label carrying no `data`. It is what the thumbnail and the
  template preview fill their sample values from, for the closure reason given in the thumbnail
  requirement, and what a view describing the template rather than a label reads.
- `variables`: the `{vars.<key>}` keys the layout reads, as a list of keys without the prefix,
  ascending.
- `param_defaults`: what each declared default resolves to for this request, under the requirement
  "The template detail reports what each declared default resolves to". It is keyed on the declared
  parameters rather than on either input list, so it covers a parameter no branch reads.

Every other response carrying the same template-detail body SHALL include them too, `param_defaults`
included. That is every response of this shape, not only `GET /api/templates/{id}`: creating a template,
replacing one and moving one between groups each return it, and each is read by a client that will seed
a form from it.

**The single walk is the union rule applied to a repeat, not an exception to it.** A repeat draws one
instance per element (`repetition`), so a label carrying no `data` draws none, and expanding the subtree
against that label would report a strip's contents as read by nothing at all. Some label supplies a
non-empty list, so those entries are ones a label could produce, which is what this union holds. The
consequence is what the thumbnail depends on: a template whose only read of `tags` is a repeating
container printing `{tags}` reports `tags` in `inputs.all` with `interpolated` true, the thumbnail
invents a one-element list from it by the invention table below, and the preview draws one instance
rather than an empty strip.

**`inputs.default` and `POST /api/templates/{id}/inputs` do the opposite, because they answer a
different question.** Each is computed for one label, by the rule the renderer applies, so each expands
a repeat exactly as that label's render would: a repeat over an absent or empty list contributes only
its own name, and the entries the subtree's other reads produce appear once the label carries elements.
That is what a gated branch already does to those two lists, and `inputs.all` is what a client reads to
see every control the template can ask for.

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

#### Scenario: The union holds a repeated subtree's reads

- **WHEN** a template declares `tags: { type: list }` with no default and a packed container carrying
  `repeat: tags` prints `{tags}` and `{price}`
- **THEN** `inputs.all` holds entries for `tags`, with control `list` and `interpolated` true, and for
  `price`
- **AND** `inputs.default`, computed for a label carrying no `data`, holds the `tags` entry alone,
  because that label draws no instance

#### Scenario: A label carrying elements reports the subtree's other controls

- **WHEN** `POST /api/templates/{id}/inputs` is given one label carrying `tags: ["A"]` for that template
- **THEN** its list holds `tags` and `price`

#### Scenario: The thumbnail of a repeating template draws an instance

- **WHEN** a thumbnail is rendered for that template
- **THEN** `tags` is filled from `inputs.all` with a one-element list holding its own name, and the
  preview draws one instance reading `tags`, rather than an empty strip

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
