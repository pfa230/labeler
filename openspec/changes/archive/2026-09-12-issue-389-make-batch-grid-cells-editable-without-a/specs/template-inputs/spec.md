## MODIFIED Requirements

### Requirement: A screen renders the reported inputs and decides nothing else

A screen that collects label data SHALL decide what it offers, and whether a label is complete, from
what the service reports and from nothing else. It SHALL NOT inspect the template's layout, evaluate a
`when:` condition, or normalize a value in order to decide any of this. The service publishes two
derivations of the same walk, and which one a screen reads follows from how many labels it collects at
once.

**A screen collecting one label** SHALL render exactly the entries the service reports for the label
it is about to submit, using each entry's `control` and seeding it from `default`, and SHALL treat a
label as incomplete exactly when some entry marked `required` has no value. The print form is that
screen.

**A screen collecting many labels at once** — a grid, whose columns are shared by rows that select
different branches — SHALL offer a column for every name the template detail's `inputs.all` reports,
less any entry the tolerate rule omits for that screen, whatever any row's data holds, and SHALL judge
each row against the entries the service reports **for that row**. The grid rule is stated in full below, under "A grid is the one screen whose columns and
whose controls are not the same set"; everything else in this requirement binds both shapes unless it
names one.

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
offers no editor for such an entry, and holds no value for it unless something other than the operator
put one there. One thing does: a connector row materialized with a multi-valued column mapped onto a
`list` parameter carries that column's elements as an array, shows them read-only and sends them
(`connector-multi-valued-fields`), and this requirement neither adds that nor takes it away. Every
other row on those screens — parsed from a CSV, added by hand — holds nothing for such an entry, and
what happens then is the tolerate rule in `An input list describes the controls one label needs`,
which also says how the resulting run fails. The batch grid, the CSV import grid and the connector
mapping screen are those screens today.

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
the names the template detail's `inputs.all` reports, less any entry the tolerate rule omits for that
screen — the union over every `when:` branch, which is the derivation that walks the layout with no
label at all — so the fields of every variant have a column and the table's shape does not move as rows
select different branches. Its **cells** follow
each row's own list: a cell whose name is not in that row's list SHALL be neither validated nor
submitted for that row, and SHALL be **editable** on the same terms as a cell whose name that list
does report, taking the `control` its `inputs.all` entry publishes. The tolerate rule still governs a
control the grid cannot draw, so a `list` entry is no more editable in a column no row activates than
in one every row does. A value a cell holds SHALL be retained whether or not the row's list reports
its name, and SHALL be submitted once the row's own values activate the branch that reads it. That is how a union
column and a per-row list coexist without either rule bending: the column set says what the template
can ask for, and the row's own list says what that row must supply and what it sends.

Column order differs by grid, and neither grid's order is changed by where its names now come from.
The connector grid SHALL order its columns as `inputs.all` publishes them, which is declaration order
under this capability's ordering rule and is the order a per-row list publishes too.

The CSV import grid SHALL order its columns as the loaded CSV's headers, in header order, followed by
every parameter column that CSV has no header for, in `inputs.all` order. That is the order it renders
today and the union does not disturb it: a loaded column keeps the position the file gave it, so the
grid reads in the shape of the sheet the operator loaded, and the parameter columns the file does not
carry are appended rather than interleaved. Declaration order therefore governs the appended group and
not the whole row of headings. It is stated here because the union changes which names land in that
appended group, and a rule reading "columns follow `inputs.all` order" would have made every CSV a
re-ordering of the operator's own file.

Deriving the columns from the rows instead is what #385 records. A row arrives with its gating
parameter unset — a connector row whose mapping leaves it blank, an `enum` declaring no default — so
no row activates any branch, and every field behind that gate loses its column at once, on every row.
The operator is not shown a blank field to fill: they are shown no field, and nothing says one exists
or that the gate's own column is what would reveal it. A gate decides what renders on a label; a bulk
editor asks a different question, and what an operator may type into a grid is not the gate's to
decide.

A grid SHALL NOT distinguish a column, or a cell, by whether a row's list reports its name. Reporting
is per row and a column is not, so a mark on the column would be wrong for every row that does select
that branch; and a mark on the cell would say only what the row's own values already say. This is a
decision, not an omission: giving such a cell a presentation of its own is a change to this
requirement, made deliberately, and not a liberty an implementation takes.

