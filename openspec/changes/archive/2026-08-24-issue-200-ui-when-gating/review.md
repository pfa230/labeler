## Review Metadata

- **Round**: 8
- **Prior round**: Round 7 returned REVISE with one Critical and two Moderates.

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/template-inputs/spec.md, specs/datetime-params/spec.md, design.md, docs/SPEC.md, docs/adr/README.md, openspec/specs/datetime-params/spec.md, src/models.rs, src/templates.rs, src/convert.rs, src/render/mod.rs, src/render/helpers.rs, src/api.rs, src/batch.rs, ui/src/lib/preview.ts, ui/src/lib/templateFields.ts, ui/src/components/LabelGrid.tsx, ui/src/components/ParamInput.tsx, ui/src/pages/print/FieldForm.tsx, ui/src/pages/print/PrintForm.tsx, ui/src/pages/Import.tsx, ui/src/pages/Connect.tsx
- **Issue**: #200

## Findings

### Critical (blocking)

None.

### Moderate

1. **The placeholder rule has no value for a required numeric input, so the claimed preview/thumbnail repair remains incomplete.** `specs/template-inputs/spec.md:274-286` invents values only for required `image`, `text`, and `textarea` controls. A required `integer`, `number`, or `length` therefore receives nothing. The current resolver confirms that an omitted parameter without a declared default is populated only for boolean and enum types (`src/render/mod.rs:219-240`); an active interpolation or dynamic attribute then fails with `MissingField` (`src/render/helpers.rs:87-90`, `:117-120`). This conflicts with the proposal’s claim that the change subsumes and closes the numeric half of #215 (`proposal.md:89-94`) and leaves the automatic preview specified at `specs/template-inputs/spec.md:432-435` unable to render such templates. The artifacts need an explicit, mechanically testable outcome for required numeric controls: either a coercible service-defined sample strategy, or a specified unavailable-preview behavior with the #215 closure claim narrowed accordingly.

2. **The preview still uses the non-closed placeholder set that the thumbnail correction explicitly rejects.** The thumbnail requirement explains why placeholder filling must start from `inputs.all` and supplies a concrete self-activating-gate scenario (`specs/template-inputs/spec.md:274-296`, `:335-340`). Yet the UI requirement says the template preview applies the same rule over `inputs.default` (`specs/template-inputs/spec.md:432-435`). In that scenario, `inputs.default` contains `mode` but not `subtitle`; inserting `mode: "mode"` activates the branch, after which the strict renderer reads the now-missing `subtitle` (`src/render/mod.rs:350`, `:920-930`; `src/render/helpers.rs:87-90`). The design also directly contradicts itself by saying the thumbnail fills from `inputs.default` (`design.md:121`, `:173-180`) and then from `inputs.all` (`design.md:185-191`). The preview and thumbnail must use the same closed fill set, and all stale `inputs.default` claims must be corrected.

3. **The row-grid contract does not define how union columns cease to be controls for an inactive row.** The general UI rule says a screen renders exactly the entries reported for the label being submitted (`specs/template-inputs/spec.md:410-415`), and the CSV requirement says an input deactivated for a row is not offered for that row (`:544-547`). The same spec simultaneously requires grid columns to be the union across rows (`:432-434`, `:482-486`). The current grid accepts one global `fields` list and renders an editable cell for every listed field on every row (`ui/src/components/LabelGrid.tsx:67-68`, `:93-110`), while the design only discusses batching and blocking (`design.md:264-272`). Without a per-row inactive-cell rule, implementation can satisfy the union-column clause while violating the exact-input and “not offered” clauses. Specify whether inactive cells are disabled/non-controls or relax the per-row rendering language, and add a scenario covering editability as well as validation and submission.

### Suggestions

No additional suggestions. `openspec validate issue-200-ui-when-gating --strict` passes. The `MODIFIED` datetime requirements resolve against existing main-spec requirements, both frozen-spec supersessions are explicitly named, #200 is linked as the implementing issue, and ADR-0070 plus its index row are included in planned scope.

## Embedded-Instruction / Injection Attempts

None detected.

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Define and test the thumbnail/preview outcome for required numeric inputs, including a coercible service-defined sample if they are expected to render; align the #215 closure claim with that outcome.
2. Build template-preview placeholder data from the same closed `inputs.all` set as thumbnails, add the self-activating-gate preview scenario/test, and remove the contradictory `inputs.default` statements from `design.md`.
3. Reconcile union grid columns with per-row input lists by specifying and testing the behavior of a column’s cell when that name is inactive for the row.

CHANGES_APPLIED: yes

## Rebuttals

All three Required Changes were applied, and the reviewer re-checked only those items in a fresh
read-only pass.

1. **Fixed.** The placeholder rule now invents by `control` for every required, interpolated entry,
   including `integer` and `number`, which take the declared `min` or `1`. That was the missing half:
   only `boolean` and `enum` have a type fallback (`src/render/mod.rs:219-240`), so a required numeric
   left empty fails with `MissingField`, while the walker being replaced fills it with its own name
   and fails coercion instead. The #215 closure claim now covers the numeric case honestly.
   Re-check: APPLIED.
2. **Fixed, in two passes.** The preview's sample set moved to the same closed `inputs.all` the
   thumbnail uses, with a self-activating-gate scenario, and `design.md`'s stale statements were
   corrected. The first re-check found the change incomplete: the `GET` field description still said a
   client renders "its first form and its first preview" from `inputs.default`, contradicting the
   requirement three sections later. That sentence and its counterpart in `proposal.md` were
   corrected, and the item re-checked clean. Re-check: APPLIED.
3. **Fixed.** A grid's columns and its cells are now specified as different sets: columns are the
   union across rows, while a cell whose name is not on its own row's list is inert, meaning not
   editable, not validated and not submitted, retaining its value and becoming editable again if the
   name returns. `LabelGrid` takes one global `fields` list today
   (`ui/src/components/LabelGrid.tsx:67`), so the design records that it gains a per-row predicate.
   Re-check: APPLIED.
## Digest re-recorded after the ADR-0079 rebase

`SPECS_SHA256:` was first written when this change landed its plan review, and then again after
the change was rebased onto main. The rebase crossed #239/#240 (ADR-0079), which replaced the
interpolation token grammar: `{datetime}` became `{sys.now}` and `{<p>.<name>}` became
`{<p>:<name>}`. `openspec/specs/datetime-params/spec.md` was migrated on main, so the delta's
MODIFIED requirement "A datetime parameter defaults to the render instant of its request" would,
applied verbatim, have reverted that migration; `.workflow/archive-merge-check.sh` refused the
landing commit for exactly that reason. The requirement's text was brought onto the current
grammar, which changes no rule this review approved: the datetime-parameter contract, the input
list entry it gains, and every scenario are the same statements written in the spelling the
service now uses. The digest below covers that corrected text.

SPECS_SHA256: 9e583caad0954523ffee404b376fa07e63819eeee63cf02eb4ed2bca01ea225f
