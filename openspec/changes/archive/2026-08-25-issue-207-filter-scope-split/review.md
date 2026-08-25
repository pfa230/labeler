## Review Metadata

- **Round**: 1
- **Prior round**: none

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/, design.md, current connector-browser spec, AGENTS.md/CLAUDE.md, openspec/config.yaml, ConnectorBrowser.tsx, connectorFilter.ts, connector-browser tests, homebox.rs, docs/adr/README.md
- **Issue**: #207

## Findings

### Critical (blocking)

None.

### Moderate

1. **The planned source-filter accessibility wiring does not satisfy the delta’s per-control association requirement.** The specification requires “each filter control” to be associated with its group’s scope statement (`openspec/changes/issue-207-filter-scope-split/specs/connector-browser/spec.md:52-55`). For column filters, the design explicitly puts `aria-describedby` on every input (`design.md:43-48`). For source filters, however, it places `aria-describedby` only on the ancestor `<fieldset>` and relies on the legend being announced (`design.md:37-42`). An `aria-describedby` relationship on a fieldset is not inherited by its descendant inputs; the existing ordinary and tag inputs have only their individual `aria-label` attributes (`ui/src/pages/connect/ConnectorBrowser.tsx:491-504`, `ui/src/pages/connect/ConnectorBrowser.tsx:518-526`). Consequently, the planned DOM would expose the source group description but would not mechanically associate that description with each source-filter input as the scenario requires.

### Suggestions

None.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Amend `design.md` so every source-filter input, including the tag input, is programmatically associated with the source-scope description—for example, by giving each input an `aria-describedby` reference in addition to retaining the fieldset/legend grouping. Specify a regression assertion that checks the accessible description of representative ordinary and tag filter inputs.

CHANGES_APPLIED: yes

## Rebuttals

**Required Change 1 — fixed.** `design.md` "The source group is a `fieldset`" now puts
`aria-describedby` on every input in the source group, the tag input explicitly included, and states
why a reference on the `fieldset` does not reach the controls' accessible description. The refine
group's paragraph was updated in the same pass to name that association as the same mechanism, and a
Risks entry adds the regression assertion: the accessible description of the Search input, of the tag
input, and of one column filter box. Those three edits are the whole of what changed after the round-1
verdict.

Reviewer re-check, codex, scoped to Required Change 1 only:

> RECHECK: SATISFIED
> REASON: design.md requires `aria-describedby` on "each input in the group," explicitly including the
> tag input, and specifies regression assertions for the accessible descriptions of both Search and
> tag inputs.
