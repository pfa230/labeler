## Context

See `proposal.md` — Why. Constraints that shape the approach:

- **The UI has no icon system.** `ui/src` contains zero `<svg>` elements. Every icon today is a Unicode
  glyph in a text node: `★`/`☆` (`Templates.tsx:171`), `ⓘ` (`Templates.tsx:157`), `▾`/`▸`
  (`PrintersSection.tsx:198`), `☾`/`☀` (`ThemeToggle.tsx:28`), `⚠` (`LabelGrid.tsx:103`). There is no
  icon dependency to add one to.
- **The palette is a small semantic set.** `ui/src/theme.css:5-11` defines `--paper --surface --ink
  --muted --faint --border --accent --accent-soft --good --bad`, each with a light and a dark value,
  several declarations to a line. `--good` and `--bad` are the toast colours (`app/toast.tsx:62`); they
  mean success and error.
- **`--accent-ink` is already spoken for.** Nineteen call sites read `var(--accent-ink, #fff)` as text
  *on* a solid accent background (e.g. `TemplateDetail.tsx:204`, `Templates.tsx:471`). The token is
  never defined in `theme.css`, so all nineteen take the `#fff` fallback. A new token for accent text
  *on a tint* therefore cannot be called `--accent-ink`.
- **The badge sits over three different backgrounds.** An unselected card is `--surface` and a selected
  one is `--accent-soft` (`Templates.tsx:73`); the detail page sits on the body's `--paper`
  (`theme.css:15`, `app/Shell.tsx:138-140`). A selected card is therefore tinted with exactly the
  colour the `single` chip is filled with.
- **The two badges are duplicated, and only one is boxed in.** `Templates.tsx:38-47` is a file-local
  `FormatBadge({ type: string })`, which cannot reach `positions` because its prop is a string.
  `TemplateDetail.tsx:266-271` is the same markup inline inside a component that already holds
  `detail.format` (`TemplateDetail.tsx:258-276`), so that site could read `positions` today and simply
  does not. Sharing one component is what stops the icon, tokens and count being written twice.
- **Three sites name a format in prose, not as a badge.** `Catalog.tsx:53-56` renders `entry.format` as
  a plain `<dd>`; `TemplateDetail.tsx:17-20` builds the Dimensions sentence, which for a sheet ends in
  the word `sheet`, rendered at `TemplateDetail.tsx:274-276`; `PreviewPane.tsx:29-32` emits "Open sheet
  preview" as the fallback link inside an `<object>`. Every other `format.type` read drives logic, not
  presentation (`Connect.tsx:133`, `Import.tsx:120`, `print/FieldForm.tsx:52`, `print/PrintForm.tsx:55`,
  `lib/preview.ts:34`).
- **`positions` is already on the wire.** `TemplateFormat` types the sheet arm with
  `positions: [number, number][]` (`ui/src/api/types.ts:5`), and both `TemplateSummary` and
  `TemplateDetail` carry a `format: TemplateFormat` (`ui/src/api/types.ts:37-56`). Nothing needs to be
  added to the API. `CatalogEntry.format` by contrast is a bare `string` (`ui/src/api/catalog.ts:18`)
  with no positions at all.
- **Existing tests match the badge by its bare word.** `Templates.test.tsx:115-116` asserts
  `getByText("single")` and `getByText("sheet")`; `TemplateDetail.test.tsx:130-137` covers a `single`
  fixture only. Only the grid's `sheet` assertion breaks. No other assertion under `ui/src` matches
  badge text: `Catalog.test.tsx:106-107` matches `sheet · avery`, which is a group *label* built at
  `Catalog.tsx:110`, not `entry.format`.
- **A test can already read `theme.css`.** `pages/connect/connectorGridViewport.test.ts:1-16` does
  exactly that with `readFileSync(resolve(process.cwd(), "src/theme.css"))`, and CI runs vitest with
  `ui/` as its working directory (`.github/workflows/ci.yml:69-94`).

## Goals / Non-Goals

**Goals:**