A grid cell SHALL render its **control** directly and always on, through the column's `cell` template
rather than through an overlay editor opened on a gesture. No double click, no F2, and no other gesture
is required to make a cell editable; the control in the cell is the editor and the cell accepts typing
with no prior click. Which control the cell shows SHALL be decided from `control` alone, on the entry
the grid holds for that row and that name, and no cell SHALL fall back to a free-text control for a
control that has one:

- `text` SHALL be a single-line text control.
- `textarea` SHALL be a control that accepts a line break.
- `select` SHALL be a control whose choices are the entry's `values`, one standing for nothing chosen,
  and the value the cell currently holds where that is neither of those, and nothing else, so a value
  outside `values` cannot be entered by typing. The retained choice shows a value a CSV import or a
  connector delivered rather than hiding it behind a blank control, and it offers nothing the cell did
  not already hold.
- `checkbox` SHALL be a tick box, over the three states below.
- `integer` and `number` SHALL be a numeric control carrying the entry's `min` and `max` where it
  publishes them, stepping by 1 for `integer` and unconstrained for `number`, and reporting itself
  invalid while it holds a value outside those bounds or one that is not a number.
- `date` SHALL be a date control, and `datetime` a date-and-time control.
- `image` SHALL render no control, on the same terms as `list`: a per-cell file chooser is the wrong
  shape for bulk editing, and free text was never an editor for a data URI, so an operator sets an
  image on the print form. That is not the inertness above: the name is in the row's list, and its
  value is held, validated and submitted like any other. Only the control is withheld.

`slider` SHALL be ignored by a grid. A range presentation cannot carry a readable value in a dense
table cell, so an `integer` or `number` entry takes the plain numeric control whatever its `slider`
flag says. The print form is untouched by this and keeps honoring the flag.

**Keyboard** Tab and Shift+Tab SHALL walk the cells in DOM order and that is the whole keyboard story.
No custom focus code, no Enter-moves-down, no arrow navigation between cells: arrow keys move the caret
inside the field the operator is in. The grid windows its rows, so Tab reaches the end of the rendered
window and stops there rather than scrolling on, and that is accepted. This is a loss against the
previous double-click editor where arrow keys walked cells until a gesture opened an editor; the store's
hotkey dispatch treats a focused INPUT/TEXTAREA as `isInput` and its arrow handlers return early, so
always-on inputs and arrow-key cell navigation cannot both exist. A newline the operator enters into a
`textarea` cell SHALL reach the submitted `data` unaltered; Enter in a `textarea` inserts a newline.

The grid store's installed handlers still call `preventDefault()` for ArrowUp/ArrowDown before returning
when `isInput` is true, and `isInput` excludes `SELECT`, so without suppression textarea vertical
navigation and number stepping would be blocked and select arrows would invoke grid navigation. LabelGrid
SHALL prevent the grid from consuming control keystrokes: each always-on control SHALL stop propagation
of ArrowUp/ArrowDown/ArrowLeft/ArrowRight (and Home/End where relevant) so the document-level hotkey
listener never receives them, and LabelGrid SHALL intercept the store's `hotkey` action and suppress it
when the event target is an INPUT, TEXTAREA, or SELECT inside an always-on cell. Arrow keys then move
the caret/value inside the control rather than the grid focus.

**Clicks and focus inside an always-on control SHALL NOT be stolen by the grid.** The vendor's click
handler exempts only `closest("input")` and otherwise dispatches `focus-cell` (`react-grid/dist/index.es.js:1487`),
which then focuses the cell wrapper (`react-grid/dist/index.es.js:223`); the wrapper also handles
bubbling `focus` and can refocus itself (`react-grid/dist/index.es.js:249`). With installed SVAR 2.7.3 a
click on a focused custom textarea moved `document.activeElement` from `TEXTAREA` to
`DIV[role=gridcell]`. The keyboard suppression above does not address clicks, and `focus-cell` carries
only `row`, `column`, and `eventSource` (vendor dispatch at `:1489`, official API at
`docs.svar.dev/react/grid/api/actions/focus-cell/`), so inspecting an event target is unsupported.
LabelGrid SHALL ensure pointer and focus do not move to the wrapper: each always-on control SHALL stop
propagation of `mousedown`/`click` and of `onFocus` (bubbling focus) so neither the click dispatch nor
the wrapper's focus handler runs for that control. The control then retains focus and remains operable
after initial mount, subsequent clicks, and Tab/Shift+Tab.

