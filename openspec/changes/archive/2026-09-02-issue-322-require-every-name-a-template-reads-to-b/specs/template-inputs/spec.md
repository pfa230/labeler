## MODIFIED Requirements

### Requirement: An input list describes the controls one label needs

An **input list** is an ordered list of entries, one per distinct name the operator may be asked for.
Each entry carries everything needed to render one control and to decide whether the label is
complete:

| Field | Meaning |
| --- | --- |
| `name` | The declared parameter the control fills, which is the request `data` key that carries it. |
| `control` | `text`, `textarea`, `integer`, `number`, `select`, `checkbox`, `date`, `datetime`, or `image`. |
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
otherwise, `false` for `checkbox`, and the request's captured instant for `date` and `datetime`.

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