- One component, two call sites, three cues, as specified in `specs/template-format-badge/spec.md`.
- Cues that survive greyscale and colour-blindness, and that render identically on every platform.
- A count that comes from the data already loaded, with no extra request and no new field.
- A badge that is legible and delineated over every background it appears over today, proved by
  computation rather than by inspection, and a test that fails if a palette edit or a new background
  breaks that.
- Every normative outcome the spec states either asserted by the vitest suite from resolved colour
  values, or explicitly identified as a visual judgement checked in a browser. No requirement left to
  good intentions, and no assertion that passes on a token reference without checking what it resolves
  to.

**Non-Goals:**

- **No icon system.** This change hand-writes two icons. It does not add an icon library, an `<Icon>`
  abstraction, or a sprite sheet. The second consumer of an icon is what would justify those.
- **No other pill is restyled**, including the group chip on the same card
  (`Templates.tsx:87-93`) and the neutral `<code>` id chip beside the badge (`Templates.tsx:126-131`).
- **The selected card's own tint is not changed.** Reworking selection styling would fix the collision
  from the other side, but it edits an unrelated affordance and needs its own screenshots.
- **The app-wide `--accent` pair is not touched.** `--accent-deep` fixes the `single` badge's contrast
  where the badge is; it does not fix `--accent` on `--accent-soft` at its root. That pair also colours
  the selected-card background, the SVAR grid's hover and selection rows and the favorite star, and
  correcting it means re-tinting the UI and re-screenshotting every page. Tracked as #210.
- **The three prose mentions stay as they are**, per the spec's third requirement. See Decisions.
- **No spec of what a template *is*.** `single` versus `sheet` semantics live in frozen `docs/SPEC.md`
  §3.1 and are untouched.

## Decisions

### The icon is a hand-written inline SVG, not a Unicode glyph

`▭` (U+25AD) and `▦` (U+25A6) would match the existing glyph precedent and cost no markup, and they
were the first candidate. They are rejected on rendering: neither is present in the fonts that resolve
`ui-sans-serif, system-ui, -apple-system, sans-serif` on the platforms this app is used from, so both
land in a per-platform fallback (DejaVu Sans on most Linux, Segoe UI Symbol on Windows, Apple Symbols on
macOS). Weight, cap height, baseline and advance width then differ per OS, and a badge whose whole
purpose is "distinguishable at a glance" cannot rest its distinguishing mark on a glyph whose size we
do not control. Bundling a font to fix that is far more than two icons are worth.

CSS-drawn cells (a `grid` span with six `<i>` children) render deterministically and break no SVG
precedent, but cost six nested elements per sheet badge against an SVG's six `<rect>`s inside one
element, and put the geometry in Tailwind classes where it is harder to read than a `viewBox`.

An icon library (lucide-react, heroicons) is rejected for two icons: the smallest of them is larger
than the whole current UI bundle's icon budget, and neither ships a "sheet of labels" icon that beats
six rectangles.

So: two inline SVGs, `viewBox="0 0 12 12"`, rendered at 12px, `fill="currentColor"` so each inherits
its badge's text colour and neither needs its own token, `aria-hidden="true"` and `focusable="false"`
so neither is conveyed by assistive technology nor reachable by tab.

Geometry, chosen so the two silhouettes differ in orientation as well as in cell count:

- **single** — one landscape rounded rect, roughly a label's proportion: `x=0 y=3 w=12 h=6 rx=1`.
  Occupied bounds 12 × 6.
- **sheet** — six rects, 2 columns × 3 rows: `w=3 h=3`, columns at `x=2,7`, rows at `y=0,4.5,9`.
  Occupied bounds 8 × 12, portrait like a sheet of stock, against the single's landscape 12 × 6.
  Gutters are 2 units horizontally and 1.5 vertically.

2 × 3 rather than 3 × 3 for a stated reason: three columns inside the same 8-unit occupied width, with
the same 1.5-unit gutters, leaves cells of `(8 - 3) / 3 ≈ 1.67` units. At a 12px render one unit is one
CSS pixel, so those cells and their gutters both fall under two pixels and, at 1× device pixel ratio,
land on sub-pixel boundaries where antialiasing greys the pattern into a smear. 3-unit cells survive
that; 1.67-unit cells do not. This is a claim about a 12px render at 1×, not a general one.

