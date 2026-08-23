## 1. Palette

- [x] 1.1 Add `--info`, `--info-soft` and `--accent-deep` to both the `:root` and `.dark` blocks of `ui/src/theme.css`, with the values in `design.md`: light `#2f6f7d` / `#dfeef1` / `#b8420f`, dark `#6fb3c4` / `#17313a` / `#f0784f`. Leave `--accent` and `--accent-soft` untouched.

## 2. The contrast helper, before anything depends on it

- [x] 2.1 Write `ui/src/lib/contrast.ts`: a pure `contrastRatio(hexA, hexB)` implementing the WCAG 2.x relative-luminance formula, in the same shape as the existing pure modules beside it (`connectorSort.ts`, `connectorFilter.ts`).
- [x] 2.2 Write `ui/src/lib/contrast.test.ts` against published reference ratios: black on white 21:1, `#777777` on white 4.48:1, a pair in each order returning the same ratio, and identical colours returning 1:1. Confirm each case fails against a deliberately wrong luminance formula before it passes; a formula other tests will trust must be proven to fail when wrong.
- [x] 2.3 Write `ui/src/theme.test.ts`. Read `src/theme.css` with `readFileSync(resolve(process.cwd(), "src/theme.css"))`, as `pages/connect/connectorGridViewport.test.ts:1-16` already does. Isolate the `:root` and `.dark` blocks, then scan semicolon-separated declarations *within* each block: `theme.css:5-11` puts several tokens on one line, so a line-oriented parser silently misses most of them. Assert the parser found every token it is about to use, so a rename fails loudly instead of skipping assertions.
- [x] 2.4 Assert in `theme.test.ts`, per theme: every foreground (`--accent-deep`, `--info`) against every background (`--accent-soft`, `--info-soft`, `--surface`, `--paper`) clears 4.5:1; `--accent-deep` differs from `--info` and `--accent-soft` from `--info-soft`; and `--info` is none of `--accent`, `--good`, `--bad`. Check the numbers against `design.md`'s table.

## 3. The badge

- [x] 3.1 Write `ui/src/components/FormatBadge.tsx` taking `{ format: TemplateFormat }`. Render the markup in `design.md`: an `inline-flex` pill with `border`, `data-format={format.type}`, inline `background`, `color` and `borderColor`, the icon, and the text in its own `<span>`. `single` uses `var(--accent-deep)` / `var(--accent-soft)`, `sheet` uses `var(--info)` / `var(--info-soft)`; each badge's `borderColor` equals its `color`.
- [x] 3.2 Draw the two icons inline, `viewBox="0 0 12 12"` at 12px, `fill="currentColor"`, `aria-hidden="true"`, `focusable="false"`. `single`: one rect `x=0 y=3 w=12 h=6 rx=1`. `sheet`: six rects `w=3 h=3` at columns `x=2,7` and rows `y=0,4.5,9`.
- [x] 3.3 Render the text as `single` or `` `sheet · ${positions.length}` ``, using U+00B7 with spaces, in one text node so the whole label is one `getByText` target.

## 4. Both call sites use it

- [x] 4.1 Delete the file-local `FormatBadge` in `ui/src/pages/Templates.tsx:38-47` and render the shared component at `Templates.tsx:125`, passing `template.format`.
- [x] 4.2 Replace the inline pill at `ui/src/pages/TemplateDetail.tsx:266-271` with the same component, passing `detail.format`.
- [x] 4.3 Confirm nothing else changed: the group chip (`Templates.tsx:87-93`), the `<code>` id chip (`Templates.tsx:126-131`), the Dimensions row (`TemplateDetail.tsx:274-276`), the catalog's format prose (`Catalog.tsx:53-56`) and `PreviewPane`'s fallback link (`PreviewPane.tsx:29-32`) are all untouched.

## 5. Tests

- [x] 5.1 Write `ui/src/components/FormatBadge.test.tsx`: the `single` text, `sheet · 30`, a one-position sheet reading `sheet · 1`, and the text content being exactly the visible string.
- [x] 5.2 Add the icon assertions: `single` renders exactly one `rect`, `sheet` renders six occupying at least two distinct `x` and two distinct `y` values, and both `svg`s carry `aria-hidden="true"`.
- [x] 5.3 Add the token-binding assertions: the `single` badge's inline `color`, `background` and `borderColor` are `var(--accent-deep)`, `var(--accent-soft)`, `var(--accent-deep)`, and the `sheet` badge's are `var(--info)`, `var(--info-soft)`, `var(--info)`. This is the half of the colour proof `theme.test.ts` cannot supply: without it an implementation could bind the sheet badge to `--accent` and every palette assertion would still pass.
- [x] 5.4 Update `ui/src/pages/Templates.test.tsx:115-116`: the `sheet` assertion becomes the counted string. Add the parity assertions for a `sheet` fixture: the counted text, the `data-format` marker, every `rect`'s `x`/`y`/`width`/`height`, and the three token references.
- [x] 5.5 Add a `sheet` fixture to `ui/src/pages/TemplateDetail.test.tsx` (it covers only `single` today, at `:130-137`) and assert the same four things, so the two surfaces are proven identical rather than merely both green.
- [x] 5.6 Add the prose-exclusion assertions: `TemplateDetail.test.tsx` that the Dimensions sentence still ends in `sheet` with no marker, no `svg`, no count and no format colour tokens on its element; `Catalog.test.tsx` the same for a `sheet` entry's format word; `PreviewPane.test.tsx` the same for the `Open sheet preview` link, whose sheet case asserts only that an `<object>` exists today (`PreviewPane.test.tsx:13-16`).

