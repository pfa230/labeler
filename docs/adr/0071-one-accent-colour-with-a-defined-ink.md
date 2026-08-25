# 71. One accent colour, dark enough to carry text, with a defined ink on its fill

Date: 2026-08-24

## Status

Accepted. Issue [#210](https://github.com/pfa230/labeler/issues/210). Partially supersedes [ADR-0066](0066-format-badge-carries-icon-colour-and-count.md) (the `--accent-deep` token decision only; its icon, count, and border decisions stand).

## Context

In the light palette, `--accent` `#e4572e` fails WCAG 2.x AA contrast (4.5:1) as text over every light ground in the application:
- 3.13:1 over the selection tint `--accent-soft` (`#fbe9e2`)
- 3.68:1 over the card surface `--surface` (`#ffffff`)
- 3.47:1 over the page background `--paper` (`#faf8f3`)

Ten locations paint text in `--accent` across six screens and the mobile header.

Furthermore, `--accent-ink` was never defined in `theme.css`. Nineteen primary action call sites fell back to `var(--accent-ink, #fff)`, rendering white text over the accent fill. This resulted in failing contrast ratios of 3.68:1 in light mode and 2.80:1 in dark mode (`#f0784f`), leaving primary buttons sub-AA in both themes.

ADR-0066 introduced `--accent-deep` (`#b8420f` light, `#f0784f` dark) solely to achieve legible text on the single format badge, deferring app-wide palette consolidation to #210.

## Decision

1. **Darken light `--accent` to `#b8420f`.** Dark mode `--accent` `#f0784f` is already 5.18:1 or better against all backgrounds and remains unchanged. Light mode contrast ratios move to 4.67:1 on `--accent-soft`, 5.49:1 on `--surface`, and 5.17:1 on `--paper`.
2. **Retire `--accent-deep`.** Because `--accent` is now `#b8420f` in light and `#f0784f` in dark, `--accent-deep` is redundant. `FormatBadge` and badge tests repoint to `var(--accent)`.
3. **Define `--accent-ink` in both palettes.** `#ffffff` in light (5.49:1 on `#b8420f`) and `#16140f` in dark (6.58:1 on `#f0784f`). All nineteen call sites drop the `, #fff` fallback to ensure control labels are strictly determined by the theme palette.
4. **Why palette darkening was chosen over the dark-ink alternative:**
   A lower-churn alternative was evaluated: retain `#e4572e` for control fills, promote `--accent-deep` for text, and use a dark ink (`#16140f` provides 5.00:1 on `#e4572e`) for light-mode button labels. While technically passing AA, this was rejected because:
   - Dark text on an orange primary button reads as disabled or secondary in light mode, creating visual dissonance with dark mode's light-on-dark primary buttons.
   - It would make the two-orange split permanent rather than resolving the temporary accommodation acknowledged in ADR-0066.
5. **Light mode ink constraint:** On the deepened `#b8420f` fill, no dark ink achieves AA (near-black `#16140f` yields 3.35:1; pure black `#000000` yields only 3.82:1), so the light ink must be a light one. Within that constraint white is a choice, not the only option: `--paper` `#faf8f3` reaches 5.17:1 and `#f2efe7` reaches 4.78:1. White `#ffffff` is taken because at 5.49:1 it is the highest and because a primary button's label has no reason to tint away from it. The two palettes therefore take opposite inks by necessity; which light ink is ours to pick.
6. **Enforcement and coverage:**
   - The theme test (`ui/src/theme.test.ts`) verifies that `--accent` achieves at least 4.5:1 across backgrounds as text, at least 3:1 against `--surface` as a non-text component (the selected template card's border), that `--accent-ink` achieves at least 4.5:1 against `--accent`, and that `--accent-deep` is absent.
   - The general one-accent rule (no secondary accent shade) is enforced by review and this ADR rather than mechanical assertions.

7. **The SVAR grid has no accent-tinted row, selected or hovered.** ADR-0066 lists "the SVAR grid's hover and selection rows" among the surfaces `--accent` paints. That is not true of the shipped app, and this record corrects it. Four `--wx-*` entries in `theme.css` name an accent token and none can fire: `--wx-table-select-background` and `--wx-table-select-border` style `.wx-selected`, which the grid sets only when its own row selection is on, and `ConnectorBrowser.tsx:659` passes `select={false}`; `--wx-table-drag-over-background` styles `.wx-inactive`; `--wx-background-hover` styles only `.wx-icon.wxi-close:hover`, which no configured column mounts, and the imported stylesheet has no `.wx-row:hover` rule at all. Verified against the rendered DOM with the grid populated, the column picker open, a filter typed and rows clicked: all three classes count zero. The mapping is kept rather than deleted, because `theme.css` maps the vendor's token set completely and enabling a vendor feature later should inherit our palette rather than the vendor's greys; the block's comment now records which entries are inert.

   What the grid *does* paint in the accent is `--wx-color-primary`: the focused cell's 1px outline and the column-resize grip. These are non-text marks over two grounds. A focused body cell is over `--surface`, which the 3:1 assertion covers directly. A focused header cell and the resize grip are over `--paper`, since `theme.css:63` maps the header background to it; no 3:1 assertion is added for that ground because the accent is already held to 4.5:1 against `--paper` as a text foreground, which subsumes it.

## Consequences

- All accent text, button labels, and non-text selection indicators pass WCAG AA contrast across both light and dark themes.
- The light theme accent visibly deepens from vermilion (`#e4572e`) to rust (`#b8420f`).
- `--accent-deep` is removed; all format badges and components share `--accent`.
- The non-text accent marks are the selected template card's border, held to 3:1 against `--surface` directly, and in the connector grid the focused-cell outline and the column-resize grip, held against `--surface` or `--paper` depending on which cell, per decision 7. There is no accent-tinted grid row.
