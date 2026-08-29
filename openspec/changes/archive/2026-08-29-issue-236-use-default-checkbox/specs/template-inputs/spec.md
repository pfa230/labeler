## MODIFIED Requirements

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