**A control SHALL NOT alter a value it cannot display, and SHALL present it with an operable
replacement control.** A cell can hold a value its native control has no form for: a `select` cell
holding a value outside `values`; a `date`/`datetime` cell holding an offset-bearing RFC 3339 instant
(e.g., `2026-09-01T12:00:00Z`) which `input type=date`/`datetime-local` displays as empty; an
`integer`/`number` cell holding a non-numeric string; a `checkbox` cell holding a malformed value; or
any `text`/`textarea` cell whose next paragraph would otherwise apply. All are values a CSV import or
connector can deliver and that grid validation does not refuse. Restoring the general text fallback
dropped from `spec.md:1170`, the cell SHALL render the operable control in its empty/unset visual
state (date/number shows empty, checkbox shows unset, select shows with retained option) **plus** an
adjacent text adornment showing the stored raw value (`title`/`aria-describedby` and visible text), so
what the native control displays (empty) does not misrepresent what will submit. The adornment and the
control coexist in the same always-on cell. Rendering or focusing the cell without an explicit change
SHALL leave the stored value exactly as it was; an explicit change (typing, selecting, checking) SHALL
replace the stored value with the control's new value, clearing the adornment, after which normal
validation applies.

**Appearance** An editable cell SHALL draw a bordered input on the surface colour at rest, not on hover
or focus, so that editable and read-only cells are told apart by looking, without clicking anything. A
read-only cell — a `list` entry, an `image` entry, and every cell while `disabled` (a batch is in
flight) — SHALL render as plain text. Per-row editability continues to come from `cellInput`, not from
the column; a cell for which `cellInput` returns no entry SHALL render as read-only. `registerInlineEditor`
and the column `editor` predicate are removed; the column's `cell` is the only rendering path and
commits travel the existing `update-cell` intercept or `onRowsChange` directly, which is the same data
flow the vendor documents for an external editor.

**Accessibility of the editable grid.** SVAR derives each cell wrapper's `aria-readonly` directly from
the column `editor` property (`react-grid/dist/index.es.js:256`), so removing `editor` would mark every
data cell as read-only in the accessibility tree. `aria-readonly` describes cells where editing is
disabled under the WAI-ARIA grid pattern. LabelGrid SHALL synchronize each data-cell wrapper's
`aria-readonly` with actual per-row editability: `false` for operable controls (has `control` and
`cellInput` returns an entry and not `list`/`image` and not `disabled`), and `true` for missing
entries, `list`, `image`, and `disabled`. The value SHALL reflect transitions into and out of `disabled`
without a remount. The obsolete `editor` mechanism SHALL remain removed.

**An unset `checkbox` or `select` cell SHALL show that nothing is chosen**, distinguishably from
`false` and from the first option, and its control SHALL offer a gesture returning it to that state.
This is the rule the opening block states for an entry with no `default`, reaching the one screen that
could previously not express it. A cell holding the empty string for either control is unset. For a
`select` the returning gesture SHALL be an offered choice standing for nothing chosen. For a
`checkbox` it SHALL be a third activation, cycling unset, checked, unchecked and back to unset,
because a cell cannot carry a second control: Tab is the grid's own commit-and-move, so a separate
clearing control beside the tick box could not be reached by keyboard at all.

An empty cell whose row reports an error against that name SHALL keep its control operable alongside
the marker naming that error, because the marker is what tells an operator which cell blocks the run
and the control is what lets them repair it. The marker SHALL be surfaced as an adjacent adornment
with an accessible name/description (`aria-describedby`/`title` plus visual `⚠ <error>`) rather than
replacing the control; an unset tick box or empty choice in its place would say less, and a marker
alone would leave an initially invalid required cell — and any cell cleared during editing — with no
way to type a value while `Connect.tsx:226`/`Import.tsx:155` keep the run blocked.

