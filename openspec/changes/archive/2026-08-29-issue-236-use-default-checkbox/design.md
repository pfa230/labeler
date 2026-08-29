## Context

See `proposal.md` — Why. Nothing on the server changes: `param-resolution` already resolves an omitted
parameter from its declared `default:`, and the input list already publishes `required: false` for
exactly those parameters.

Three facts from the current baseline shape the approach.

**The form decides nothing about controls.** `FieldForm` renders off the published `InputSpec[]`
(`ui/src/pages/print/FieldForm.tsx:31`) and its incomplete rule is one line, `input.required && blank`
(`:45`). No layout walk, no type dispatch.

**There is one submitted map, and a sixth request that does not use it.**
`submittedData = pruneDataForSubmit(value.data, inputs)` (`ui/src/pages/print/PrintForm.tsx:78`) is
derived once and spread into the preview, both downloads and both prints. The list request is separate
and today sends raw `value.data` (`:46-50`), which is the one place the two can drift.

**Part of deferral exists, unnamed, and narrower than the pruning rule suggests.** Pruning drops an empty
value for six control kinds (`openspec/specs/template-inputs/spec.md:573`), but only an unbounded numeric
control and the two date controls can actually be emptied: a checkbox toggles
(`ui/src/components/ParamInput.tsx:164-188`), a select offers only its options (`:192-223`), and a
bounded numeric entry is a slider (`:98-135`). So the checkbox is the first omission gesture for six of
the nine controls, not three.

## Goals / Non-Goals

**Goals:**
- One explicit, visible state per entry separating "the template decides" from "I decided", reflected in
  every request the form issues.
- Deferred on arrival, so the common case needs no interaction.
- Uniform across all nine controls.
- No server change, no response-shape change, no template-schema change, and no new dependency on an
  open issue.

**Non-Goals:**
- The CSV import grid and the connector grid (#242), which the delta explicitly exempts.
- Anything about `required` as published, or about which defaults are publishable (#262), or about a
  default's authored scalar kind (#270).

## Decisions

**Deferral changes submission, and says nothing about presentation.** This is the decision the design
turns on, and two earlier drafts got it wrong in opposite directions.

The first made a deferred control blank and unseeded. That contradicted three requirements it did not
modify: `param-resolution:159` and its scenario at `:221-224` say a plain published default is seeded
into the print form; `datetime-params:125` says the form seeds a datetime control from the published
default and `:165-168` says a datetime declaring a default is not flagged blank; and
`template-inputs:14-24, :68-78` define `required: false` as "not incomplete without a value", which an
unset-and-blocking control contradicts.

The second kept the control seeded and then *promised* it: that the disabled control shows what will
print and hands the operator the default to edit from. That is the same mistake from the other side.
`template-inputs:73-84` deliberately reserves what a client may do with a published default to #262,
naming `"80mm"` in a `length` and RFC 3339 in a date control as the shapes that break, and the code
agrees: numeric controls hand the raw value to `<input type="number">` or through `Number`
(`ui/src/components/ParamInput.tsx:98-109`, `:138-160`), date controls pass the raw string to
`date`/`datetime-local` (`:226-240`), and the image chooser has no `value` binding at all (`:48-64`).
Promising display would have settled #262 by accident.

So the requirement says only what deferral *submits*, and explicitly disclaims the display question. The
checkbox's label names the entry's published default, as text: text renders `"80mm"`, an RFC 3339
timestamp and an absent data URI alike, needing no control able to hold them. It is careful not to claim
that is what the label will print, because a published default can be one the render rejects and an open
form holds the list it was rendered with. That keeps the change to **one** delta and leaves #262 exactly
as open as it was.

**A deferral set in `FormValue`, keyed by name.** `FormValue.data` is a flat map whose absent key already
means "the operator has typed nothing", so a sentinel inside it would be ambiguous. Keying by name rather
than position is what lets deferral survive a list re-request that changes which entries are active.

**Re-checking discards whatever was entered.** The alternative, remembering it, makes the checkbox a
toggle with hidden state, so an operator who re-checks to start over gets their typo back. What the
control returns to is the seeding rule's business, not this requirement's.

**The `image` control needs an explicit reset.** Its file chooser is an uncontrolled
`<input type="file">` (`ui/src/components/ParamInput.tsx:48-63`), so clearing `FormValue.data[name]`
leaves the browser still displaying a chosen filename. Re-checking must clear the input's own value, or
the form would show a file that is not being sent. This is the one place the "discard" rule needs code
rather than state.

**The list request carries what submission carries.** `PrintForm` sends raw `value.data` to
`useLabelInputs` (`:46-50`) while output requests use the pruned map (`:78`). Deferral makes that drift
visible: a deferred name left in the list request would report the branch its value selects while the
render follows the branch its absence selects. Both must read one derived map. The delta says so, and
deliberately claims nothing stronger: list resolution is lenient and render resolution is strict
(`openspec/specs/template-inputs/spec.md:176-201`), so a published default the render rejects still
diverges between them exactly as any omitted name does today.

**Deferral follows the entry across branch changes, and nothing survives a template change.** An entry
can enter the list when a branch activates and leave when it deactivates, and the requirement already
retains values across that, so deferral is retained the same way. A template change is different in kind:
`PrintForm` initialises its state once (`ui/src/pages/print/PrintForm.tsx:31-36`) and `Print.tsx` does not
key it by template id (`:20-26`), so today a name both templates declare keeps the previous template's
value. Resetting only the deferral bit would leave template A's value displayed in a disabled control
while template B's render resolved B's default, which is exactly the divergence this change exists to
remove. Both the values and the deferral set are therefore reinitialised from the new `inputs.default`,
and the requirement says so rather than leaving it to "with the rest of the form's state".

**ADR-0090, "A declared default is deferred, not copied."** Records that the print form represents a
declared default by omitting the name while still showing it, and why deferred is the arrival state. It
supersedes nothing: ADR-0088 established that a default must be declared, and this decides how a screen
offers one. Adds its row to `docs/adr/README.md`. `0090` is free: `main` at `5bcfb05` ends at
`0089-wrapping-and-the-overflow-policy.md`, and no `009x` ADR exists on any branch or in any of the
seven live worktrees. The previous draft claimed `0089` and was wrong on both halves, so this number was
checked against `git log --all --diff-filter=A -- 'docs/adr/009*.md'` and against every worktree's
`docs/adr/`, not against one stale checkout.

## Risks / Trade-offs

**A disabled control reads as broken to some operators.** → It sits under a checked `Use default`
checkbox naming the published default, so the disabled state is explained rather than bare, and what the
control itself shows is unchanged from today.

**The checkbox label may name a default the control below it cannot show, so the two can disagree
visually.** → That disagreement already exists today, silently, and is #262. Naming the default in text
is what makes it visible at all; the requirement is careful to promise the label, not the control.

**The list request and the submitted map can drift again.** → They are derived from one map; a test
asserts the list request omits a deferred name, which is the case that would otherwise fail silently by
reporting the wrong branch.

**Two idioms coexist until #242: a checkbox on the print form, a seeded copy in every grid row.** →
Stated in the delta rather than left implicit.

**A checkbox per defaulted entry is visual weight.** → It appears only for entries publishing a default,
and the contract is stated as a checkbox with a `Use default` label rather than as a layout, so the
presentation can compress without a spec change.

## Migration Plan

None. No stored state, no API surface, no template files. Deferral is computed from the published list.
Rollback is reverting the UI change: submitting every seeded default is what ships today, and the service
accepts both shapes.
