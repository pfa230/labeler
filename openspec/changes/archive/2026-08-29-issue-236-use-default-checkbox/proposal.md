## Why

Implements [#236](https://github.com/pfa230/labeler/issues/236).

Since `param-resolution` landed, an omitted parameter resolves from the template's declared `default:`
and from nothing else, and the input list marks exactly those parameters `required: false`. The print
form seeds every control from the entry's published `default` (`ui/src/pages/print/PrintForm.tsx:21-25`,
required by `openspec/specs/template-inputs/spec.md:494`) and then submits what it seeded, because
`pruneDataForSubmit` (`ui/src/lib/labelInputs.ts:197`) drops only names absent from the active list and
blank values on non-text controls, never a value equal to its default.

So the template's default is copied into every request. Nothing tells the operator a value came from the
template rather than from a person, and editing a template's `default:` does not reach a form already
open on it.

**Part of the mechanism exists, and less of it than it looks.** The pruning rule drops an empty value
for six control kinds (`openspec/specs/template-inputs/spec.md:573`), and `datetime-params` states the
consequence outright: "Clearing the control SHALL submit an omission... the declared default, or
`422 MissingField` when there is none" (`openspec/specs/datetime-params/spec.md:134`).

But a screen can only send an empty value for a control an operator can empty, and most cannot be. A
`checkbox` toggles between two booleans (`ui/src/components/ParamInput.tsx:164-188`), a `select` offers
only its declared options (`:192-223`), and a bounded numeric entry is a slider that always sits
somewhere (`:98-135`). Emptying therefore reaches omission on an unbounded numeric entry and on a `date`
or `datetime` control, and nowhere else. For every other control the checkbox is the first gesture that
can express it at all, and even where emptying works, nothing says that is what it means.

## What Changes

- An input entry that publishes a `default` renders a **`Use default` checkbox** above its control in
  the print form, checked whenever the entry appears, naming the published default **as text** in its
  label.
- **Deferral changes what is submitted, and nothing else.** While deferred, the control is disabled and
  the entry's name is omitted from the submitted `data`, so the service resolves it. Editing the
  template's default then changes what prints, with no edit to the form.
- The change makes **no claim about displaying a published default in its control**. What a disabled
  control shows is whatever today's seeding rule already puts there, and whether a published default can
  be shown, held or edited in its control stays reserved to
  [#262](https://github.com/pfa230/labeler/issues/262), which `template-inputs:73-84` already names,
  citing `"80mm"` in a `length` and RFC 3339 in a date control. The checkbox label names the entry's
  **published default**, as text, which is why it needs no control able to hold it: an `image` entry's
  file chooser could never display one. It does not claim to name what the label will print, since a
  published default may be one the render rejects.
- Because deferral never empties a control it cannot make an entry incomplete, so the published meaning
  of `required` is untouched.
- Clearing the checkbox enables the control, leaving whatever the seeding rule gave it. Re-checking
  restores deferral and discards anything entered meanwhile; for the `image` control, whose file chooser
  the browser owns, it also clears the chooser's selection.
- The checkbox's accessible name contains the entry's unique `name`, so two entries stay distinguishable
  even when they share a `description` and a default, and it does not share a label element with the
  value control.
- Deferral follows the entry, not the position: an entry appearing later because a branch activated
  arrives deferred, and an entry leaving the list keeps its state for its return. Selecting a different
  template reinitialises both the values and the deferral state from the new template's
  `inputs.default`, so a name the two templates share carries nothing across.
- The print form only. The CSV import grid and the connector grid keep seeding and submitting;
  [#242](https://github.com/pfa230/labeler/issues/242) tracks the affordance for them.

No server change, no API change, no response-shape change, no template-schema change.

## Capabilities

### New Capabilities

None.

### Modified Capabilities
- `template-inputs`: the requirement "A screen renders the reported inputs and decides nothing else"
  (`openspec/specs/template-inputs/spec.md:491`) says a screen submits exactly the names in its current
  list (`:565`). A deferred name is omitted from that map, so that sentence changes, the deferral
  control is added, and the print form's list re-request is stated to carry the same map submission
  would.

Nothing else is modified, and that is a consequence of keeping the control seeded.
`param-resolution`'s seeding rule and its scenario "A plain default is seeded" (`:159`, `:221-224`),
`datetime-params`' seeding and clearing rules (`:125`, `:134`, `:165-168`), and `template-inputs`' own
definition of `required` and of an incomplete label (`:14-24`, `:68-78`) all stay true as written,
because deferral neither empties a control nor leaves an entry without a value.

## Impact

- `ui/src/pages/print/PrintForm.tsx` — the deferral set joins `FormValue`; `submittedData` omits
  deferred names. All five request bodies already read one derived `submittedData` (`:78`), so they
  follow with no per-site change. The list request at `:46-50` currently sends raw `value.data` and must
  send the same map submission would.
- `ui/src/pages/print/FieldForm.tsx` — renders the checkbox and owns its label associations.
- `ui/src/components/ParamInput.tsx` — already accepts `disabled`; unchanged otherwise, because this
  change does not touch seeding. The `image` branch needs a reset path for the uncontrolled file input
  (`:48-63`), which is the one place the discard rule needs code rather than state.
- `ui/src/pages/Print.tsx` — `PrintForm` is not keyed by template id (`:20-26`) and initialises its
  state once (`PrintForm.tsx:31-36`), so reinitialising on a template change is real work.
- `ui/src/lib/labelInputs.ts` — the list re-request keyed on form values must account for deferral.
- Tests in `FieldForm.test.tsx`, `PrintForm.test.tsx`, `labelInputs.test.ts`.
- `docs/adr/0090-*.md` and its row in `docs/adr/README.md`.