A cell whose stored value holds a line break SHALL indicate continuation rather than rendering as one
collapsed line, for **every** control — including an editable `text` control holding an imported
multiline value. A CSV import and a connector both deliver such values into cells the operator never
opens, and `input type=text` strips newlines from its displayed value (verified `first\nsecond` shows as
`firstsecond`). For a `text` cell whose stored value holds a newline, the editable cell SHALL remain an
operable single-line input showing the first line truncated with a `+N` adornment and `title` holding the
full stored value, alongside the input; the stored value SHALL retain the newline until an explicit edit
replaces it, and the row SHALL submit the stored newline value unaltered if the operator leaves the
control without typing. A `textarea` cell, and any read-only cell, SHALL show the same `first line +N`
indication with `title` as before. In all cases the value is distinguishable from the same words written
with a space and the operator can tell the cell holds more than the row shows.

No screen SHALL treat a label as complete, or allow it to be submitted, while the list for that
label's current values has been requested and not yet received. A stale list would otherwise report
one branch's names as satisfied while the render followed another. The only exception is the failure
path below.

The CSV import grid and the connector grid SHALL request lists for their rows in one request, and
SHALL block a run while any row's list is unresolved. Those lists decide validation and submission
alone; the columns come from `inputs.all` under the rule above, so a grid SHALL draw its columns
before any row's list has arrived, and SHALL NOT add or drop one when a list does arrive.

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
A screen SHALL submit exactly the names in its current list, less any name it is deferring; for a
grid that list is the row's own, so a value typed into a column the row does not activate is held and
not sent, exactly as a value entered before a branch was deactivated is.

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

- **WHEN** the grid shows a `subtitle` column, one row selects `orientation: horizontal` and another
  selects `vertical`, for which `subtitle` is not reported
- **THEN** the vertical row's `subtitle` cell is editable, is not validated, and is absent from the
  `data` that row submits

*The heading is historical. `openspec validate --strict` refuses a `MODIFIED` block that drops a
scenario name the current spec carries, so the name outlives the rule it described; the normative
outcome is the editable cell above.*

#### Scenario: An inert cell keeps its value and comes back

- **WHEN** a row holding a `subtitle` value switches to `orientation: vertical` and then back to
  `horizontal`
- **THEN** the value is there throughout, the cell is editable throughout, and the row submits
  `subtitle` again once it is back on `horizontal`

#### Scenario: A gate no row satisfies still shows the fields behind it

- **WHEN** a template declares `orientation` as an `enum` with no `default:`, declares `tags` and
  `location` as `string` parameters, reads `{tags}` and `{location}` only inside a container gated on
  `orientation: horizontal`, and connector rows are added with no value mapped onto `orientation`
- **THEN** the grid shows a column for `orientation`, for `tags` and for `location`, each row's
  `tags` and `location` cells are editable, and no row is refused for either

#### Scenario: Typing the gate's value brings its branch into play

- **WHEN** the operator types `horizontal` into that row's `orientation` cell, having already typed a
  `tags` value while the gate was unset
- **THEN** the row's list reports `tags`, the row is validated against it, and the submitted `data`
  carries both `orientation` and the `tags` value typed before

#### Scenario: The import grid keeps its CSV's header order and appends the rest

- **WHEN** a template declares `params:` as `title`, `subtitle`, `code` in that order and reads all
  three unconditionally, so every list it produces holds all three, and a CSV whose headers read
  `subtitle`, `title` is loaded into the import grid
- **THEN** its columns run `subtitle`, `title`, `code`: the two the file carries in the file's order,
  then the one it does not, appended

#### Scenario: The connector grid orders every column by declaration

- **WHEN** the same template is loaded in the connector grid, whose rows come from a connection rather
  than a file
- **THEN** its columns run `title`, `subtitle`, `code`

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
  `tracking_url`, while the grid shows a column for each, and both columns are editable on both rows

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

- **WHEN** the operator types into a cell whose control is `textarea` and inserts a newline between two
  words
- **THEN** the cell's value carries a newline at that point, and the row submits it unaltered

#### Scenario: Enter commits a grid cell rather than breaking the line

