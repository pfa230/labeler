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

#### Scenario: A screen skips a control it cannot draw

- **WHEN** the CSV import grid renders the inputs for that template, having no editor for a `list`
- **THEN** it renders every other entry, renders no column for `tags`, and does not fail
- **AND** running the import, which submits through `POST /api/batch`, answers `422 BatchInvalid` with
  an entry in `details.failures` for that row carrying code `MissingField`, when the parameter declares
  no default

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

**The print form edits a `list` entry as an ordered set of element rows, and there the empty list is
both its empty value and its unset state.** That is the one exception to the paragraph above, it is
stated against that paragraph because it is that paragraph it bends, and it reaches exactly one screen.
Every rule in this block binds the print form and no other screen. A screen that draws no `list` control
holds no value for such an entry and offers no editor for it; what it does instead is the tolerate rule
in `An input list describes the controls one label needs`, which also says how the resulting run fails.
The batch grid, the CSV import grid and the connector mapping screen are those screens today.

A list's empty value is `[]`, which the service accepts as present and empty (`list-params`). The only
other thing the print form could put on the wire is an omission, which for an entry publishing no
`default` is a refused render naming that parameter — `MissingField`, in whichever envelope the
endpoint the form used carries it — and for one publishing a `default` is what the deferral checkbox
below already expresses. A distinguishable "nothing chosen yet" state would therefore name nothing the form
could send. So a `list` entry SHALL hold `[]` on the print form from the moment it appears, that
entry's editor SHALL render zero rows for it, and that `[]` SHALL be submitted like any other value.

It follows that a `list` entry SHALL never be the entry that makes a label incomplete on the print form.
It always has a value there, whatever `required` says, and `required` is unchanged by this: it still
means the service has no usable default and a screen must supply one, and `[]` supplies it.

The editor SHALL render one row per element, in order. Each row SHALL carry a single-line text control
holding that element, a control removing that element, a control moving it one position earlier and a
control moving it one position later. The editor SHALL also carry a control appending an empty element
as the new last row. The value the form holds and submits SHALL be the elements in row order, as an
array of strings.

The form SHALL NOT normalize what those rows hold: an element left empty is submitted as the empty
string, a duplicate is submitted twice, and no element is trimmed. This is the rule against normalizing
that the opening paragraph already states, and a list is where it is most tempting to break, because an
empty row reads as a mistake rather than as a value. It is a value the service accepts, and the operator
asked for it by appending the row.

The move-earlier control on the first row and the move-later control on the last row SHALL be **inert**:
activating one SHALL do nothing, and each SHALL report itself as unavailable to assistive technology. A
one-element editor therefore carries two inert move controls rather than none. While the entry is
editable they SHALL remain in the focus order, and SHALL NOT be made unfocusable in order to express
that they are inert.

That last clause is a requirement rather than an implementation detail because the obvious spelling, a
natively disabled button, cannot hold focus at all, and the focus rule below then has nowhere to put it:
an element moved into the first row would leave the operator's focus on the document.

After a move, focus SHALL follow the moved element to the same control on its new row. Activating that
control twice therefore moves one element twice, and activating it once more at the boundary moves
nothing, where without the rule the second activation would reach a different element. This is what
separates an editor reachable by keyboard from one operable by keyboard. After a removal, focus SHALL
move to the removing control of the row that took the removed row's position, or of the preceding row
when the removed row was the last, or to the appending control when the removed element was the only
one.

Every control in the editor SHALL carry an accessible name containing the entry's `name` and, for a
control acting on one element, that element's position as the editor currently shows it. The `name` is
what makes two entries sharing a `description` distinguishable, exactly as it does for the deferral
checkbox below. The editor as a whole SHALL be a group named for the entry, so a note rendered against
the entry is announced with the controls it describes.

While a `list` entry is deferred, **every** control in its editor SHALL be disabled outright, the
appending control and the inert move controls included. The rule below disables "its value control",
and for this control that is all of them; the clause keeping an inert move control focusable governs an
editable editor, and a deferred editor is not being operated, so nothing in it needs to hold focus. Its
rows SHALL show the published `default`, one row per element, under the seeding rule above and with no
reshaping.

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

