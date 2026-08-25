## Review Metadata

- **Round**: 1
- **Prior round**: none

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/ui-colour-palette/spec.md, design.md, ui/src/theme.css, ui/src/theme.test.ts, ui/src/lib/contrast.ts, ui/src/setupTests.ts, ui/src/components/FormatBadge.tsx, ui/src/components/FormatBadge.test.tsx, ui/src/components/PreviewPane.tsx, ui/src/components/ParamInput.tsx, ui/src/components/EmptyTemplates.tsx, ui/src/pages/Login.tsx, ui/src/pages/Templates.tsx, ui/src/pages/Templates.test.tsx, ui/src/pages/Catalog.tsx, ui/src/pages/Connect.tsx, ui/src/pages/TemplateDetail.tsx, ui/src/pages/TemplateDetail.test.tsx, ui/src/pages/Import.tsx, ui/src/pages/Setup.tsx, ui/src/pages/NewTemplate.tsx, ui/src/pages/connect/ConnectorBrowser.tsx, ui/src/pages/settings/PrintersSection.tsx, ui/src/pages/settings/ConnectionsSection.tsx, ui/src/pages/print/PrintForm.tsx, ui/src/app/Shell.tsx, docs/adr/0066-format-badge-carries-icon-colour-and-count.md, docs/adr/README.md, AGENTS.md, openspec/config.yaml, openspec/specs/template-format-badge/spec.md
- **Issue**: #210

## Findings

### Critical (blocking)

None.

### Moderate

1. The rejected lower-churn alternative rests on a false contrast claim. `design.md:59-62` says that no ink can fix labels on the existing `#e4572e` fill without moving that fill. By the stated WCAG 2.x formula, `#16140f` on `#e4572e` is 4.9966:1, and the existing light `--ink` `#1c1a17` is 4.7139:1. A viable alternative therefore is to retain the current control fill, use the existing deeper orange for accent text, and select dark ink for light-mode controls. ADR-0066 already records the two-orange arrangement as an accepted trade-off (`docs/adr/0066-format-badge-carries-icon-colour-and-count.md:81-85`). The chosen consolidation may still be defensible, but the artifacts must compare it honestly and justify why the extra visual churn and ADR supersession are warranted.

2. The new specification contradicts both itself and the planned palette. Its operative text narrowly prohibits a second accent “for text” (`specs/ui-colour-palette/spec.md:75-85`), but its scenario requires the palette to offer “exactly one accent colour” (`spec.md:94-98`). The plan explicitly retains `--accent-soft` as a second accent shade (`design.md:34-38`), and both palettes currently define it (`ui/src/theme.css:7,12`). The scenario must be narrowed to one primary accent value shared by text and control-fill roles; as written, the shipped design cannot satisfy it.

3. The specification promotes internal CSS mechanics into a behavioral contract despite the proposal saying these properties are internal and not a published theming API (`proposal.md:41-42`). Requiring a “dedicated ink” and forbidding call-site fallback syntax (`spec.md:48-53,69-73`) distinguishes implementations that render identically and meet the same contrast ratio. The behavioral requirement should say that every accent-filled control resolves its label colour from the palette and meets AA; the literal-token and fallback-cleanup choices belong in design, ADR, tasks, and implementation verification.

4. The token-dependency inventory is incomplete and would leave three existing tests asserting a deleted token. The proposal lists only `FormatBadge.tsx`, `setupTests.ts`, and `theme.test.ts` (`proposal.md:71-74`), while `FormatBadge.test.tsx:75-81`, `Templates.test.tsx:142-149`, and `TemplateDetail.test.tsx:242-247` all require `var(--accent-deep)`. In addition, `design.md:117-121` says the foreground set “gains” `accent`, but the current set contains the soon-to-be-deleted `accent-deep` (`theme.test.ts:42`) and the resolved-colour comparison also names it (`theme.test.ts:56-58`). The plan must explicitly replace those references, not merely add `accent`.

5. The claimed “whole invariant” test coverage is not designed. `design.md:28-31,115-121` promises a theme test covering the entire capability, but the described assertions cover only contrast ratios and required-token presence. They do not detect reintroducing `--accent-deep`, another primary accent shade, or a call-site fallback, despite `spec.md:69-73,94-98` making those normative. Either add mechanical checks for every retained invariant or narrow the testing claim and remove implementation-only rules from the capability.