### A border delineates the chip, not its fill

The `single` chip is filled `--accent-soft`, and a selected card is tinted `--accent-soft`
(`Templates.tsx:73`), so on a selected card a fill-only chip disappears into the card. Three narrower
fixes were weighed and rejected. Giving `single` its own soft tint stacks two near-identical oranges,
a weak distinction, and brings back a four-token symmetry cost. Changing the selected card's tint edits
an unrelated affordance. Excluding the selected state from the requirement narrows a rule to fit an
implementation, and would break again the next time a tinted background is introduced.

Chosen: each badge carries a 1px border in its own foreground colour, `--accent-deep` or `--info`,
alongside its fill. The border does not depend on the fill differing from what is behind it, which is
the failure being fixed, and it costs one CSS property and no new token.

Measured against the background that caused the collision, a selected card's `--accent-soft`, the
`single` border stands at 4.67:1 light and 5.18:1 dark, and the `sheet` border at 4.84:1 and 6.16:1.
That is a real edge, not a hairline hint. It is not a guarantee for an arbitrary future background: one
at or near a badge's foreground colour would erase its border exactly as `--accent-soft` erased the
fill. That is why the spec enumerates the backgrounds the badge appears over and `theme.test.ts`
computes the matrix, rather than the design claiming the border always works.

The cost, stated: the group chip on the same card is also a bordered pill (`Templates.tsx:87-93`), so
the two shapes converge slightly. They stay separable because the group chip is neutral and iconless, carrying only the group's name
(`Templates.tsx:92`), while the format badge is coloured and icon-led, and the icon is precisely the
cue #201 asked for.

### `sheet` takes a new `--info` / `--info-soft` pair; `single` keeps the accent hue

`single` is the default and the majority format, and it already owns the accent, so leaving it in that
hue is the smaller visual change and keeps the grid from turning teal.

`--good` was considered for `sheet` and rejected: it is the toast success colour, and a format is not a
success. `--bad` is ruled out by the issue for the mirror reason. Demoting `single` to the neutral
`--muted` on `--bg` with a border was considered and rejected: that is exactly the group chip's styling
on the same card, which would give a neutral bordered pill a third unrelated meaning, the very problem
this change exists to reduce.

Values, a cool teal chosen to sit against the warm paper palette rather than compete with the orange:

| token | light | dark |
| --- | --- | --- |
| `--info` | `#2f6f7d` | `#6fb3c4` |
| `--info-soft` | `#dfeef1` | `#17313a` |

### `--accent-deep` carries the `single` badge's text and border

The spec requires 4.5:1 of both badges over every background, in both themes. The shipped `--accent`
`#e4572e` on `--accent-soft` `#fbe9e2` (`theme.css:5-8`) measures **3.13:1**, so the existing `single`
treatment cannot meet it, and the change would otherwise assert a rule its own output breaks.

Four ways out were weighed. Narrowing the requirement to the sheet alone ships a known AA failure in
the change whose subject is legibility, and leaves the two badges visibly unequal in weight. Changing
`--accent` itself fixes the root cause but re-tints nineteen accent surfaces and belongs to its own
issue. Giving `single` a full dedicated pair (`--format-single` / `-soft`) is symmetric but costs four
tokens for one component and severs the visual tie between a single template and the app's accent.

Chosen: one new foreground token, `--accent-deep`, against the unchanged `--accent-soft` fill. Same
hue family, deeper value, no other surface affected.

| token | light | dark |
| --- | --- | --- |
| `--accent-deep` | `#b8420f` | `#f0784f` (the same value as `--accent`, which already passes there) |

Measured contrast of each badge's text against every background it can appear over, WCAG 2.x relative
luminance. The chip fill is what sits immediately behind the text, so the fill rows are the binding
ones; the surface rows are what the border has to work against and are recorded for the same test.

