## Why

Implements [#201](https://github.com/pfa230/labeler/issues/201).

`single` and `sheet` are the two things a template can be, and the badge that says which is styled
identically for both: same pill, same `--accent-soft` background, same `--accent` text, only the word
differs (`ui/src/pages/Templates.tsx:38-47`, `ui/src/pages/TemplateDetail.tsx:266-271`). Format is the
one attribute that decides how a template prints, one PNG versus a sheet of N positions, and it is the
one distinction the eye cannot pick up without reading. On the same card the group chip is also a
pill, so pill-ness already carries two unrelated meanings.

## What Changes

- **One shared `FormatBadge` component**, `ui/src/components/FormatBadge.tsx`, imported by the
  templates grid and the template detail page. Today the grid has a file-local component taking a bare
  `type: string` and the detail page duplicates the same markup inline; the new badge carries icon,
  token and count logic that must not be written twice. It takes the whole `TemplateFormat`, because
  it reads `positions` for the count.
- **Three redundant cues instead of one word**, so the two formats separate without reading and
  without relying on colour alone:
  - **Icon.** A 12px inline SVG leads the label: one rounded rect for `single`, six rects in a 2×3
    grid for `sheet`. This is the first SVG in `ui/src`, which today draws icons with Unicode glyphs
    (`★`, `ⓘ`, `▾`). The glyphs that would serve here (`▭`, `▦`) are absent from the `system-ui` stack
    and fall back per platform, so their weight and size are not ours to control. The icon is hidden
    from assistive technology; the word remains what a screen reader conveys.
  - **Colour.** `single` keeps the accent hue, `sheet` takes a new cool teal. `--good` and `--bad`
    already mean success and error and are not free to re-mean; the neutral `--bg` / `--border` pair
    is the group chip's styling.
  - **Count.** A sheet badge reads `sheet · 30`, from `positions.length`, in both places it appears.
    That makes the badge informative rather than decorative and gives the two states different
    lengths. The detail page's Dimensions row states label and paper size but never the position
    count, so this is new information there.
- **The chip is delineated by a hairline border in its own foreground colour**, not by its fill. The
  badge sits over three different backgrounds: an unselected card (`--surface`), the detail page
  (`--paper`), and a selected card, which is tinted `--accent-soft` (`ui/src/pages/Templates.tsx:73`),
  the very colour the `single` chip is filled with. A fill alone therefore cannot be what makes the
  chip visible. A border in the badge's foreground colour does not depend on the fill differing from
  what is behind it, and it is measured against each background the badge appears over today: 4.67:1
  and 5.18:1 for `single` over a selected card in light and dark, 4.84:1 and 6.16:1 for `sheet`. That
  is not a guarantee for an arbitrary future background, which is why `theme.test.ts` computes the
  matrix rather than the design asserting the border always works.
- **Three new theme tokens** in `ui/src/theme.css`, defined for `:root` and `.dark`:
  - `--info` / `--info-soft`, the teal pair the `sheet` badge uses.
  - `--accent-deep`, a deeper burnt orange used as the `single` badge's foreground and border against
    the unchanged `--accent-soft` fill. The existing `--accent` on `--accent-soft` measures **3.13:1**
    in light mode, under WCAG AA for 12px text, so leaving it would ship a known legibility failure
    inside the change whose subject is legibility. `--accent` itself is untouched, so every button,
    selected-card border, favorite star and grid selection row keeps its current colour.

  Both foregrounds clear 4.5:1 against every background the badge can appear over, in both themes;
  the lowest is 4.67:1, and 4.61:1 across the wider matrix the theme test asserts. `design.md` carries
  the full table.
- Not in this change: the group chip, the `<code>` id chip, and every other pill in the app; the
  selected card's own tint; and the app-wide `--accent` pair, whose 3.13:1 against `--accent-soft` is
  fixed for this badge by `--accent-deep` but not at its root, since `--accent` is the primary button
  background, the selected-card border, the favorite star and the SVAR grid's hover and selection
  rows, and changing it re-tints the whole UI. That root fix is tracked as
  [#210](https://github.com/pfa230/labeler/issues/210).
- Also not in this change, the three places that name a format in prose rather than as a badge: the
  template catalog listing, whose `CatalogEntry.format` is a bare string with no positions
  (`ui/src/api/catalog.ts:11-20`), so badging it would mean changing the catalog's published format,
  tracked as [#211](https://github.com/pfa230/labeler/issues/211);
  the detail page's Dimensions row, whose sentence for a sheet ends in the word `sheet`
  (`ui/src/pages/TemplateDetail.tsx:17-20`); and `PreviewPane`'s "Open sheet preview" fallback link
  (`ui/src/components/PreviewPane.tsx:29-32`).

## Capabilities

### New Capabilities

- `template-format-badge`: how a template's format is presented in the UI. Covers what the badge
  states for each format, the cues that separate them, the legibility it holds to over every
  background it appears on, and which surfaces carry a badge as against naming the format in prose.

### Modified Capabilities

None. `openspec/specs/` holds `auto-length-layout`, `connections`, `connector-browser`,
`connector-field-transforms`, `template-groups` and `template-registry`. `template-groups` specifies
the Labels view's group filter, group chip and Move action
(`openspec/specs/template-groups/spec.md:283-299`), none of which this change touches. Frozen
`docs/SPEC.md` defines format semantics at §3.1 and describes no format badge, so this capability
supersedes no section of it.

## Impact

- **UI.** New `ui/src/components/FormatBadge.tsx`. `ui/src/pages/Templates.tsx` drops its local
  `FormatBadge` and imports the shared one, passing `template.format`.
  `ui/src/pages/TemplateDetail.tsx` replaces its inline pill with the same component.
  `ui/src/theme.css` gains `--info`, `--info-soft` and `--accent-deep` in both themes.
- **Tests.** New `ui/src/components/FormatBadge.test.tsx` covering the text, the count, the conveyed
  text, the icon cell counts, the border, and the exact colour token each format binds to, which is
  the half of the colour proof a palette test cannot supply. New `ui/src/lib/contrast.ts` with its own unit test, and
  a new `ui/src/theme.test.ts` that parses `ui/src/theme.css` and asserts, from the token values
  themselves, that both badges clear 4.5:1 over every background they appear on, that the two badges
  resolve to different colours, and that the sheet colour is none of the accent, success or error
  colours. `ui/src/pages/Templates.test.tsx:115-116` asserts the badge by its bare word and its
  `sheet` assertion changes to the counted string. `ui/src/pages/TemplateDetail.test.tsx:130-137`
  covers only a `single` fixture today and gains a `sheet` one, so the shared badge is exercised at
  both call sites rather than only on the grid, comparing icon geometry and colour tokens and not just
  the text. `ui/src/pages/Catalog.test.tsx` and `ui/src/components/PreviewPane.test.tsx` each gain an
  assertion that their prose mention of a format stays prose, with no badge markup; neither makes one
  today.
- **Backend.** None. No Rust code, no API change, no schema change. `positions` is already reachable from
  `TemplateSummary` and `TemplateDetail`, both of which carry a `format: TemplateFormat`
  (`ui/src/api/types.ts:5,37-56`).
- **Dependencies.** None. The icons are hand-written SVG; no icon library is added.
- **Docs.** ADR-0066 and its row in `docs/adr/README.md`.
- **Compatibility.** Presentation only. No stored state, no URL, no API response changes; nothing to
  migrate. A user sees the same two templates, told apart differently.
- **Evidence.** UI-only and visual, so this needs screenshots of the grid, a selected card, and the
  detail page in both light and dark mode, not only a green vitest run.