6. The accent-text inventory is factually wrong. `proposal.md:11-13` says seven call sites, four on surface or paper. There are ten source locations: `Templates.tsx:164`; `Catalog.tsx:42,186`; `TemplateDetail.tsx:79,300`; `ConnectorBrowser.tsx:456,598`; and `Shell.tsx:23,69,134`. Four use `--accent-soft`; six source locations use surface/paper contexts. Although changing the shared token reaches all of them, the count is being used to justify scope and screenshot coverage and must be corrected or explicitly deduplicated by rendered role.

### Suggestions

- The advertised contrast ratios were recomputed and are correct to two decimals, including 3.13, 3.68, 3.47, 4.67, 5.49, 5.17, 5.18, 6.07, 6.58, 2.80, 4.57, 6.86, and 4.01.
- The nineteen `var(--accent-ink, #fff)` occurrences are present across twelve files, and each is paired with an accent background under the same condition.
- Deleting `--accent-deep` does not violate `openspec/specs/template-format-badge/spec.md`: that capability constrains resolved badge colours, and the proposed accent and info values remain distinct.
- The three prose format sites support the design’s claim: `Catalog.tsx:56` uses `--ink`, `TemplateDetail.tsx:272` has no inline colour, and `PreviewPane.tsx:31` has no inline style.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Correct the false statement that no ink can make the existing `#e4572e` fill pass, include the computed passing dark-ink alternative, and give an accurate rationale for still choosing palette consolidation.
2. Narrow the “exactly one accent colour” requirement and scenario to one primary accent shared by text and control-fill roles, so retained `--accent-soft` does not contradict the contract.
3. Express the control-label contract in observable palette/contrast terms; move CSS fallback and literal-token mechanics out of the normative specification.
4. Add the three badge tests and every affected `accent-deep` assertion to the impact/design, explicitly replacing rather than merely augmenting the theme-test foreground references.
5. Align the verification design with every retained invariant by adding mechanical checks or narrowing the “whole invariant” testing claim.
6. Correct or explain the seven-call-site inventory and use the accurate inventory when defining screenshot coverage.

CHANGES_APPLIED: yes

## Rebuttals

Author (claude), on each of the six Required Changes:

1. **Fixed, after taking it to the human.** The claim was false: `#16140f` on `#e4572e` is 5.00, so
   the lower-churn alternative does reach the buttons. The human weighed the corrected comparison and
   chose to keep the consolidation. `design.md` now records the alternative as viable and rejects it
   on what it leaves behind (dark-on-orange primary buttons, a permanent two-orange split ADR-0066
   deferred to this issue) rather than on a false impossibility.
2. **Fixed.** `spec.md` narrows the rule to the text and fill roles and excludes the tint explicitly.
3. **Fixed.** The dedicated-token and fallback-syntax rules left the spec; it now requires only that
   the label colour is a property of the palette and clears the ratio in each.
4. **Fixed.** `proposal.md` and `design.md` name all three badge test files and state that the three
   `theme.test.ts` references are replaced, not added to.
5. **Fixed.** The coverage claim is narrowed, and the `accent-deep`-absent assertion is described as a
   regression guard rather than as enforcement of the general one-accent rule.
6. **Fixed.** Ten locations, listed by file:line, and the screenshot coverage derives from them.

A ratio introduced while applying (1) was itself wrong and was caught on re-check: `#16140f` on
`#b8420f` is 3.35, not 8.60. Corrected, and the correction changed the argument: no dark ink reaches
AA on the darkened accent, so white is the only viable light-mode ink.

## Re-check (round 1)

_Re-checked by: codex_

