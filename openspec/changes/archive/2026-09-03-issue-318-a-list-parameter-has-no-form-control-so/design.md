## Context

See proposal.md for motivation. What shapes the approach is where the pieces already sit.

`ParamInput` (`ui/src/components/ParamInput.tsx`) has exactly one consumer, `FieldForm`
(`ui/src/pages/print/FieldForm.tsx`), which the print form renders. The three screens the issue keeps
out of scope reach their cells through `DataEditCell` in `LabelGrid`, never through `ParamInput`, so an
editor added to `ParamInput` cannot leak into them. They also submit through `POST /api/batch`
(`Import.tsx:290`, `Connect.tsx:265`), which is why a missing list value surfaces there as
`422 BatchInvalid` and not as the top-level `MissingField` the print form would get.

`pruneDataForSubmit` (`ui/src/lib/labelInputs.ts:240-262`) is shared by the print form, `Import.tsx` and
`Connect.tsx`. It iterates the screen's `data` map, not the reported inputs, so a name the map does not
hold is never submitted. For `control === "list"` it already passes an array through and drops anything
else. That is why an untouched list entry vanishes today: the print form holds nothing for it. It is
also why the empty-list rule can be scoped to one screen without a flag — the grids hold no array, so
nothing changes for them.

`FormValue.data` and `FormValue.deferred` are the print form's own state. `initialFieldState` and
`withArrivals` (`PrintForm.tsx:24-52`) are local to that file and are the only two places an entry
enters those maps.

## Goals / Non-Goals

**Goals:**

- One editor in `ParamInput`, reaching the print form and nothing else.
- An untouched `list` entry submits `[]` through the existing pruning path, with no new argument to a
  function three screens share.
- The completeness exemption at `PrintForm.tsx:115` is deleted, not replaced by a narrower one.

**Non-Goals:**