| background behind the text | `single` (`--accent-deep`) | `sheet` (`--info`) |
| --- | --- | --- |
| own chip fill, light (`--accent-soft` / `--info-soft`) | **4.67:1** | **4.78:1** |
| own chip fill, dark | **5.18:1** | **5.80:1** |
| `--surface`, light `#ffffff` | 5.49:1 | 5.69:1 |
| `--paper`, light `#faf8f3` | 5.17:1 | 5.36:1 |
| selected card `--accent-soft`, light `#fbe9e2` | 4.67:1 | 4.84:1 |
| `--surface`, dark `#1f1c16` | 6.07:1 | 7.22:1 |
| `--paper`, dark `#16140f` | 6.58:1 | 7.82:1 |
| selected card `--accent-soft`, dark `#3a241c` | 5.18:1 | 6.16:1 |

The lowest figure in the table above is **4.67:1**. `theme.test.ts` asserts a wider matrix than the
table shows, every foreground against every background in each palette, which brings in combinations
that do not occur in the UI; the lowest of those is 4.61:1, `--accent-deep` over `--info-soft`. Both
clear 4.5:1, and asserting the cross-product is cheaper than encoding which pairings are reachable.

Over their own fills the two badges land within 0.11 of each other in light mode and 0.63 in dark, so
neither reads as the weaker of the pair. Over the backgrounds they share the gap widens, to 0.20 light
and 1.24 dark, always in the sheet's favour.

The teal pair is named `--info` rather than `--format-sheet` because the palette's names are semantic
roles (`--good`, `--bad`, `--accent`), not usages, and a third role is the natural slot for a neutral
informational colour. The risk is a later change reaching for `--info` as a generic info colour and
inheriting "means sheet" by accident; the mitigation is that the capability spec, not the token name,
is what fixes the badge's colour.

### The component takes the format, not the type

`FormatBadge({ format }: { format: TemplateFormat })`, in `ui/src/components/FormatBadge.tsx` beside
`LabelGrid`, `ParamInput`, `PreviewPane` and `EmptyTemplates`. Taking the discriminated union rather
than `type: string` is what lets it read `positions.length` on the sheet arm, and it lets TypeScript
prove the count is only reachable where positions exist. The two call sites pass `template.format` and
`detail.format` respectively.

### Only badges are in scope; prose that names a format is not

Three production sites name a format without being badges, and all three stay as they are:

- `Catalog.tsx:53-56` lists a template that is *not installed yet*, from a JSON index fetched by the
  browser from GitHub (`api/catalog.ts:9`). `CatalogEntry.format` is a bare `string` with no positions
  (`api/catalog.ts:18`), so a badge there could show an icon and a colour but never a count, and making
  it show one means changing the catalog's published format. That is a different change against a
  different data source, tracked as #211 rather than smuggled in here.
- `TemplateDetail.tsx:17-20` builds the Dimensions sentence, whose sheet form ends in the word `sheet`
  ("25 × 25 mm on 210 × 297 mm sheet"), rendered at `TemplateDetail.tsx:274-276`. It is a sentence about
  size that happens to end in the format's name, one row below the badge, and badging it would say the
  same thing twice.
- `PreviewPane.tsx:29-32` renders "Open sheet preview" as the `<a>` fallback inside an `<object>`, shown
  only when the browser cannot display the PDF. It is a link label, not a status marker, and its
  `format: "single" | "sheet"` prop (`PreviewPane.tsx:13`) selects which branch renders rather than what
  a badge says.

The spec's third requirement names all three exclusions so this is a decision on the record and not a
gap.

### Markup shape, and the one test hook

```
<span class="inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs font-medium"
      data-format={format.type}
      style={{ background: soft, color: fg, borderColor: fg }}>
  <svg aria-hidden="true" focusable="false" width="12" height="12" viewBox="0 0 12 12" ...>
  <span>single | sheet · 30</span>
</span>
```

`·` (U+00B7) surrounded by spaces is the separator this UI already uses for a secondary fact after a
primary one: `` `${s.label} · ${s.breadcrumb}` `` (`ConnectorBrowser.tsx:708`), `.join(" · ")`
(`PrintersSection.tsx:179`), `` `${category} · ${vendor}` `` (`Catalog.tsx:110`). `sheet (30)` and
`30-up` were considered; parentheses read as an aside rather than a fact, and `30-up` is print-shop
jargon this UI does not otherwise speak. The word and the count live in one text node, so the whole
label is one `getByText` target and what a screen reader conveys is exactly the string a sighted user
reads.

