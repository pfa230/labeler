## Why

[Issue #210](https://github.com/pfa230/labeler/issues/210). In the light palette `--accent` `#e4572e`
on `--accent-soft` `#fbe9e2` measures **3.13:1** by the WCAG 2.x relative-luminance formula, under the
4.5:1 that AA asks of normal-size text. Split out of #201, which fixed its own badge with a deeper
`--accent-deep` and left the app-wide pair alone because re-tinting it needs its own screenshots.

Measuring the rest of the palette widened the defect in two directions the issue records only as open
questions:

- `--accent` fails AA as text on **every** light background, not only the tint: 3.13 on
  `--accent-soft`, 3.68 on `--surface`, 3.47 on `--paper`. Ten locations paint text in `--accent`:
  four over the tint (`Catalog.tsx:42`, `TemplateDetail.tsx:300`, `ConnectorBrowser.tsx:456`,
  `Shell.tsx:23`), five over `--surface` (`Catalog.tsx:186`, `TemplateDetail.tsx:79`,
  `ConnectorBrowser.tsx:598`, `Shell.tsx:69`, `Shell.tsx:134`), and the favourite star
  (`Templates.tsx:164`), which sits over `--surface` or over the tint depending on whether its card is
  selected. Fixing the tint pairing alone leaves the six that never touch the tint.
- `--accent-ink` is never defined. Nineteen call sites read `var(--accent-ink, #fff)`, so every
  primary control in the app renders white on the accent fill: **3.68:1 in light and 2.80:1 in dark**.
  That is the wider failure of the two, it is the one users read whole button labels through, and it
  is the only one where dark mode is the bad case.

Half-fixing this ships a change titled "fix the accent contrast" that leaves nineteen buttons under AA
in both themes, and leaves the next reader assuming the palette was audited.

## What Changes

- **Darken the light `--accent` from `#e4572e` to `#b8420f`**, so one colour serves as both text and
  fill. Light-mode ratios move to 4.67 on `--accent-soft`, 5.49 on `--surface`, 5.17 on `--paper`, and
  5.49 for white on the fill. Dark-mode `--accent` `#f0784f` is already 5.18 or better as text and is
  not touched.
- **Retire `--accent-deep`.** Once `--accent` is `#b8420f` in light and `#f0784f` in dark, the token
  holds the same value as `--accent` in both palettes. Two names for one paint is dead weight, and two
  near-identical oranges rendered side by side read as a mistake rather than a system. The format
  badge (#201, ADR-0066) repoints to `--accent`.
- **Define `--accent-ink` in both palettes**: `#ffffff` in light (5.49 on the new accent) and `#16140f`
  in dark (6.58 on `#f0784f`). Drop the `, #fff` fallback from all nineteen call sites, so no control's
  label colour is decided by a literal repeated at the call site.
- **Extend `ui/src/theme.test.ts`** to assert the invariant rather than today's badge-only subset: the
  accent must clear 4.5:1 over every background it carries text on, `--accent-ink` must clear 4.5:1
  over `--accent`, and both must hold in both palettes.
- **Re-screenshot the UI in both themes.** Every accent surface re-tints: primary buttons, the selected
  card border, the group filter chips, the favourite star, the active nav item, the accent-tinted chips,
  and, in the connector grid, the focused-cell outline and the column-resize grip.

Not a breaking change to any published contract: `--accent-deep` and `--accent-ink` are internal CSS
custom properties on this app's own stylesheet, with no documented theming API over them. The visible
break is aesthetic and deliberate: `#b8420f` is a perceptibly deeper rust than the current vermilion,
in the same hue family.

## Capabilities

### New Capabilities

- `ui-colour-palette`: what the accent colour must hold to as text, as a control fill and as a
  non-text indicator; that one accent serves every accent role; and that the ink on the accent fill is
  a defined token rather than a call-site fallback. Frozen `docs/SPEC.md` documents the REST service
  and template schema and says nothing about the web UI's palette, so this capability supersedes no
  section of it.

### Modified Capabilities

None. `template-format-badge` states its legibility requirement over resolved colours, never over
token names, so repointing the `single` badge's foreground from `--accent-deep` to `--accent` leaves
every one of its requirements satisfied: the two badges still resolve to different text colours
(`#b8420f` against `#2f6f7d`) and different fills, the sheet badge's colour is still distinct from the
accent, and both still clear 4.5:1 over every background they appear over.

## Impact

- `ui/src/theme.css`: light `--accent`; `--accent-ink` added to both blocks; `--accent-deep` removed
  from both blocks.
- Nineteen call sites across twelve files drop the `, #fff` fallback (`Login`, `Templates` ×6,
  `Catalog` ×2, `Connect`, `TemplateDetail` ×2, `Import`, `Setup`, `PrintersSection`, `NewTemplate`,
  `ConnectionsSection`, `EmptyTemplates`, `PrintForm`).
- `ui/src/components/FormatBadge.tsx`: the `single` foreground.
- `ui/src/setupTests.ts`: `noBadgeStyling` rejects `--accent-deep` by name; with the token gone the
  clause matches nothing and the guard silently weakens.
- Three test files assert the badge paints itself `var(--accent-deep)` and must be repointed with it:
  `FormatBadge.test.tsx:80`, `Templates.test.tsx:147,149`, `TemplateDetail.test.tsx:245,247`.
- `ui/src/theme.test.ts`: three existing references to `accent-deep` are **replaced**, not added to.
  `REQUIRED:36` swaps it for `accent-ink`, `FOREGROUNDS:42` swaps it for `accent`, and the
  resolved-colour comparison at `:57` compares `accent` against `info`. The new assertions
  (`--accent-ink` over `--accent`; the accent as a non-text mark at 3:1; no second accent shade) are
  added alongside.
- `docs/adr/`: a new ADR, and its row in `docs/adr/README.md`.
- No Rust, no API, no template schema. UI-only and visual, so it needs screenshot evidence in both
  themes alongside the test.