- **WHEN** the operator presses Enter while editing a cell whose control is `textarea`
- **THEN** the cell's value inserts a newline, because the cell's control is always on and Enter is not
  a commit gesture

*This scenario's name is historical. `openspec validate --strict` refuses a `MODIFIED` block that drops a scenario name, so the name outlives the rule it described; the normative outcome is the newline above.*

#### Scenario: Escape abandons a grid cell edit

- **WHEN** the operator types into a cell whose control is `textarea` and presses Escape
- **THEN** the cell keeps the value it held after the typing, because the control is always on and
  Escape is not an abandon gesture; the grid does not act on the key

*This scenario's name is historical. `openspec validate --strict` refuses a `MODIFIED` block that drops a scenario name, so the name outlives the rule it described; the normative outcome is the always-on behaviour above.*

#### Scenario: A cell holding a newline is distinguishable from one holding a space

- **WHEN** one row's `message` holds `line one`, a newline, then `line two`, and another row's holds
  `line one line two`
- **THEN** the two cells render differently, and the first shows that its value continues past what
  the cell displays

#### Scenario: An editable text cell holding a newline shows continuation and preserves the value

- **WHEN** a `text` cell (control `text`) holds `first\nsecond` from a CSV import and is rendered as an
  editable always-on input
- **THEN** the cell shows an operable single-line input displaying `first` with a `+1` adornment and
  `title` holding `first\nsecond`, and the row submits `first\nsecond` unaltered if the operator leaves
  without an explicit edit; an explicit edit replaces the stored value

#### Scenario: A date cell holding an offset-bearing instant shows text plus an operable control

- **WHEN** a `date` cell holds `2026-09-01T12:00:00Z` which `input type=date` cannot represent
- **THEN** the cell shows an operable date control in its empty state plus an adjacent text adornment
  showing `2026-09-01T12:00:00Z` (`title`/`aria-describedby`); focusing or rendering without an explicit
  change leaves the stored value as `2026-09-01T12:00:00Z` and the row submits it, while picking a date
  replaces it

#### Scenario: A numeric and checkbox cell holding an unrepresentable value shows text plus an operable control

- **WHEN** an `integer` cell holds `abc` and a `checkbox` cell holds `maybe`, each unrepresentable by its
  native control
- **THEN** each cell shows its operable control in its empty/unset state plus an adjacent text adornment
  showing the stored raw value, and rendering without an explicit change preserves that raw value for
  submission; an explicit change (typing a number / cycling the checkbox) replaces it

#### Scenario: A select cell offers the values the entry declares and nothing else

- **WHEN** the grid renders a cell whose entry has control `select` with `values` of `small`, `medium`
  and `large` and which holds no value
- **THEN** the cell shows a control offering exactly those three choices plus one standing for nothing
  chosen, at rest and without any click, and no other value can be entered into the cell by typing

#### Scenario: A select cell keeps a value its choices do not carry

- **WHEN** a CSV import puts `enormous` into that cell
- **THEN** the cell shows a control offering `enormous` alongside `small`, `medium`, `large` and the
  choice standing for nothing chosen, and nothing else; the cell still holds `enormous`; and the row
  submits `enormous`, which the render refuses on the same terms it does today

#### Scenario: A checkbox cell is toggled rather than typed

- **WHEN** the grid renders a cell whose entry has control `checkbox` and which holds no value
- **THEN** the cell shows a tick box at rest, and activating it makes the cell hold a true value, the
  row submits it, and the cell draws a ticked box rather than the text `true`

#### Scenario: A checkbox cell can be returned to unset

- **WHEN** the operator activates the tick box of an unset `checkbox` cell three times
- **THEN** the cell reads checked after the first activation and submits a true value, unchecked after
  the second and submits a false value, and unset after the third, whereupon the row submits no key for
  that name

#### Scenario: An unset checkbox does not read as false and an unset select does not read as its first option

- **WHEN** a row carries no value for an entry with control `checkbox` and none for an entry with
  control `select` whose first declared value is `small`, and neither entry is required
- **THEN** the checkbox cell shows a box that is neither checked nor unchecked, the select cell shows
  that nothing is chosen rather than showing `small`, and the row submits no key for either

#### Scenario: An integer cell enforces the bounds its entry publishes

