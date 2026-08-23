# 66. The format badge carries an icon, its own colour and a position count, and is delineated by a border

Date: 2026-08-23

## Status

Accepted. Issue [#201](https://github.com/pfa230/labeler/issues/201).

## Context

`single` and `sheet` are the two things a template can be, and until now the badge that said which was
styled identically for both: same pill, same `--accent-soft` fill, same `--accent` text, only the word
differing. The grid card had a file-local `FormatBadge` taking a bare type string, and the detail page
repeated the same markup inline.

Format is the attribute that decides how a template prints: one PNG, or a sheet of N positions. On a
grid of cards it was the one distinction the eye could not pick up without reading, and every badge
read as the same orange chip. The group chip beside it is also a pill, so pill-ness already carried two
unrelated meanings.

Three constraints shaped the answer. The UI has no icon system: every icon in `ui/src` was a Unicode
glyph in a text node (`★`, `ⓘ`, `▾`, `☾`, `⚠`), and there was no icon dependency to add one to. The
palette is a small semantic set, in which `--good` and `--bad` are the toast colours and mean success
and error. And colour alone fails for a colour-blind user, so whatever distinguished the two had to
work with the colour removed.

Measurement turned up a fourth. `--accent` `#e4572e` on `--accent-soft` `#fbe9e2` is **3.13:1**, under
the 4.5:1 WCAG AA asks of 12px text, so the badge this change exists to make legible was starting from
a failing contrast in light mode.

## Decision

The badge is one shared component, `ui/src/components/FormatBadge.tsx`, taking the whole
`TemplateFormat` rather than its type string, so it can read `positions`. Both the grid card and the
detail page's Format row render it.

It carries **three cues, each sufficient alone**:

- **An icon**, the first inline SVG in `ui/src`: one landscape rounded rect for `single`, six rects in
  a 2×3 portrait grid for `sheet`, at 12px in `currentColor`, `aria-hidden`. Unicode `▭`/`▦` would have
  matched the glyph precedent at no markup cost, but neither resolves in the `system-ui` stack, so both
  fall back per platform and their weight and size are not ours to control. A badge whose purpose is
  "distinguishable at a glance" cannot rest its mark on a glyph whose size the OS picks.
- **A colour.** `single` keeps the accent hue, `sheet` takes a new `--info` / `--info-soft` teal pair.
  `--good` and `--bad` were rejected: a format is neither a success nor an error. The neutral
  `--bg`/`--border` treatment was rejected because it is the group chip's styling on the same card.
- **The text**, where a sheet states its position count: `sheet · 30`. That makes the badge informative
  rather than decorative, and gives the two states different lengths.

Two token decisions follow from the measurements:

- **`--accent-deep`** (`#b8420f` light, `#f0784f` dark) carries the `single` badge's text and border,
  taking it to 4.67:1 and 5.18:1 against the unchanged `--accent-soft` fill. `--accent` itself is
  untouched: it is the primary button background, the selected-card border, the favorite star and the
  SVAR grid's hover and selection rows, and re-tinting all of them is a separate change, tracked as
  [#210](https://github.com/pfa230/labeler/issues/210).
- **A 1px border in each badge's own foreground colour delineates the chip, not its fill.** A selected
  card is tinted `--accent-soft` (`ui/src/pages/Templates.tsx:63`), exactly what the `single` chip is
  filled with, so a fill-only chip vanishes the moment a card is selected. Giving `single` a second
  orange tint stacks two near-identical colours; changing the selected card's tint edits an unrelated
  affordance; excluding the selected state from the rule narrows a rule to fit an implementation.

Both foregrounds clear 4.5:1 over every background the badge appears on, in both themes: the lowest is
4.67:1. `ui/src/lib/contrast.ts` and `ui/src/theme.test.ts` compute those ratios from the token values
parsed out of `theme.css`, so a later palette edit that breaks one fails the suite rather than drifting
from a copy.

Only badges are in scope. Three sites name a format in prose and are unchanged: the catalog listing,
whose `CatalogEntry.format` is a bare string with no positions (tracked as
[#211](https://github.com/pfa230/labeler/issues/211)); the detail page's Dimensions sentence, which
ends in the word `sheet`; and `PreviewPane`'s "Open sheet preview" fallback link.

## Consequences

The two formats separate without reading and without colour: in greyscale the icon and the text still
tell them apart, and a screen reader conveys the word, never the shape.

`ui/src` now contains SVG. That is a precedent, not a system: two hand-written icons in one component
do not justify an icon library, and the second consumer of an icon is what would.

The palette gains a third semantic colour role and a second orange. `--info` may later be reached for
as a general info colour and drift from meaning "sheet"; that is acceptable, because what fixes the
badge's colour is the `template-format-badge` capability spec, not the token being exclusive. The two
oranges are a standing hazard for a contributor reaching for the wrong one, mitigated by
`theme.test.ts` failing when the badge's pair stops passing AA.

The badge moved to the card's top rail, beside the selection checkbox and opposite the group chip,
rather than staying in the bottom row beside the id chip. That was not planned: the browser check found
that the wider badge had squeezed the `<code>` id chip from roughly 55px to 12px against a natural width
of 70-98px, rendering a single character. The id chip already truncated before this change, so the
truncation was inherited, but the badge made it useless. Restyling the id chip and wrapping the row were
both weighed; moving the badge costs no card height, leaves every id rendering untruncated for the first
time, and puts the format at the same position on every card, which is what a scanned grid wants.

The badge is now a bordered pill sharing that rail with the group chip. They do not read as two of the
same thing: the format badge is coloured and icon-led on the left, the group chip neutral and text-only
on the right. Sharing the rail did cost the group chip something: it now carries `min-w-0` and is the
element that truncates when a group name is long, because the checkbox and badge beside it are both
fixed-size. Before the move the chip had nearly the whole row and rarely truncated.

The root defect stays: `--accent` on `--accent-soft` is still 3.13:1 everywhere else it appears. This
change fixes the badge, not the pair. That is #210.