1. APPLIED WITH A NEW PROBLEM - `design.md:59-77` correctly recognizes the dark-ink alternative as viable—recomputed ratios are 5.00:1 and 4.71:1—and gives a consolidation rationale. However, `design.md:124` incorrectly claims `#16140f` on `#b8420f` is 8.60:1; it is 3.35:1.
2. APPLIED - `specs/ui-colour-palette/spec.md:75-103` now limits the one-colour rule to the primary accent used for text and fills, and explicitly excludes the tint at `:85-87`.
3. APPLIED - `specs/ui-colour-palette/spec.md:44-73` now specifies palette-resolved label colour and AA contrast without requiring CSS token names or forbidding fallback syntax.
4. APPLIED - `proposal.md:78-84` and `design.md:133-150` identify all three badge-test files and explicitly replace all three `theme.test.ts` references to `accent-deep`.
5. NOT APPLIED - `design.md:30-31` still claims mechanical coverage of the one-accent rule, but the proposed check at `design.md:146-147` only forbids the specific name `accent-deep`; it would not detect another second accent shade, as required by `spec.md:77-79,99-103`.
6. NOT APPLIED - The accurate ten-location inventory and screenshot coverage now appear at `proposal.md:11-17` and `design.md:177-186`, but `design.md:46` still incorrectly describes “seven text call sites.”

**New problems introduced by the edits:** `design.md:124` gives 8.60:1 for `#16140f` on `#b8420f`; the WCAG 2.x relative-luminance result is 3.35:1.

RECHECK_RESULT: OUTSTANDING

## Re-check (round 1, pass 2)

_Re-checked by: codex_

1. APPLIED - The corrected ratio is 3.35:1 under the WCAG 2.x formula, matching `design.md:124-126`.
5. APPLIED - The design now says the check guards only against `accent-deep`, while the general one-accent rule remains review- and ADR-enforced (`design.md:151-155`).
6. APPLIED - The design consistently identifies ten text call sites (`design.md:46`, `design.md:186-188`).

**New problems introduced by the edits:** none

RECHECK_RESULT: ALL_APPLIED

---

## Post-implementation rounds

The verdict above covers the plan. After the apply stage, the diff review found an ADR number
collision and a set of claims about the connector grid that were not true of the shipped app. Fixing
those edited proposal.md, specs/, design.md and the ADR, which voids the plan verdict staleness
guarantee, so the reviewer was re-run on the edits. Three passes; the author own corrections were
wrong in the first two.

## Post-implementation artifact re-check

_Re-checked by: codex_

1. PROBLEM - 0071 is free: `main`, all Git refs/history, and all five sibling worktrees contain no 0071; issue-197 alone has untracked 0070. The filename, heading (`docs/adr/0071-one-accent-colour-with-a-defined-ink.md:1`), README rows (`docs/adr/README.md:79,82`), and design (`design.md:195-203`) agree, but checked tasks still contain `00NN` placeholders (`tasks.md:45,52`).
2. OK - The four identified mappings cannot fire. `select={false}` disables `.wx-selected` (`ConnectorBrowser.tsx:655-660`); no row dragging is configured; and the imported vendor CSS applies `--wx-background-hover` only to `.wx-icon.wxi-close:hover`, with no `.wx-row:hover` rule (`ui/node_modules/@svar-ui/react-grid/dist/index.css:1`). The configured columns provide no component that mounts that close icon (`ConnectorBrowser.tsx:404-435`).
3. PROBLEM - Live-grid claims remain in `proposal.md:42-43` and `tasks.md:69-72`, contradicting `design.md:187-188`. The spec also retains selected-row tint wording (`spec.md:14,113`). Additionally, `design.md:177-178` and the ADR at line 36 incorrectly say the imported stylesheet uses `.wx-item.wx-focus`; it does not. The broader “no live accent surface” wording is also too strong: `theme.css:43` maps `--wx-color-primary` to the accent, and the vendor CSS uses it for focused cells and resize grips.
4. OK - Keeping the inert mappings is defensible and preferable here. The mapping is genuinely complete—41 unique vendor variables and the same 41 in `theme.css`—and `theme.css:29-38` explicitly documents the inert cases and future-feature rationale. Deleting only these mappings would make later vendor-feature activation fall back to vendor colours.
5. OK - The selected template card remains a real non-text subject (`spec.md:105-128`): selection paints `--accent-soft` plus an independent `--accent` border (`ui/src/pages/Templates.tsx:63-64`). `theme.test.ts:64-66` mechanically checks that border against `--surface`; recomputed ratios are 5.491268:1 light and 6.071891:1 dark.