- **WHEN** the entry for `copies` has control `integer` with `min: 1` and `max: 9`, and the operator
  types `20` into its control
- **THEN** the control carries those bounds, reports the value it holds as invalid, and cannot be
  stepped past `9`

#### Scenario: An integer cell steps by one where a number cell does not

- **WHEN** one entry has control `integer` and another has control `number`, and neither publishes
  bounds
- **THEN** the integer control steps by 1, and the number control accepts a fractional value such as
  `2.5`

#### Scenario: A date cell and a datetime cell open the controls their entries name

- **WHEN** the grid renders a cell whose entry has control `date`, and one whose entry has control
  `datetime`
- **THEN** the first shows a date control and the second a date-and-time control at rest, in place of
  any free-text control

#### Scenario: An editor does not destroy a value it cannot display

- **WHEN** a `date` cell holds `2026-09-01T12:00:00Z`, which its control has no form for
- **THEN** the cell shows an operable date control in its empty state plus an adjacent text adornment
  showing `2026-09-01T12:00:00Z`, and rendering/focusing without an explicit change leaves the value as
  `2026-09-01T12:00:00Z`

*This scenario's name is historical. `openspec validate --strict` refuses a `MODIFIED` block that drops a scenario name, so the name outlives the rule it described; the normative outcome is the always-on preservation above.*

#### Scenario: An image cell opens no editor

- **WHEN** the operator focuses a cell whose entry has control `image` and holds a data URI
- **THEN** no control appears, the cell shows that it holds an image rather than the data URI, and the
  row still submits the value the cell holds

*This scenario's name is historical. `openspec validate --strict` refuses a `MODIFIED` block that drops a scenario name, so the name outlives the rule it described; the normative outcome is the always-on read-only cell above.*

#### Scenario: A grid ignores a slider flag the print form honors

- **WHEN** an entry with control `number` carries `slider: true` and publishes `min` and `max`
- **THEN** its grid cell shows the plain numeric control, while the print form still renders a
  range control for the same entry

#### Scenario: An empty invalid cell keeps its marker whatever its control

- **WHEN** a required `checkbox` cell and a required `select` cell are both empty and the row reports
  `required` against each
- **THEN** each cell shows its control (an unset tick box / a select with the `"(none)"` choice) alongside
  the marker naming that error, with the diagnostic surfaced via `aria-describedby`/`title` and visual
  `⚠ <error>`, rather than the marker replacing the control

#### Scenario: An initially invalid required cell can be filled

- **WHEN** a row is rendered whose required `text` cell is empty and reports `required`, showing its
  control alongside `⚠`, and the operator types a value into that control
- **THEN** the row's data updates, the marker clears on the next validation, and the row becomes
  submittable

#### Scenario: Clearing then refilling a required cell restores the row

- **WHEN** a row's required `text` cell holds a value, the operator clears it (leaving the control
  empty with `⚠` alongside), and then types a new value
- **THEN** the first clear makes the row invalid with the marker alongside the still-editable control,
  and the retyped value clears the marker and makes the row submittable again

#### Scenario: Escape abandons an edit in a control that is not a text box

- **WHEN** the operator changes the option in a `select` cell's control
- **THEN** the cell holds the newly chosen value, because the control is always on and Escape is not an
  abandon gesture; the grid does not act on the key

*This scenario's name is historical. `openspec validate --strict` refuses a `MODIFIED` block that drops a scenario name, so the name outlives the rule it described; the normative outcome is the always-on behaviour above.*

#### Scenario: A grid cell is editable without a double click

- **WHEN** the Connect grid and the Import grid render a cell whose entry has control `text`
- **THEN** the cell shows a text control at rest, and typing into it with no prior click or double click
  updates the row's data and is submitted

#### Scenario: Every control type renders its control at rest

- **WHEN** the grid renders cells for entries with controls `text`, `textarea`, `select`, `checkbox`,
  `integer`, `number`, `date` and `datetime`
- **THEN** each cell shows its corresponding control at rest, without any gesture, and the control
  reflects the cell's current value

#### Scenario: A list cell and a disabled cell render no control

- **WHEN** the grid renders a cell whose entry has control `list`, and when the same grid is rendered
  with `disabled` true while a batch is in flight
