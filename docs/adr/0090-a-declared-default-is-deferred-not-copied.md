# 90. A declared default is deferred, not copied

Date: 2026-08-29

## Status

Accepted. Issue [#236](https://github.com/pfa230/labeler/issues/236). Supersedes nothing:
[ADR-0088](0088-explicit-parameter-defaults.md) established that a default must be declared and
published, and this decides how a screen offers one.

## Context

The print form seeded every entry publishing a `default` into its own `data` map and submitted that
copy with the label. The copy was taken when the form rendered, so a template whose default was
edited and reloaded still printed the value the open form was holding, and the operator had no way
to say "the template decides this one" for a control that cannot be emptied: a `checkbox`
presentation toggles between two booleans, a `select` offers only its declared options, and a
bounded numeric entry is a slider that always sits somewhere.

`param-resolution` already resolves an omitted parameter from its declared `default:`, so the
correct request was to omit the name. Nothing on the server needed to change; what was missing was
a gesture that reached omission.

The form also had one map too many. Output requests carried the pruned map, while the input-list
request carried the raw values, so the list could report the branch a value selects while the render
followed the branch its absence selects.

## Decision

1. **The affordance is a checkbox, and deferral is the arrival state.** Every entry publishing a
   `default` renders a `Use default` checkbox naming that published default as text, checked
   whenever the entry first appears. The common case therefore needs no interaction, and the
   uncommon one is one click.

2. **Deferral changes what is submitted, and makes no claim about presentation.** A deferred entry's
   value control is disabled and its name is absent from the submitted `data`. What the disabled
   control *displays* is whatever the seeding rule already put there. A published default a control
   cannot hold, `"80mm"` in a numeric control or a data URI in a file chooser, is named in the
   checkbox's label as text, which every default renders as; whether a control can show one is
   [#262](https://github.com/pfa230/labeler/issues/262) and stays open.

3. **The accessible name carries the entry's `name`.** It is unique within a list, so two entries
   sharing a `description` and a default stay distinguishable, and the checkbox never shares a label
   element with the value control.

4. **Re-checking discards.** Clearing the checkbox enables the control and leaves the seeded value
   in place, to be submitted like any other. Re-checking restores deferral and returns the control
   to the seeding rule's value, including clearing the file chooser an `image` entry renders, whose
   selection is the browser's own state and would otherwise outlast the value it stood for.

5. **The list request carries what submission carries.** Both are one derived map: the values held,
   pruned by the same rules, less any name being deferred. A deferred name reaches the service as an
   omission there exactly as it will at render time, so the branch the list reports is the branch
   the render takes.

6. **Deferral follows the entry across branches, and nothing survives a template change.** An entry
   a later list brings in arrives deferred; one that leaves keeps its state and is restored if it
   returns, on the same terms its value is. Selecting a different template reinitialises both values
   and deferral, so a name two templates share carries nothing across.

7. **The grids keep copying, for now.** The CSV import grid and the connector grid still seed and
   submit each cell's default. Two idioms coexist until
   [#242](https://github.com/pfa230/labeler/issues/242) settles the affordance for a grid.

## Consequences

- Editing a template's default changes what an already-open form prints, because the form sends no
  value for it.
- Six of the nine controls gain their first way to say "omit this", where previously only an
  unbounded numeric entry and the two date controls could be emptied.
- The disabled control and the checkbox's label can disagree visually, for a default no control can
  hold. That disagreement existed silently before; naming the default in text is what makes it
  visible, and #262 is where it is settled.
- No server change, no response-shape change, no template-schema change. Rollback is reverting the
  UI: the service accepts both shapes.
