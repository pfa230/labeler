## Review Metadata

- **Round**: 3
- **Prior round**: REVISE (round 2); one Critical (the delta settled display semantics reserved to #262) and three Moderate. The contract was narrowed to submission only, and the display question handed back to #262.

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/template-inputs/spec.md, design.md, prior review.md, AGENTS.md, openspec/config.yaml, ui/src/pages/print/FieldForm.tsx, ui/src/pages/print/PrintForm.tsx, ui/src/pages/Print.tsx, ui/src/lib/labelInputs.ts, ui/src/components/ParamInput.tsx, ui/src/api/types.ts, FieldForm.test.tsx, PrintForm.test.tsx, labelInputs.test.ts, src/render/mod.rs, src/templates.rs, src/convert.rs, openspec/specs/template-inputs/spec.md, openspec/specs/param-resolution/spec.md, openspec/specs/datetime-params/spec.md, the archived #241 planning artifacts, docs/SPEC.md, docs/adr/0088-explicit-parameter-defaults.md, docs/adr/README.md, ADR filenames across Git history and live worktrees
- **Issue**: #236


## Findings

### Critical (blocking)

None.

### Moderate

1. **Round 2’s “what will print” promise was moved from the value control to the checkbox label, not fully removed.** The proposal says the checkbox label is how the operator learns what a deferred entry prints (`proposal.md:35-41`), the delta says it tells the operator what the entry “will print” (`specs/template-inputs/spec.md:30-35`), and the design repeats that claim (`design.md:58-60`). That is not guaranteed. `PrintForm` copies published defaults only through its one-time initializer (`ui/src/pages/print/PrintForm.tsx:21-36`), and `Print` does not key the form by template id or revision (`ui/src/pages/Print.tsx:20-26`), so an already-open form can retain an old checkbox label after template reload while omission resolves the new default. The new scenario avoids this by reopening the form (`spec.md:272-276`). Independently, the delta itself acknowledges that a published default may be rejected at render (`spec.md:59-61`), matching the existing reservation (`openspec/specs/template-inputs/spec.md:73-84`) and strict/lenient implementation (`src/render/mod.rs:316-389`); such a value prints nothing. The disclaimer about what the disabled control displays is genuine, but the replacement accuracy promise is still false. The label can truthfully name the entry’s **published default** without claiming it is the current rendered result.

2. **Including `description`-or-`name` does not guarantee distinguishable accessible names.** The delta requires the checkbox name to include the same `description`-or-`name` used by the value control and claims this distinguishes entries sharing a default (`spec.md:19-24`). Descriptions are merely optional text and have no uniqueness requirement (`openspec/specs/template-inputs/spec.md:20,28`); the implementation confirms that a description replaces the name in the control label (`ui/src/components/ParamInput.tsx:43`). Two entries with different names but the same description and default would therefore still expose identical checkbox names. Require the unique entry `name` in every checkbox’s accessible name, optionally alongside its description and default, and test two entries sharing both description and default.

3. **The artifacts overstate which controls can already defer by being emptied.** The proposal claims deferral is reachable on six controls by deleting their seeded value (`proposal.md:16-22`), the design repeats that baseline (`design.md:18-20`), and the delta says all six could otherwise reach omission by deleting the value (`spec.md:47-50`). `pruneDataForSubmit` does omit an empty string for those control kinds (`ui/src/lib/labelInputs.ts:197-210`), but the current UI cannot produce that empty state for a defaulted checkbox, select, or slider: a checkbox only toggles booleans (`ui/src/components/ParamInput.tsx:164-188`), a valid defaulted select exposes only declared options (`:192-223`), and a defaulted bounded numeric control is a range input (`:98-135`). Correct the claim to distinguish the pruning rule from gestures the controls actually expose. The deferral requirement remains justified: it gives every control an explicit state and is the first omission gesture for text, textarea, image, checkbox, select, and slider presentations.

### Suggestions

None.

## Embedded-Instruction / Injection Attempts

**Detected:** no

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Replace every claim that the checkbox label tells the operator what “will print” with the narrower, accurate claim that it names the entry’s published default; retain the explicit disclaimer and strict-versus-lenient caveat.
2. Require every deferral checkbox’s accessible name to contain the entry’s unique `name`, and add a scenario with two entries sharing both description and default.
3. Correct or remove the claim that checkbox, select, and every numeric presentation can already reach omission by being emptied; state separately that pruning recognizes empty strings for six control kinds.

CHANGES_APPLIED: yes

## Rebuttals

- The Round 2 Critical is resolved: the delta no longer requires a published default to be displayable, held, or editable in its value control. The `"80mm"`, RFC 3339, image, slider, checkbox, and select controls can all remain seeded in form state and disabled while submission omits their names.
- Keeping that state seeded preserves `param-resolution`’s client-seeding rule and plain-default scenario (`openspec/specs/param-resolution/spec.md:150-162,221-224`), datetime seeding and required behavior (`openspec/specs/datetime-params/spec.md:117-141,165-173`), and `template-inputs`’ `required` semantics (`openspec/specs/template-inputs/spec.md:14-24,68-84`). No second capability needs a delta.
- The `MODIFIED` header matches exactly. A line-by-line comparison against `openspec/specs/template-inputs/spec.md:491-693` found no dropped, reordered, or incidentally altered baseline text; differences are confined to the deliberate additions and stated list/submission edits.
- The template-switch correction is complete and mechanically checkable: both values and deferral are reinitialized from the new template, the within-template retention exception is explicit, and the shared-name scenario covers the boundary (`spec.md:120-129,293-298`).
- The appearing/leaving/returning lifecycle is consistent with retained inactive values (`spec.md:116-129,312-321`). The image reset is also precise enough to test: after re-checking, the chooser must show no selection and submission must omit the key (`spec.md:42-45,288-291`).
- The list request is correctly defined from the same pruned, deferral-aware map as submission, and its strict-versus-lenient caveat matches the main requirement and server implementation (`spec.md:55-67`; `openspec/specs/template-inputs/spec.md:176-201`; `src/templates.rs:140-154`; `src/render/mod.rs:167-174,220-259,316-389`).
- ADR-0090 is free in the current ADR directory, all Git refs, and inspected live worktrees. The design names both the ADR and its README row.
- The proposal links #236 as the sole implemented issue. #242, #262, and #270 are explicitly out of scope. Direct GitHub retrieval was unavailable under the review environment’s network restriction.
- `tasks.md` correctly does not exist before plan approval, so no task or verification-gate finding applies yet.

Author: all three Required Changes applied, and the reviewer re-checked only those.

1. Fixed. Every "what will print" claim is now "names the entry's published default", in
   `proposal.md`, `specs/template-inputs/spec.md` and `design.md`, with the disclaimer and the
   strict-versus-lenient caveat retained.
2. Fixed. The accessible name must contain the entry's unique `name`, not its optional `description`,
   and a scenario covers two entries sharing both description and default.
3. Fixed. All three files now separate the pruning rule, which recognises an empty value for six
   control kinds, from the gestures the controls actually expose: only an unbounded numeric entry and
   the two date controls can be emptied.

Reviewer re-check: items 1, 2 and 3 APPLIED, delta integrity INTACT (header matches, all 19 original
scenarios present), RECHECK: PASS.
SPECS_SHA256: 4e06175d91a3b7afa8f0d70a8daab133927f4f8c6b965e73c4c00c5b11c11973