- Any server change. Nothing under `src/` is touched.
- Drag-and-drop reordering (#347), a grid or CSV or connector spelling for a list (#271, #320, #348).
- A per-element type or value set. `list-params` settled that elements are strings.
- Changing `pruneDataForSubmit`'s contract, which would change what the two grids submit.
- The `Use default` checkbox's rendering of a list default, and the stale `datetime-params` table cell.
  Both are real and both are cut; proposal.md records what each would say.

## Decisions

### The empty list is seeded into the print form's state, not synthesized at submission

A `list` entry that publishes no `default` enters `FormValue.data` as `[]` in `initialFieldState` and
in `withArrivals`, alongside the defaulted entries those two already seed. Everything downstream then
needs no knowledge of lists: `pruneDataForSubmit` passes the array through unchanged, the completeness
check sees a value, and the retention rule that keeps a value across a branch switch keeps this one.

Both functions are private to `PrintForm.tsx`, which is what confines the rule to the one screen the
spec scopes it to. No other screen gains a value for a `list` entry, and none is asked to.

Alternatives:

- **Emit `[]` from `pruneDataForSubmit` for every reported `list` input.** Rejected: the function is
  shared, so `Import.tsx` and `Connect.tsx` rows would start submitting `tags: []` for a column those
  screens deliberately exclude. The issue requires their behaviour to be unchanged and their tests
  unedited.
- **Give `pruneDataForSubmit` a fourth argument the print form passes.** Rejected: a flag is a second
  code path through a function three screens share, and it would encode as a caller option what is
  really a property of the screen that holds the value.
- **Have the editor call `onChange([])` on mount.** Rejected: a render-time side effect, and it fires
  again every time the entry returns after a branch switch.

The seeding must survive `withArrivals`'s return guard, which is `deferred === value.deferred ? value :
...`. A `list` entry with no default changes `data` and not `deferred`, so that guard would discard it;
the guard becomes a check on both maps. This is the one non-obvious edit in the change.

No extra list request results. The first paint seeds from `detail.inputs.default` before
`useLabelInputs` reads `value.data`, so the very first request already carries `tags: []`; later
arrivals seed at the same moment the form re-requests anyway. A `list` cannot appear in a `when:`
(`conditional-visibility`), so carrying `tags: []` can never change which entries come back.

### An inert move control keeps its place in the focus order

The two rules the issue asks for pull against each other. "Disabled at the ends" and "focus follows the
moved element" cannot both hold if the end controls are natively `disabled`, because a disabled button
cannot take focus: an element moved into the first row would leave the operator's focus on the document.

The move controls are therefore never natively disabled while the entry is editable. At the ends they
carry `aria-disabled="true"`, do nothing when activated, and stay in the focus order. That is the
standard spelling for a control that must stay reachable, it satisfies the issue's criterion that the
control be **inert**, and it needs no second mechanism for the boundary case.

Alternatives:

- **Send focus to a different control at the boundary**, such as the row's move-later control.
  Rejected: it puts the operator somewhere they did not ask to be, and it is a special case that exists
  only at the ends.
- **Drop the focus rule.** Rejected: React keys the buttons by position, so without it the operator's
  second activation moves a *different* element. That is the difference between reachable by keyboard
  and operable by keyboard, which is an acceptance criterion of the issue.

Deferral is the one place the controls are natively disabled, the move controls included. Nothing in a
deferred editor is being operated, so nothing in it needs to hold focus, and the spec says so where it
states both rules.

### The editor holds no state of its own

The rows are derived from the `string[]` prop on every render, and every gesture calls `onChange` with
a new array. An editor holding its own draft array would have two sources of truth for one value, and
the retention and deferral rules already move that value out from under the component: re-checking the
deferral checkbox replaces it, and switching templates reinitialises it.

The consequence is that an empty text row is a real element, `""`, and not a UI-only placeholder. That
is also what the spec requires: a screen normalizes nothing, and `[""]` is a value the service accepts.

The one piece of state the component does keep is the ref map used to place focus after a move or a
removal, set in a layout effect after the state change. Appending needs no such handling: the appending
control stays where it is.

### `required` still means what it means, and a list entry can never be incomplete on the print form

`[]` satisfies a required list, so the completeness check never blocks on one. That is not a special
case bolted onto the check: it falls out of the entry holding a value, which is why the exemption at
`PrintForm.tsx:115` is deleted rather than rewritten. The rule the check applies is unchanged, and
`FieldForm`'s `invalid` computation needs no list branch for the same reason, which is why that file is
not edited at all.

The delta narrows one existing scenario, "A broken default is shown as a diagnostic, not a value", to
an entry with control `text`. It was written generically and its "flagged incomplete until the operator
supplies a value" is not true of a list entry, which always holds one. No behaviour changes: no screen
draws a list control today, so nothing was ever judged by it.

### `ParamInput`'s `ParamSpec` spelling gets the editor too

Every other branch in `ParamInput` accepts both spellings, `control === "x"` and the declared
`paramSpec.type`. The list branch already does (`control === "list" || paramSpec.type === "list"`) and
keeps doing so. No caller passes a `ParamSpec` today, but a branch that answered differently by
spelling is the drift the rest of the component avoids.

## Risks / Trade-offs

- **The `withArrivals` return guard is easy to miss** → seeding would be silently dropped for exactly
  the case the issue is about, an undefaulted list, and the form would look right until submission. A
  test that submits without touching the editor catches it, and one is required by the specs.
- **`aria-disabled` is a weaker signal than `disabled`** → a control that looks inactive but is
  clickable is a real trap if the handler is not actually short-circuited. The handler must return
  early at the boundary, and a scenario asserts that activating an end control moves nothing.
- **Focus management is the part jsdom tests weakly** → assertions go through `document.activeElement`
  after a keyboard activation, which jsdom does model; nothing here depends on layout or on pointer
  behaviour, which it does not.
- **The editor's accessible names carry element positions, which change under the operator** → a name
  read by assistive technology just before a move names the position, not the element. The alternative,
  naming the element's own text, is worse: an empty element would have no name at all. The spec fixes
  the position spelling so the choice is visible rather than incidental.
- **`valid` gates `useLivePreview`, so the preview's behaviour moves with the exemption.** It does not
  start or stop firing: the exemption already made a list entry satisfied, so the preview fired and the
  render answered `422 MissingField`. What changes is that the request now carries `tags: []` and
  succeeds. That is the intended direction, and no existing test asserts the failing shape.
