## MODIFIED Requirements

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
otherwise, `false` for `checkbox`, the request's captured instant for `date` and `datetime`, and the
first of the entry's `values` for `select`. Only the `select` fill carries a further condition, stated
below.

The numeric case has to be filled and has to be filled with a *number*. A required `length`,
`number` or `integer` resolves to nothing when omitted — as, after `param-resolution`, does every other
type without a declared default — so leaving it empty makes an active `{width}` token or a dynamic `size`
fail with `MissingField`. Filling it with the entry's own name, which is what the walker this requirement
replaces does, fails coercion instead. A declared bound, or `1`, is coercible and inside any declared
range.

A parameter whose declared default **fails to resolve** is `required: true` by that rule, so for
every control but `select` the thumbnail SHALL invent for it on exactly the terms it invents for a
parameter declaring no default at all. A `select` carries one further condition, stated and paid for
below, and it is the whole of the difference: eligibility is `interpolated` and `required`, and a
`select` additionally requires that the parameter declare no `default:`. So a broken default is masked
by a placeholder on every other control, and propagates on a `select`. The preview of such a template
therefore renders where a caller's render of it would be `422 TemplateInvalid`. On those controls that
is the uniform rule applied and not a carve-out: every value a preview prints is one the service
chose, a preview never reaches a render a caller asked for, and it has never claimed that a caller's
render would succeed. What says the default is broken is `param_defaults`, which
is on the same response the catalog grid already reads.

`checkbox`, `date` and `datetime` join that list because `param-resolution` removed the fallbacks that
used to make them resolve on their own. A `boolean` without a declared `default:` is no longer `false`
and a `datetime` without one is no longer the render instant; each is now `required: true`, so a
thumbnail that did not invent for them would fail with `MissingField` on every template that reads one.
What they get is a preview-only placeholder on exactly the terms `text` gets its own name: it is chosen
by the service because no caller supplied anything, it never reaches a render a caller asked for, and it
is not a default. The instant SHALL be the one the request already captured, so a thumbnail reads the
clock once (`interpolation-tokens`).

An entry whose control is `select` SHALL be invented for only where the template declares **no**
`default:` for it, and the invented value SHALL be the first of the entry's `values`, which is
non-empty for every template that loads. An `enum` that declares a `default:` is therefore never stood
in for: the default is resolved and shown, and one that cannot be resolved fails the thumbnail with
`param_default_unresolvable` naming the parameter, on the same terms a caller's render of that
template fails. That is where a `select` parts company with the other controls, which are stood in for
whether or not their declared default resolves. Nothing supplies a value for a declared `enum` alongside the
placeholder data, and no preview-only selection covers one a token never reads.

**Why `select` alone carries that condition.** Every other fill announces itself: the literal text
`title` where a title belongs, a 1×1 transparent PNG, `false`, `1`. The first of an `enum`'s `values`
is a legal value of that parameter and announces nothing, so inventing one for a **broken** default
would restore the first-value stand-in this requirement drops, and would restore it for the one
class of template whose author asked for something the service could not deliver: the catalog would
show a thumbnail no reader can tell from a healthy template's. `param_defaults` reports the broken
default on the response the grid already reads, and the picture beside it would still show a label the
template does not describe. This is the whole of the departure, and it reaches nothing but a `select`
whose parameter declares a `default:`.

An `enum` that no active item prints is not invented for, by condition (1) above, so one declaring no
`default:` is **absent**. A `when:` naming an absent parameter is false (`param-resolution`), so the
item it gates does not draw, and a thumbnail shows a gated branch only where the template's own
declarations select it.

Every name not invented for SHALL take the value the service resolves for it, which after
`param-resolution` is its declared `default:` and nothing else. A name with no
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
alternative declares `mode` an `enum` instead, whose value is its declared `default:` or, declaring
none, the first of its `values`, neither of which is the parameter's own name, and which an operator
can choose. Nothing in this requirement offers `mode: mode` as an authoring form.

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

- **WHEN** a `text` item interpolates `{orientation}` for an `enum` parameter declaring
  `default: vertical`, so its entry carries `interpolated` true and `required` false
- **THEN** the thumbnail invents no value for it and the label shows `vertical`, neither the first of
  its `values` nor the literal text `orientation`

#### Scenario: A printed enum shows the option selection where the two differ

- **WHEN** a thumbnail is rendered for a template printing `{orientation}` where `orientation` declares
  `values: [horizontal, vertical]` and `default: vertical`, so the first of its `values` and its
  declared default differ
- **THEN** the label shows `vertical`, because no selection is merged into the request data and the
  first of its `values` reaches nothing

#### Scenario: A printed enum declaring no default shows the first of its values

- **WHEN** a thumbnail is rendered for a template printing `{orientation}` where `orientation` declares
  `values: [horizontal, vertical]` and no `default:`, so its entry carries `interpolated` true and
  `required` true
- **THEN** the label shows `horizontal`, and the thumbnail renders rather than failing on an
  unresolved token

#### Scenario: An enum only a gate names and declaring no default leaves its item out

- **WHEN** a thumbnail is rendered for a template whose container carries `when: { outline: yes }`,
  where `outline` declares `values: [yes]` and no `default:` and no active item prints it
- **THEN** the thumbnail renders without that container, because `outline` is absent

#### Scenario: A broken enum default fails the thumbnail

- **WHEN** a thumbnail is rendered for a template declaring
  `orientation: { type: enum, values: [horizontal, vertical], default: "{vars.orient}" }`, printed by
  an active `text` item, and the store holds no `orient`
- **THEN** the thumbnail is `422 TemplateInvalid` with `details.reason` `param_default_unresolvable`
  naming `orientation`, rather than showing `horizontal`

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
activate. The difference is the `select` control. A thumbnail fills one only where the entry is
interpolated, required and its parameter declares no `default:`, taking the first of the entry's
`values`, and leaves every other `enum` to what the service resolves. A client SHALL fill more: the
first allowed value in the request `data` for **every declared `enum`** in `inputs.all`, including one
whose parameter declares a `default:` and one only a `when:` key names, which carries
`interpolated: false` and which the fill rule would otherwise skip. Those two shapes are where the two
differ, and there a template preview can show a value, or a branch, that the same template's thumbnail
does not (#343). A `date` or `datetime` entry it fills SHALL carry an RFC 3339 value with an explicit offset or `Z`, not a bare date and not an offset-free
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