`list` is in neither set, and the omission rule never reaches it: `[]` is not an empty string. A screen
holding a value for a `list` entry SHALL submit it, `[]` included, and SHALL NOT drop it — which today
is the print form, the one screen that holds one. A screen that draws no `list` control holds nothing
for the entry and so submits nothing for it, which is the tolerate rule and not this one.

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

- **WHEN** an entry with control `text` carries `default_error` because its declared default names an
  absent variable
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

#### Scenario: An untouched list entry submits the empty list

- **WHEN** the print form paints a template declaring `tags: { type: list }` whose active layout reads
  `{tags:join(', ')}`, and the operator submits without touching the editor
- **THEN** the form is submittable, the submitted `data` carries `tags` as the empty array, and the
  render is not refused for a missing `tags`

#### Scenario: Elements are submitted in row order

- **WHEN** the operator appends two elements to that editor and types `A` into the first and `B` into
  the second
- **THEN** the submitted `data` carries `tags: ["A", "B"]`

#### Scenario: An element left empty is submitted as an empty string

- **WHEN** the operator appends one element to an empty editor and types nothing into it
- **THEN** the submitted `data` carries `tags: [""]`, rather than the row being dropped or the value
  falling back to the empty array

#### Scenario: A declared list default seeds one row per element

- **WHEN** the print form paints a template declaring `tags: { type: list, default: [CONSUMABLE] }`
- **THEN** the editor shows one row holding `CONSUMABLE`, every control in it is disabled, the deferral
  checkbox is checked and names that published default, and submitting sends no `tags` key

#### Scenario: Clearing a list entry's checkbox submits what the editor holds

- **WHEN** the operator clears that entry's checkbox, removes the `CONSUMABLE` row and submits
- **THEN** every control in the editor is operable, and the submitted `data` carries `tags` as the empty
  array

#### Scenario: Moving elements changes the submitted order

- **WHEN** the editor holds `A`, `B`, `C`, the operator moves `C` one position earlier and then moves
  `A` one position later
- **THEN** the submitted `data` carries `tags: ["C", "A", "B"]`

#### Scenario: Removing an element removes it from the submitted array

- **WHEN** the editor holds `A`, `B`, `C` and the operator removes `B`
- **THEN** the submitted `data` carries `tags: ["A", "C"]`

#### Scenario: The ends of the editor offer no move but keep their place

- **WHEN** the editor holds `A`, `B`, `C`
- **THEN** the first row's move-earlier control and the last row's move-later control report themselves
  as unavailable and move nothing when activated, both are still reachable by keyboard, and the
  editor's four other move controls each move an element

#### Scenario: Focus follows a moved element to the boundary

- **WHEN** the operator moves the second of three elements one position earlier by keyboard, and
  activates the focused control again without moving focus by hand
- **THEN** the element that started second is first, focus is on that first row's inert move-earlier
  control, and the second activation moved nothing

#### Scenario: Two list entries sharing a description stay distinguishable

- **WHEN** a template publishes `tags` and `codes`, both `type: list` with `description: "Values"`, and
  each editor holds two elements
- **THEN** every control's accessible name contains its own entry's `name`, and every control acting on
  one element also names that element's position

#### Scenario: A list whose default failed to resolve is still printable

- **WHEN** a `list` entry carries `default_error` and publishes no `default`
- **THEN** its editor is empty and every control in it is operable, no checkbox is offered, the error's
  message is surfaced against the entry, and the form is submittable, sending `tags` as the empty array

#### Scenario: An undefaulted list entry appearing later arrives holding the empty list

- **WHEN** switching `orientation` brings a `tags` entry declaring `type: list` with no `default` into
  the list for the first time, and the operator submits without touching the editor
- **THEN** the form is submittable, and the submitted `data` carries `tags` as the empty array

#### Scenario: The initial list request carries the empty list for an undefaulted list entry

- **WHEN** the print form paints a template declaring `tags: { type: list }`
- **THEN** the initial list request carries `tags: []` in its `data`

#### Scenario: A grid draws no list editor and holds no value for one

- **WHEN** the CSV import grid and the batch grid render a template declaring `tags: { type: list }`
- **THEN** neither draws the editor, the CSV import grid shows no `tags` column, a batch grid cell for
  `tags` is not editable, neither screen holds or submits a value for `tags`, and neither fails
