# Plan review

AUTHOR: claude
REVIEWER: codex
VERDICT: APPROVE_WITH_CHANGES
ROUNDS: 1

## Required changes

The author must apply these edits; **NO further review follows**.

1. **[verified] Preserve original grid positions when identifying invalid rows.** [design.md:82](/home/pfa/projects/labeler/.worktrees/issue-386/openspec/changes/issue-386-a-sheet-preview-renders-one-label-and-th/design.md:82) filters invalid rows before mapping their indices. That renumbers invalid rows 2 and 5 as 1 and 2, contradicting [spec.md:86](/home/pfa/projects/labeler/.worktrees/issue-386/openspec/changes/issue-386-a-sheet-preview-renders-one-label-and-th/specs/batch-grid-preview/spec.md:86). Specify `viewRows.flatMap((row, index) => rowInvalid(row) ? [index + 1] : [])`. Add a test on each page with invalid rows 2 and 5 asserting the exact message `Fix rows 2, 5 to preview the sheet.`

2. **[verified] Give pending input resolution precedence over refusal messages.** [design.md:89](/home/pfa/projects/labeler/.worktrees/issue-386/openspec/changes/issue-386-a-sheet-preview-renders-one-label-and-th/design.md:89) checks `blocked` before `rowsPending`, although [spec.md:101](/home/pfa/projects/labeler/.worktrees/issue-386/openspec/changes/issue-386-a-sheet-preview-renders-one-label-and-th/specs/batch-grid-preview/spec.md:101) requires a rendering state while inputs resolve. This matters because [labelInputs.ts:232](/home/pfa/projects/labeler/.worktrees/issue-386/ui/src/lib/labelInputs.ts:232) returns fallback inputs during resolution, which can temporarily mark a row invalid. Put `rowsPending` first in the page’s sheet-preview state selection and explicitly document that precedence in the delta. Add page tests where fallback inputs report a missing required field but resolved inputs make the row valid: show rendering without a repair message or batch request while pending, then request the preview after resolution.

3. **[verified] Restrict the single-label “nothing is previewed” rule to fallback selection.** [spec.md:33](/home/pfa/projects/labeler/.worktrees/issue-386/openspec/changes/issue-386-a-sheet-preview-renders-one-label-and-th/specs/batch-grid-preview/spec.md:33) says nothing is previewed when no row is valid, while the preserved implementation previews an explicitly selected row even when invalid ([Connect.tsx:243](/home/pfa/projects/labeler/.worktrees/issue-386/ui/src/pages/Connect.tsx:243), [Import.tsx:175](/home/pfa/projects/labeler/.worktrees/issue-386/ui/src/pages/Import.tsx:175)). State explicitly that an existing selected row is previewed regardless of validation; only absent or removed selections use the first-valid fallback, with no preview when that fallback finds nothing. Add a scenario preserving an explicitly selected invalid row and the existing service-error presentation.

CHANGES_APPLIED: yes
SPECS_SHA256: 6288e41af3ce3cfe1f3995393609e1b3f427d1c5d988201eab16001266746cc8