**Other problems found in these edits:** none

RECHECK_RESULT: OUTSTANDING

## Post-implementation artifact re-check, pass 2

_Re-checked by: codex_

1. FIXED - `tasks.md:42-52` consistently assigns ADR-0071 with no `00NN` placeholders; `docs/adr/0071-one-accent-colour-with-a-defined-ink.md:1-7` confirms the numbered, accepted ADR.
3. FIXED WITH A NEW PROBLEM - The row-selection claims are corrected in `proposal.md:42-44`, `tasks.md:69-74`, `spec.md:108-128`, `design.md:167-200`, and ADR-0071:36-45. The imported stylesheet:1 does paint focused-cell outlines and resize-grip marks with `--wx-color-primary`. However, `theme.css:63` gives header cells `--paper`, so the resize grip—and potentially a focused header cell—is not over `--surface`; therefore `design.md:188-200`, ADR-0071:38, and `theme.css:40-42` overstate coverage by the surface-only 3:1 assertion at `theme.test.ts:64-65`.

**New problems introduced by these fixes:** The claim that both grid marks are over `--surface` and covered by its 3:1 assertion is inaccurate: the imported stylesheet:1 places the resize grip in the header cell, while `theme.css:63` maps the header background to `--paper`. The separate accent-on-paper assertion at `theme.test.ts:42-57` does numerically cover that contrast at the stronger 4.5:1 threshold, but not via the claimed 3:1 surface assertion.

RECHECK_RESULT: OUTSTANDING

## Post-implementation artifact re-check, pass 3

_Re-checked by: codex_

3. FIXED - The artifacts correctly distinguish a focused body cell over `--surface` from a focused header cell and resize grip over `--paper` (`design.md:185-195`; `0071-one-accent-colour-with-a-defined-ink.md:38`). The CSS maps the grid body background to `--surface`, primary colour to `--accent`, and header background to `--paper` (`ui/src/theme.css:46-48,64`). The test cross-products `accent` with `paper` and enforces at least 4.5:1 (`ui/src/theme.test.ts:42-43,54-57`). Recomputed WCAG ratios are 5.173813:1 light and 6.576732:1 dark for accent on paper, both exceeding 4.5:1 and therefore 3:1. Declining a redundant dedicated 3:1 paper assertion is sound.

**New problems introduced by this fix:** none

RECHECK_RESULT: ALL_OK

---

## Final full-diff round

The rounds above each judged a slice: the plan, then the artifact corrections the diff review
forced. This one judged the complete final diff, tracked and untracked, with the reviewer told not to
lean on the earlier passes. It found three Moderate and two Suggestions, all real. Two of the fixes
were themselves wrong and were caught on re-check.

## Final full-diff review

_Reviewed by: codex_

- **Scope**: complete final diff, tracked and untracked
- **Files read**: `AGENTS.md`; `docs/SPEC.md`; `docs/adr/README.md`; ADR-0066; ADR-0071; all five files under `openspec/changes/issue-210-accent-contrast-aa/`; `openspec/specs/template-format-badge/spec.md`; all 20 tracked UI files in the diff; `ui/src/lib/contrast.ts`; `ui/src/app/Shell.tsx`; `ui/src/components/ParamInput.tsx`; `ui/src/pages/connect/ConnectorBrowser.tsx`; `ui/package.json`; `ui/package-lock.json`; `ui/node_modules/@svar-ui/react-grid/dist/index.css`; `.agy-apply.log`; `agy-apply.stdout`; `gapshots.mjs`; all 16 light/dark screenshots under `/tmp/screenshots/`

## Findings

### Critical (blocking)

None.

### Moderate

1. **The theme test can pass while the browser uses a broken palette.** `theme.test.ts:17-29` uses one non-global `RegExp.exec`, so it reads only the first matching `:root` or `.dark` block; `THEMES` fixes those first matches at `theme.test.ts:32`. A later `:root { --accent: #e4572e; }` or `.dark { --accent: #e4572e; }` wins in the CSS cascade but is invisible to the test, which continues testing `#b8420f` and `#f0784f`. Thus the test can pass against shipped colours that fail AA, contrary to `specs/ui-colour-palette/spec.md:130-146`. Task 4.5 only proves that replacing the value in the first block fails (`tasks.md:36-38`); it does not close this failure mode.