`data-format` is the only concession to testability in app markup: it states what the badge is for and
gives both the suite and a browser check a stable selector, independent of the text. It does not by
itself prove the icons or the colours differ; the assertions below do that. The codebase uses
`data-testid` twice, both in test-local stubs and never in app code, so this does not extend that
pattern.

### Every spec requirement maps to an assertion, or is named as a visual judgement

Colour is proved in two halves, because neither half closes the chain alone. `ui/vite.config.ts:11`
runs vitest under jsdom, where an inline `style` holding `var(--accent-deep)` stays that literal
string: a component test can prove *which token* a format is bound to, never what it resolves to.
`theme.test.ts` can resolve tokens, but knows nothing about which badge uses which. Asserting only the
second half would let an implementation bind the sheet badge to `--accent`, or swap the two variants
outright, with every theme assertion still green. So the component test pins the exact token each
format uses for foreground, fill and border, and the theme test independently proves those values
differ, clear 4.5:1 over every background, and are none of the status colours.

| Spec outcome | How it is checked |
| --- | --- |
| single icon depicts exactly one cell; sheet four or more in ≥2 rows and ≥2 columns | `FormatBadge.test.tsx` counts `svg rect` in each badge and asserts the sheet's rects occupy ≥2 distinct `x` and ≥2 distinct `y` values |
| each format is bound to its own tokens | `FormatBadge.test.tsx` asserts the `single` badge's inline `color`, `background` and `borderColor` are `var(--accent-deep)`, `var(--accent-soft)` and `var(--accent-deep)`, and the `sheet` badge's are `var(--info)`, `var(--info-soft)` and `var(--info)` |
| the badges resolve to different text colours and different fill colours | `theme.test.ts` compares the resolved values of `--accent-deep` against `--info`, and `--accent-soft` against `--info-soft`, per theme |
| distinguishable with colour removed | `FormatBadge.test.tsx`: cell count and text, both colour-independent |
| the icon is hidden and the conveyed text is the visible text | `FormatBadge.test.tsx` asserts the `svg` carries `aria-hidden="true"` and that the badge's text content is exactly the visible string |
| `sheet · N`, `single` alone, N = declared positions, N = 1 still a sheet | `FormatBadge.test.tsx` over three fixtures |
| grid and detail render the same icon, colours and text | `Templates.test.tsx` and `TemplateDetail.test.tsx` render the same `sheet` fixture and each assert the counted string, the `data-format` marker, the icon's full list of `rect` `x`/`y`/`width`/`height` attributes, and the same three token references the component test pins. Text, marker and rect *count* alone would pass for two six-cell icons of different geometry, or for a detail pill wearing the wrong colours. `TemplateDetail.test.tsx:130-137` has no sheet fixture today, so it gains one; without it the detail page could keep an iconless, countless pill and both suites would still pass |
| catalog prose untouched | `Catalog.test.tsx` gains an assertion that a `sheet` entry's format renders as the plain word `sheet`, in an element carrying no `data-format` marker, no `svg`, no position count, and neither `var(--info)` nor `var(--info-soft)` in its inline style. It makes no such assertion today: its only `sheet` match is the group label `sheet · avery` (`Catalog.test.tsx:106-107`) |
| Dimensions row untouched | `TemplateDetail.test.tsx` asserts the Dimensions sentence still ends in `sheet`, in an element carrying no `data-format` marker, no `svg`, no position count, and neither format's colour tokens in its inline style |
| preview fallback untouched | `PreviewPane.test.tsx` gains an assertion that the sheet branch still renders the `Open sheet preview` link text, in an element carrying no `data-format` marker, no `svg`, no position count, and neither format's colour tokens in its inline style. Its sheet case today asserts only that an `<object>` exists (`PreviewPane.test.tsx:13-16`) |
| both badges ≥ 4.5:1 over every background, both themes | `theme.test.ts` parses `theme.css`, extracts `--accent-deep --accent-soft --info --info-soft --surface --paper` per theme, and computes the matrix above with `ui/src/lib/contrast.ts` |
| sheet's colour is none of the accent, success or error colours | `theme.test.ts` compares `--info` against `--accent`, `--good` and `--bad`, per theme |
| the chip is delineated over a background of its own fill colour | `FormatBadge.test.tsx` asserts each badge's border colour equals its foreground token, and `theme.test.ts` computes that border colour's contrast against each background the badge appears over |
| the icons *read* as a label and a sheet at 12px; the teal sits right against warm paper; the border does not read as heavy | **visual judgement**, browser check, not assertable |