- **THEN** neither cell shows an editing control; both render as plain text (a `list` cell showing the
  display text or `—`, a disabled cell showing the held value), while an editable cell in the same grid
  with `disabled` false shows its control

#### Scenario: Tab moves focus between editable cells in DOM order

- **WHEN** the grid renders a row with two editable cells and the operator presses Tab while focused in
  the first cell's control
- **THEN** focus moves to the next editable cell's control in DOM order, and Shift+Tab moves it back;
  arrow keys move the caret inside the focused field rather than between cells

#### Scenario: Arrow keys inside always-on controls are not consumed by the grid

- **WHEN** the operator presses ArrowUp/ArrowDown inside a focused `textarea` control, ArrowUp/ArrowDown
  inside a focused `integer`/`number` control, and ArrowUp/ArrowDown inside a focused `select` control
- **THEN** the textarea caret moves vertically, the number control steps its value, and the select control
  navigates its options, without the grid moving focus to another cell and without `preventDefault`
  blocking the native behaviour

#### Scenario: Click inside textarea and select retains control focus

- **WHEN** the operator clicks inside a focused `textarea` control and inside a focused `select` control
  in the grid
- **THEN** `document.activeElement` remains the `TEXTAREA`/`SELECT` (not `DIV[role=gridcell]`), the control
  remains operable, and the wrapper's bubbling focus handler does not refocus the cell

#### Scenario: Initial focus lands on the first editable control

- **WHEN** a grid with no preview column is freshly mounted with one row whose first data cell is
  editable (`text`)
- **THEN** focus is on that `text` control without requiring a click, and subsequent clicks inside
  `textarea`/`select` and Tab/Shift+Tab keep focus on the intended control rather than the wrapper

#### Scenario: The grid cell wrapper exposes correct aria-readonly

- **WHEN** the grid renders an editable `text` cell, a cell for a missing entry, a `list` cell, an `image`
  cell, and when `disabled` becomes true and then false again
- **THEN** the editable cell's wrapper has `aria-readonly="false"` while the missing-entry, `list`,
  `image`, and `disabled` wrappers have `aria-readonly="true"`, and the values update on the
  `disabled` transition without a remount

#### Scenario: Editable cells are visually distinct from read-only cells

- **WHEN** the grid renders an editable `text` cell and a read-only `list` cell, and the same grid while
  `disabled`
- **THEN** the editable cell draws a bordered input on the surface colour at rest, and the read-only
  cells render as plain text, distinguishable without hover or focus

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

- **WHEN** the CSV import grid and the batch grid render a template declaring `tags: { type: list }`,
  the import grid's rows coming from a CSV and the batch grid's from a connection with no column
  mapped onto `tags`
- **THEN** neither draws the editor, the CSV import grid shows no `tags` column, a batch grid cell for
  `tags` is not editable, neither screen holds or submits a value for `tags`, and neither fails

#### Scenario: A mapped list behind an unsatisfied gate stays visible and read-only

- **WHEN** a template declares `tags: { type: list }` and `orientation` as an `enum` with
  `values: [horizontal, vertical]` and no `default:`, reads `{tags:join(', ')}` only inside a container
  gated on `orientation: horizontal`, and a connector row is materialized with the connection's
  multi-valued `tags` column mapped onto `tags` while nothing is mapped onto `orientation`
- **THEN** the batch grid shows a `tags` column, that row's cell reads the elements' display text and
  cannot be edited, and the row keeps the array
- **AND** no error is reported against `tags`, since the row's own list does not report it
- **AND** the row is refused, and the run blocked, for the `orientation` its list does report as
  `required` with no value, which is the gate key reported whether or not its condition holds

#### Scenario: A valid row on the other branch submits without the mapped list

- **WHEN** the operator types `vertical` into that row's `orientation` cell
- **THEN** the row is no longer refused, the `tags` cell still reads the elements' display text and
  still cannot be edited, and the submitted `data` carries `orientation` and no `tags`

#### Scenario: Activating the gate submits the mapped array unchanged

- **WHEN** the operator types `horizontal` into that row's `orientation` cell instead
- **THEN** the row's list reports `tags`, and the submitted `data` carries the connector's elements in
  the connector's order, unchanged and unflattened