2. **The ADR and design falsely claim pure white is the only passing light-mode ink.** `design.md:124-132` and ADR-0071:31 say white is the “sole” passing ink and that opposite inks are forced. The stated dark-ink results are correct—`#16140f` is 3.3514:1 and black is 3.8243:1 on `#b8420f`—but they prove only that a lighter ink is required. Existing palette colours such as `#faf8f3` pass at 5.1738:1, and `#f2efe7` passes at 4.7790:1. White is a sensible choice, but not forced. This also leaves checked task 5.2’s “white is forced” claim unsupported (`tasks.md:45-48`).

3. **Checked visual task 7.1 was not performed in the state it specifies.** It requires the catalog to be screenshotted “with an installed entry” (`tasks.md:63-66`). Both `/tmp/screenshots/03_catalog_light.png` and `03_catalog_dark.png` show only `Install` buttons. `Catalog.tsx:77` renders `Reinstall` for an installed entry, proving none was installed in those shots. The template evidence is also split: screenshots 01 show a selected card but no filled favourite star, while screenshots 08 show favourites but no selected card. Visual inspection occurred, but the checked box overstates what was captured.

### Suggestions

1. `theme.css:27` says scoping the variables to `.connector-grid-viewport` prevents “the vendor stylesheet” from leaking elsewhere. The stylesheet itself is imported globally at `ConnectorBrowser.tsx:3`; only this app’s vendor-token overrides are scoped. Reword the sentence to distinguish the global vendor rules from the scoped palette mapping.

2. Correct the screenshot inventory in `design.md:229-239`. Runtime TSX contains 36 exact `var(--accent)` references across 16 files, not 38. The six listed screens also do not cover all references: `Setup.tsx:62`, `Import.tsx:559`, `NewTemplate.tsx:67`, `PrintForm.tsx:234`, and `EmptyTemplates.tsx:28` are among the omitted states.

## What I checked and found correct

- Recomputed the WCAG 2.x ratios independently. The original light accent measures 3.1317:1 on `--accent-soft`, 3.6832:1 on `--surface`, and 3.4703:1 on `--paper`; white on the old light and dark fills measures 3.6832:1 and 2.7983:1.
- The new light accent measures 4.6690:1 on `--accent-soft`, 5.4913:1 on `--surface`, and 5.1738:1 on `--paper`; white ink measures 5.4913:1. Dark accent measures 5.1790:1, 6.0719:1, and 6.5767:1 on those grounds, with dark ink at 6.5767:1. Both palettes therefore fix #210’s text, control-label, and non-text contrast failures.
- All nineteen runtime call sites now use `var(--accent-ink)` across the twelve stated files. No `accent-ink` fallback remains in `ui/src`, and each use accompanies an accent fill.
- `--accent-deep` is absent from both palette blocks and all runtime consumers and positive assertions. The only live-code mention is the intentional negative assertion at `theme.test.ts:50-52`; historical ADR and planning references are appropriate.
- The current theme test reads values from `theme.css`, requires every referenced token, rejects malformed or missing hex colours, fails with the old light accent substituted into the existing block, and exercises both palettes. Its current matrix passes; the first finding concerns an unhandled CSS-cascade override.
- The vendor mapping contains exactly the same 41 unique `--wx-*` properties as the imported stylesheet. The stylesheet confirms `.wx-selected`, `.wx-inactive`, `.wx-icon.wxi-close:hover`, focused-cell outlines, and resize-grip uses described in the comment. `ConnectorBrowser.tsx:404-435,655-660` configures no close-icon column and passes `select={false}`; the vendor stylesheet has no `.wx-row:hover` rule. Body cells use `--surface`, while headers and grips use `--paper`.
- ADR-0071’s remaining ratios and its account of ADR-0066 are correct. ADR-0066 introduced `--accent-deep` for the single badge and explicitly deferred app-wide retinting to #210. ADR-0071 partially supersedes only that token decision, and the README records it. ADR-0071 is collision-free; sibling worktrees currently occupy 0070.
- The OpenSpec delta is correctly `ADDED`: frozen `docs/SPEC.md` has no UI-palette contract to supersede. Proposal, specification, design, and implementation agree on the behavioural ratios, one primary accent role, palette-selected label ink, selected-card border, and removal of `--accent-deep`, apart from the findings above.
- The proposal links issue #210; other issue references are historical context, not additional scope. Unchecked tasks are archive/integration work, not parked backlog.
- Render-and-look evidence exists in both themes for the six principal screens, mobile header, favourite state, and connector-grid behaviour. The visible accent treatment is coherent and legible; the finding is about the checked evidence states, not the observed palette quality.