`ui/src/lib/contrast.ts` is a pure `contrastRatio(hex, hex)` implementing WCAG 2.x relative luminance,
sitting beside `connectorSort.ts` and `connectorFilter.ts` in the same `lib/` pattern. It gets its own
`contrast.test.ts` against published reference ratios (black on white 21:1, `#777777` on white
4.48:1), because a formula that other tests trust must itself be proven to fail when wrong.

`theme.test.ts` follows `connectorGridViewport.test.ts:1-16`, which already reads `src/theme.css` with
`readFileSync(resolve(process.cwd(), "src/theme.css"))` under vitest. Its parser must isolate the
`:root` and `.dark` blocks and then scan semicolon-separated declarations *within* each block:
`theme.css:5-11` puts several tokens on one line, so a line-oriented parser would silently miss most of
them. Parsing the CSS rather than duplicating the hex values in TypeScript is deliberate: a later
palette edit that breaks AA then fails the suite instead of drifting from a copy.

### Evidence is a browser as well as vitest

Per `CLAUDE.md`'s rule on visual artifacts, the last row of that table is the point: vitest proves the
structure, the text, the names and the numbers, and cannot tell whether the thing looks right. The grid
including a selected card, and a detail page, get loaded in both themes and looked at, with screenshots
attached to the change.

### ADR

`docs/adr/0066-format-badge-carries-icon-colour-and-count.md`, new, superseding nothing. 0065 is the
highest on `main` as of writing; confirm 0066 is still free against `main` before writing it. It records
why one word in one colour was not enough, why the cue is threefold, why an SVG breaks the glyph
precedent, why the chip is delineated by a border rather than a fill, and the arrival of `--info` and
`--accent-deep` in the palette.

## Risks / Trade-offs

- **A 12px icon may not read as what it depicts.** A 6-rect grid at 12px is a texture more than a
  picture. → The icon never carries meaning alone: the word is always present, and the count and colour
  differ too. The browser check decides whether the geometry needs another pass; the spec constrains
  cell counts and arrangement, not the exact rects, so a revision does not void it.
- **The border converges the badge with the group chip**, which is also a bordered pill on the same
  card. → Colour, icon and text separate them; confirm at the browser check that a card carrying both
  does not read as two of the same thing.
- **The first SVG in `ui/src` invites an icon library later.** → Non-Goals says the second consumer is
  what would justify one. Two hand-written icons in one component is not a system.
- **`--info` may be reached for as a general info colour** and drift from meaning "sheet". → Acceptable:
  a general info colour is a legitimate role for it, and the badge's colour is fixed by the capability
  spec rather than by the token being exclusive.
- **`--accent-deep` makes two oranges live in the palette**, and a later contributor may reach for the
  wrong one. → The names say which is which, and `theme.test.ts` fails if the badge's pair stops passing
  AA over any of its backgrounds, which is the failure that would actually matter.
- **The count makes the sheet badge wider**, and it sits in a card row alongside a truncating `<code>`
  id chip (`Templates.tsx:126-131`). A three-digit count on a narrow card could squeeze that chip. → The
  chip already truncates and the row holding both is `min-w-0` (`Templates.tsx:124`); confirm at the narrowest card width during the
  browser check, with a wide sheet in the fixture.
- **The root `--accent` on `--accent-soft` failure survives this change** at 3.13:1 everywhere else it
  appears. → Named in Non-Goals with the measurement, fixed for this badge, and to be filed as its own
  issue #210 rather than silently carried.