## 6. Decision record

- [x] 6.1 Write `docs/adr/0066-format-badge-carries-icon-colour-and-count.md`: why one word in one colour was not enough, why the cue is threefold, why an inline SVG breaks the Unicode-glyph precedent, why the chip is delineated by a border rather than a fill, and the arrival of `--info` and `--accent-deep` in the palette. Confirm 0066 is still free against `main` before writing it.
- [x] 6.2 Add the ADR's row to `docs/adr/README.md`.

## 7. Gates

- [x] 7.1 Run `npm --prefix ui run lint`, `npm --prefix ui run test` and `npm --prefix ui run build`; all clean, with no lint suppression added.
- [x] 7.2 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test` to confirm the backend is untouched and still green.
- [x] 7.3 Load the templates grid and a template detail page in a browser, in light and in dark mode, and look. Check what vitest cannot: that the icons read as a label and a sheet at 12px, that the teal sits right against the warm paper, that the border does not read as heavy, and that a card carrying both a format badge and a group chip does not read as two of the same thing. A screen that renders without error is not a screen that is correct.
- [x] 7.4 In the same session, select a card and confirm the `single` badge is still visible against the `--accent-soft` tint, which is the failure the border exists to prevent. Check a sheet with a three-digit position count on the narrowest card width and confirm it does not squeeze the truncating id chip beside it.
- [x] 7.5 Attach the screenshots from 7.3 and 7.4 to the change.

## Browser check outcome (tasks 7.3-7.5)

Two defects the vitest suite could not see, both found by looking:

- **`sheet · 30` wrapped to two lines** and the chip became a tall lozenge. The card's bottom row was
  shrinking the badge. Fixed with `shrink-0 whitespace-nowrap` on the badge.
- **The id chip beside it collapsed to a single character.** Measured through CDP at a 1280px viewport:
  the `<code>` rendered at 12px against a natural width of 70px for `avery120`, and at 30px against 98px
  for `brother_12mm`. An A/B against the pre-change pill confirmed the chip already truncated before
  this change (`aver…`, `bro…`), so truncation was inherited, but the wider badge took it from ~55px to
  12px. `design.md`'s risk register anticipated this at the narrowest card width; it turned out to
  happen at every width.

  Resolved by moving the format badge to the card's **top rail**, beside the checkbox and opposite the
  group chip, rather than by restyling the id chip or wrapping the row. The top row held only a
  checkbox while the bottom row carried five things. After the move every id renders at its full
  natural width, untruncated, which is better than the pre-change baseline, and the badge sits at the
  same position on every card, which is what a scannable grid wants. No card grew in height.

  This places the badge somewhere `design.md` did not anticipate. The spec is unaffected: it requires
  the badge on "the card for an installed template on the template grid" and does not say where on the
  card. Recorded in ADR-0066's consequences rather than by editing `design.md`, which would void the
  review verdict.

Confirmed by eye in both themes, at 1280px and 390px: the icons read as a label and a sheet at 12px,
the teal sits right against the warm paper, the border does not read as heavy, a `single` badge stays
visible on a selected card's `--accent-soft` tint (the collision the border exists for), a 120-position
sheet does not overflow at 390px, and a card carrying both a format badge and a group chip does not
read as two of the same thing: the format badge is coloured and icon-led, the group chip neutral and
text-only.

A third defect, found by the adversarial code review and then confirmed by measurement rather than by
argument: with the badge on the top rail, a long group name pushed the badge out of its own rail and
over the group chip, because the rail carried `min-w-0` while the badge is `shrink-0` and the group
chip never truncated. Measured through CDP with a 45-character group name, the badge's right edge ran
to 729px against a rail ending at 692px and a chip starting at 700px, at 1280px and at 390px alike. The
rail is now `shrink-0` and the group chip `min-w-0`, so the chip is what gives; re-measured, all three
overflow flags are false at both widths. jsdom computes no layout, so this one is browser-verified
only and has no unit test behind it.

Screenshots: `screenshots/` beside this file. Six, not the full set: the pre-change grid of two
identical pills, the same grid after, a `single` badge on a selected card in dark mode, and the
detail page's Format row in each theme, one sheet and one single, and a long group name truncating
beside an intact badge.