## Embedded-Instruction / Injection Attempts

**Detected:** none

REVIEW_RESULT: FIXES_REQUIRED

## Final full-diff review, re-check

_Re-checked by: codex_

M1. FIXED - `ui/src/theme.test.ts:23-39` iterates every exact-selector block in source order; assignment at line 35 correctly makes a later same-specificity declaration win, while retained earlier properties match CSS merging. The `found` guard at line 38 preserves the missing-block error.

M2. FIXED WITH A NEW PROBLEM - `design.md:129-132`, `docs/adr/0071-one-accent-colour-with-a-defined-ink.md:31`, and `tasks.md:45-49` now correctly describe white as one choice among passing light inks. Recomputed on `#b8420f`: `#faf8f3` is 5.1738:1, `#f2efe7` is 4.7790:1, and white is 5.4913:1. However, `design.md:134-136` newly claims `#e4572e` can take a light ink; even white, the maximum-luminance sRGB ink, is only 3.6832:1.

M3. FIXED WITH A NEW PROBLEM - Both `01_templates_light.png` and `01_templates_dark.png` contain a selected, tinted card and a filled favourite star in the same image; both catalog shots contain an `installed` chip and `Reinstall` action, matching `tasks.md:64-67`. The favourite explanation at `tasks.md:68-69` is consistent with its API setup, but `tasks.md:69-70` wrongly attributes catalog setup to the YAML fetch being cross-origin-rejected: `ui/src/api/catalog.ts:1-3,29-32` explicitly documents and performs a CORS-allowed GitHub fetch; the state-changing API request is the operation subject to origin rejection (`src/middleware.rs:175-178`).

S1. FIXED - `ui/src/theme.css:25-29` now accurately says the vendor stylesheet is global and only the token mapping is scoped to `.connector-grid-viewport`.

S2. FIXED - `design.md:234-250` now gives the verified count of 36 exact runtime `.tsx` references across 16 files and explicitly limits screenshot coverage to six of nineteen fills, identifying the remaining thirteen as unshot.

**New problems introduced by these fixes:** `design.md:134-136` falsely says `#e4572e` can carry a passing light ink; `tasks.md:69-70` misidentifies the CORS-allowed catalog-YAML fetch as the cross-origin-rejected operation.

RECHECK_RESULT: OUTSTANDING

## Final full-diff review, re-check pass 2

_Re-checked by: codex_

a. FIXED - `design.md:124-139` gets both directions right. Recomputed on `#e4572e`: white is 3.6832:1 and `#faf8f3` is 3.4703:1 (fail), while `#16140f` is 4.9966:1 and black is 5.7015:1 (pass). On `#b8420f`: `#16140f` is 3.3514:1 and black is 3.8243:1 (fail), while `#faf8f3` is 5.1738:1, `#f2efe7` is 4.7790:1, and white is 5.4913:1 (pass).
b. FIXED - `tasks.md:70-74` correctly distinguishes the CORS-allowed GitHub YAML fetch from the state-changing POST to this service. `Catalog.tsx:127-133` performs that sequence and `queries.ts:65-69` identifies the write as `POST /api/templates`; under `LABELER_NO_AUTH=true`, `middleware.rs:171-178` rejects state-changing requests only when `origin_present_and_mismatched` is true, and `middleware.rs:127-161` defines that as a present Origin/Referer whose authority—including port—does not match the effective Host. POST is state-changing per `middleware.rs:47-55`.

**New problems introduced by these fixes:** none

RECHECK_RESULT: ALL_OK
