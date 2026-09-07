## Why

Six citations inside published specs name ADRs numbered above 0057, and [#378](https://github.com/pfa230/labeler/issues/378) deletes exactly that set. Repointing them first is what keeps #378 from leaving six dangling references, and two of the six are load-bearing rather than parenthetical, so they cannot be dropped and cannot wait.

Implements [#379](https://github.com/pfa230/labeler/issues/379).

DELIVERABLE: spec-only

## What Changes

Each of the six citations is repointed at the `openspec/changes/archive/` folder that owns the decision. Nothing else in any of the five requirements moves: no rule is added, withdrawn, weakened or restated, and every sentence around a repointed pointer keeps its wording, with one exception. At `colour-vocabulary:164` the published citation is `ADR-0092 §6`, and the `§6` anchor is dropped: a folder has no section numbering, so there is no folder equivalent to carry it. That loss is accepted in design.md under Risks / Trade-offs, on the ground that the same sentence already states what the divergence was.

| Site | Cites | Repointed at |
|---|---|---|
| `flow-layout/spec.md:339` | ADR-0082 | `openspec/changes/archive/2026-08-27-issue-226-unify-size-resolution` |
| `flow-layout/spec.md:559` | ADR-0082 | `openspec/changes/archive/2026-08-27-issue-226-unify-size-resolution` |
| `layout-sizing/spec.md:743` | ADR-0059 | `openspec/changes/archive/2026-08-21-issue-180-auto-length-text-alignment` |
| `layout-sizing/spec.md:781` | ADR-0084 | `openspec/changes/archive/2026-08-28-issue-245-center-ink-reserve` |
| `layout-sizing/spec.md:993` | ADR-0058 | `openspec/changes/archive/2026-08-21-issue-181-duplicate-id-not-fatal` |
| `colour-vocabulary/spec.md:164` | ADR-0092 | `openspec/changes/archive/2026-08-31-issue-280-shape-paint-model` |

The pointer names the **folder**, never a file inside it: the rationale is split across each folder's `proposal.md` and `design.md`, so the folder is the stable address and no filename has to be guessed.

At `layout-sizing:781` the clause ends up naming two ADRs and one archive folder. That mixture is intended and is not smoothed over: ADR-0045 and ADR-0050 predate OpenSpec, survive #378, and have no change folder, so an ADR number is the only address they have.

Not **BREAKING**: no requirement changes meaning, so no template, request, response or rendered label changes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `flow-layout`: two requirements whose parenthetical provenance for the `text` `overflow` policy cites ADR-0082.
- `layout-sizing`: two requirements, one carrying a supersession record naming ADR-0059 and a normative clause naming ADR-0084, the other a normative clause naming ADR-0058.
- `colour-vocabulary`: one requirement carrying a supersession record naming ADR-0092.

Each is a MODIFIED delta reproducing the complete requirement, because that is the only shape MODIFIED takes; the edits are six pointer swaps across five requirements, one of which carries two. `layout-sizing`'s "Text is laid out against the box it will get, and what does not fit is authored" holds both the ADR-0059 supersession record and the ADR-0084 metric-model clause; the other four requirements carry one swap each.

## Impact

- `openspec/specs/flow-layout/spec.md`, `openspec/specs/layout-sizing/spec.md`, `openspec/specs/colour-vocabulary/spec.md`, rewritten by archive from this change's delta.
- No `src/`, no `ui/`, no `tests/`, no `docs/`. `docs/adr/` is frozen and is not touched here; #378 deletes from it separately.
- Unblocks #378.
