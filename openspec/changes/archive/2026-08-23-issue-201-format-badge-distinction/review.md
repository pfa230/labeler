## Review Metadata

- **Round**: 3
- **Prior round**: REVISE (Critical on the selected-card chip collision, six Moderates on test coverage and citations)

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only; GitHub API unavailable due network failure
- **Artifacts reviewed**: proposal.md, specs/, design.md; all cited UI source/tests, theme/vitest configuration, CI workflow, frozen and OpenSpec specs, and ADR index
- **Issue**: #201

## Findings

### Critical (blocking)

None. All eight displayed contrast rows recompute correctly under WCAG 2.x.

### Moderate

1. **The colour tests do not bind the component to the tokens whose resolved values are tested.** The design deliberately excludes colour assertions from `FormatBadge.test.tsx`, while `theme.test.ts` only compares palette tokens (`design.md:250-268`). An implementation could assign `--accent` to the sheet badge, swap the variants, or use another failing token while every described theme test still passed. This leaves the resolved-colour, 4.5:1, status-colour, and border-delineation requirements unproved (`specs/template-format-badge/spec.md:21-23,125-137`). jsdom preserving literal `var(...)` values is not an obstacle: component tests can assert the exact token mapping, while the theme test independently resolves those tokens.

2. **The grid/detail parity assertions still do not prove identical icons or colours.** The proposed integration tests assert text, `data-format`, and rectangle count only (`design.md:263`). Two six-cell SVGs with different geometry, or badges with different colour assignments, would pass despite the requirement that both surfaces render the same icon and colours (`specs/template-format-badge/spec.md:85-106`). Rectangle attributes, `aria-hidden`, inline token references, and `data-format` are all inspectable in this repository’s jsdom/@testing-library setup.

3. **The prose-exclusion coverage omits one of the three normative exclusions and incompletely checks the other two.** The spec says the catalog, Dimensions row, and preview fallback gain no badge, icon, colour treatment, or count (`specs/template-format-badge/spec.md:93-101`). The assertion table covers only catalog and Dimensions, checking chiefly marker/count (`design.md:264-265`); it has no PreviewPane row. The existing sheet-preview test only checks that an `<object>` exists (`ui/src/components/PreviewPane.test.tsx:13-16`) and does not preserve “Open sheet preview” or exclude badge markup.

4. **The contrast discussion mixes the displayed table with a larger cross-product matrix.** The eight displayed rows are correct: `4.67/4.78`, `5.18/5.80`, `5.49/5.69`, `5.17/5.36`, `4.67/4.84`, `6.07/7.22`, `6.58/7.82`, and `5.18/6.16`. The displayed minimum is 4.67, while 4.61 is correctly obtained from the additional, non-occurring light-mode pairing `--accent-deep`/`--info-soft`. Likewise, 0.11 and 0.63 are specifically the own-fill gaps; displayed surface-row gaps reach 0.20 light and 1.24 dark. Calling 4.61 the worst case “across the whole table” and stating the spreads without that qualification is internally misleading (`design.md:168-186`; `proposal.md:47-48`).

5. **The border satisfies the current backgrounds, but the design overclaims that it works over “any background by construction.”** On the selected card, the proposed single border has 4.67:1 light and 5.18:1 dark against `--accent-soft`; the sheet border has 4.84:1 and 6.16:1. Its own-fill contrasts are also strong, so the 1px border genuinely delineates the current cases. It is not guaranteed against an arbitrary future background equal or close to the foreground, however, and the spec still enumerates current backgrounds (`design.md:114-125`; `specs/template-format-badge/spec.md:121-133`). The implementation is sound; the universal rationale is not.

6. **Several factual claims or citations remain wrong or incomplete.**
   - The group chip is not “wordless”; it renders `template.group` (`design.md:127-129`; `ui/src/pages/Templates.tsx:87-93`).
   - `ui/src/api/types.ts:5` shows the sheet union’s `positions`, but does not establish by itself that both `TemplateSummary` and `TemplateDetail` use that union; those uses are at `ui/src/api/types.ts:37-55` (`proposal.md:95-96`; `design.md:32-35`).
   - `Templates.tsx:126-131` does not contain the claimed `min-w-0` row; that class is on line 124 (`design.md:315-318`).

7. **The known app-wide accent contrast defect is left as prose backlog instead of a tracked issue.** The proposal and design repeatedly say it “should be filed” later (`proposal.md:49-54`; `design.md:67-71,319-321`), while `AGENTS.md:34-38` requires deferred work to become a GitHub issue rather than a TODO in documentation.

### Suggestions

- Scope remains cohesive for #201: exactly two production sites currently render a format badge, and the three identified running-prose sites are complete. The YAML placeholder in `NewTemplate.tsx:6-19` is configuration content, not a fourth prose presentation.
- The planned block-then-semicolon CSS parser is compatible with `theme.css:5-11` and the Vitest working directory. Adding the three new declarations inside both palette blocks will make all named tokens extractable.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Add component-level assertions binding each format to its exact foreground, fill, and border token; retain the theme tests as the independent resolved-value half of the proof.
2. Strengthen grid/detail parity checks to compare the SVG geometry and colour-token assignments, not only rectangle count, text, and `data-format`.
3. Cover all three prose exclusions, including PreviewPane, and assert absence of icon/badge/count/format colour treatment as applicable.
4. Rewrite the contrast prose to distinguish the eight displayed rows from the expanded cross-product test matrix and qualify the 0.11/0.63 spreads as own-fill comparisons.
5. Narrow the “any background by construction” claim to the specified/current backgrounds while retaining the computed border evidence.
6. Correct the “wordless” group-chip claim and the two incomplete line citations.
7. File and link a separate GitHub issue for the deferred app-wide `--accent`/`--accent-soft` contrast defect.

CHANGES_APPLIED: yes

## Rebuttals

None.

All seven Required Changes applied and re-checked by the reviewer in a scoped pass
(`RECHECK: ALL RESOLVED`). Items 3 and 5 came back NOT RESOLVED on the first re-check and were fixed
again before the second.

1. **Fixed.** `FormatBadge.test.tsx` now pins the exact foreground, fill and border token each format
   binds to, with `theme.test.ts` kept as the independent resolved-value half. `design.md` states why
   neither half closes the chain alone.
2. **Fixed.** Grid and detail parity now compares every `rect`'s `x`/`y`/`width`/`height` and all three
   colour-token references, not rect count and text.
3. **Fixed** (second pass). All three prose exclusions, catalog, Dimensions row and `PreviewPane`, now
   assert absence of marker, `svg`, position count and format colour tokens.
4. **Fixed.** The eight displayed rows are separated from the wider cross-product matrix the theme test
   asserts; the displayed minimum is 4.67:1 and 4.61:1 is named as a non-occurring pairing. The 0.11
   and 0.63 spreads are qualified as own-fill, with the shared-background spreads given as 0.20 and
   1.24.
5. **Fixed** (second pass). `design.md` and `proposal.md` both now limit the border claim to the three
   backgrounds that occur, give the measured ratios, and disclaim arbitrary future backgrounds.
6. **Fixed.** The group chip is described as neutral and iconless, carrying the group's name
   (`Templates.tsx:92`); the API types citation is `types.ts:5,37-56`; the `min-w-0` row is
   `Templates.tsx:124`.
7. **Fixed.** Filed [#210](https://github.com/pfa230/labeler/issues/210) for the app-wide
   `--accent` / `--accent-soft` defect and [#211](https://github.com/pfa230/labeler/issues/211) for the
   catalog's format presentation, and linked both from the artifacts in place of the prose backlog.
