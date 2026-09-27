## ADDED Requirements

### Requirement: Vertical alignment places a fixed metric box, inset only by the ink the block carries past its aligned edge

`alignment.vertical` SHALL position a text block's **metric box** inside the item's resolved box. The metric box runs from the first line's cap-height top to the last line's baseline, with baselines `pitch(s)` apart, exactly as the *Vertical fitting reserves the ink each alignment can expose* requirement defines `metric_block(n, s)`. A blank line has a line box like any other, so a blank first line's cap-height top is still the block's metric top. The box is Typst's default line box, cap height to baseline: the one CSS `text-box-trim` and Figma's vertical trim use, and the convention Pango, Pillow, TeX (`\strut`) and Flutter (`StrutStyle`) follow.

With `a` and `d` the emitted block's outer ink above and below, as the fitting requirement defines them:

- **`center`** SHALL centre the metric box in the item's box and inset nothing. A centred baseline is therefore a function of the box, the font size, the pitch and the line count, and never of which glyphs the lines carry: two values rendered at the same size and line count in the same box share every baseline.
- **`top`** SHALL place the metric top `a` below the box's top edge. A block with no ink above its metric top sits flush, its metric top on the box's top edge.
- **`bottom`** SHALL place the last baseline `d` above the box's bottom edge. A block with no ink below its last baseline sits flush, its last baseline on the box's bottom edge.

The inset SHALL be computed from the same emitted lines, at the same size and on the same font instance, as that item's fit and intrinsic height, so the block placed is the block the fit judged and no placement insets by more than the fit reserved. The edge a block is not aligned to needs no inset of its own: the fit reserved `a + d` for `top` and `bottom`, so the ink at the far edge lands inside the box as well.

The inset SHALL never be negative. A block whose ink falls short of its aligned metric edge is not moved toward that edge: `ace`, whose ink tops out near x-height, sits top-aligned exactly where `HELIX` does, its ink below the top edge by the gap between cap height and the top of that ink. What is aligned is the metric box, not the ink box. Centring each string's own ink box, which #127 shipped and #133 reverted, SHALL NOT be restored at any alignment.

What this costs is stated rather than hidden. A `top`- or `bottom`-aligned baseline moves between two values when one of them inks past the aligned metric edge, and by exactly that ink: `HELIX` and `Émile` top-aligned at one size differ by the height of `É`'s accent above cap height. In the bundled Inter that covers accented capitals, the dots of `i` and `j`, and the overshoot of round capitals and figures such as `O` and `0`, which rise about 0.01em above cap height. A value whose ink stays inside the metric box at the aligned edge never moves.

This requirement supersedes, in the frozen `docs/SPEC.md` §3.1, the paragraph "**What `alignment.vertical` aligns.**" in full and the "**Lowercase-only text sits lower in its slot than all-caps.**" bullet in full. The other bullets under that paragraph were already superseded: the inset and limits bullets by the fitting requirement, and the blank-edge bullet by *Text is laid out against the box it will get, and what does not fit is authored*. The "**`alignment.vertical` on auto-length items.**" paragraph, which makes `top` the default, is unaffected and remains authoritative.

#### Scenario: Unaccented capitals sit flush with the top edge

- **WHEN** a `top`-aligned `text` with a fixed `font_size` of 20 renders `HELIX` in a box taller than its cap height
- **THEN** its metric top sits on the box's top edge, with no inset, where before this requirement it was inset by 0.2412em (4.82 pt)
- **AND** in the rendered PNG the item's first inked raster row is within one row of the box's top edge

#### Scenario: An accented capital is inset by its own accent and stays whole

- **WHEN** a `top`-aligned `text` with a fixed `font_size` of 20 renders `Émile` in a box exactly as tall as its intrinsic height
- **THEN** its metric top is inset by the height `É`'s accent rises above cap height in the font instance rendered, and not by 0.2412em
- **AND** its ink, the accent included, is contained, as the *Vertical fitting reserves the ink each alignment can expose* requirement defines containment

#### Scenario: A descender is inset by its own depth and stays whole

- **WHEN** a `bottom`-aligned `text` with a fixed `font_size` of 20 renders `gyp` in a box exactly as tall as its intrinsic height
- **THEN** its baseline is inset by the depth of its lowest descender in the font instance rendered, and not by 0.2412em
- **AND** its ink, the descenders included, is contained, as the *Vertical fitting reserves the ink each alignment can expose* requirement defines containment

#### Scenario: A centred baseline does not follow the glyphs

- **WHEN** one `center`-aligned `text` with a fixed `font_size` renders `HELIX`, `Émile`, `testj` and `gyp` in turn, in a box tall enough for each at that size
- **THEN** no inset is emitted for any of them, and all four renders put the baseline on the same raster row
- **AND** their ink sits at different distances from the box's top and bottom edges, because the metric box is centred and not the ink box

#### Scenario: A block is never pulled toward its aligned edge

- **WHEN** a `top`-aligned `text` with a fixed `font_size` renders `ace` and then `HELIX` in the same box
- **THEN** both put the baseline on the same raster row
- **AND** `ace`'s first inked row lies below the box's top edge, by the gap between cap height and the top of its own ink, rather than on it

#### Scenario: Ink between the lines moves nothing

- **WHEN** a `top`-aligned two-line `text` at the default pitch renders `Hg\nÉH`, and then `ÉH\nHg`
- **THEN** `Hg\nÉH` is emitted with no inset, because its accent lies below the block's metric top and its descender above the block's last baseline
- **AND** `ÉH\nHg` is inset at the top by `É`'s accent height
- **AND** rendered `bottom`-aligned, `ÉH\nHg` is inset at the bottom by the depth of `g` and `Hg\nÉH` is not inset

## MODIFIED Requirements

### Requirement: Text is laid out against the box it will get, and what does not fit is authored

Every **active** `text` item SHALL be laid out by the steps below and SHALL have its `overflow`
policy enforced, whether or not either of its axes asks for an intrinsic size. Laying out and
demanding an intrinsic size are different things: a text in a fully authored box (`size: [40, 10]`,
fixed `font_size`) reports no intrinsic size on either axis and is still broken, fitted and checked
against its policy. Its layout is the rendered output, not a measurement.

The box a text is laid out against is known before its content is: it is the item's own extent when
that extent is authored, and the available extent, capped, when it is content or frame. On a
dynamic-width `single` the frame used for that is `format.width.max`. Nothing about line breaking
consults the page format.

1. **Break.** A value's lines are its segments at `\n`, so a value with N newlines has N+1 lines, and
   **every one of them is laid out** whatever the item's `wrap` flag says. `\r\n` is normalised to
   `\n` before this step, because `\r` is unmapped in the bundled font and would otherwise be charged
   the `.notdef` advance while rendering nothing; a lone `\r` is not a terminator (see #259).
   `wrap: true` then wraps each line to the box width, breaking at spaces and never inside a word: a
   word wider than the box stays whole on its own line, wider than the box. `wrap: false` breaks
   nothing further. A line therefore remains wider than the box when one **word** is and `wrap: true`
   was authored, which step 2 shrinks and step 3 resolves; when one **glyph** is, which no breaking
   rule can help; or when `wrap: false` was authored, which step 3 resolves.

2. **Shrink.** A `font_size` range picks the largest size in `[min, max]` at which the broken block
   fits the box height **and every line step 1 broke it into fits the box width**, in 0.5 pt steps, including the
   reservation the *Vertical fitting reserves the ink each alignment can expose* requirement
   computes, for the item's `alignment.vertical`, from the outer ink of the lines broken at that
   candidate. The text SHALL be re-broken at each candidate's glyph advances, as today's
   `largest_fitting_font` does; the emitted breaks are the ones from the selected size, not breaks
   frozen at `font_size.max`. A word too wide for the box at one candidate is therefore a reason to
   try a smaller size, not a break to place inside the word. A fixed `font_size` skips this step.
3. **Overflow.** What still does not fit is resolved by the item's `overflow` policy.
4. **Emit.** Every line produced by step 1, blank or not, gets its own line box. A blank first or last
   line is a line the caller wrote and is laid out like any other.

Blank edge lines no longer make the measured and emitted blocks differ: nothing is trimmed, so the
block step 2 chooses the size against is the block step 1 produced. Step 3 may still shorten it — the
`ellipsis` policy drops the lines that do not fit — so the contract names which block counts: the
item's **intrinsic height** SHALL be the block height of the lines emitted **after** the overflow
policy has been applied, at the size step 2 chose. What changed is that the difference is now caused
only by overflow, which is visible on the label as a marker, and never by a line silently removed for
carrying no glyphs.

This supersedes ADR-0045's blank-edge rule and the previous normative ordering of steps 2 and 4, under
which a blank edge line was counted while choosing the size and dropped when emitting. That rule
predates hard line breaks surviving at all: it was written when a non-wrapping item emitted one line,
and it is the same silent discard this capability now refuses everywhere else. A value with a blank
edge therefore occupies one more line box than it did, and may select a smaller font or gain an
overflow marker as a result.

Neither the breaks nor the size SHALL be re-decided when the item's box turns out to be larger than
the box it was laid out against. A `fill` text on a label that clamps up to `width.min` keeps the
lines and size it was laid out with, and the extra width becomes slack for `alignment.horizontal`.

An item's box SHALL be its box regardless of `alignment.horizontal`, which positions content inside
it. This supersedes `openspec/changes/archive/2026-08-21-issue-180-auto-length-text-alignment`, under
which a centred auto-width text on a dynamic-width label was given the alignment slot as its box
while a left-aligned one was given the laid-out width.

**`overflow`.** A `text` SHALL carry an `overflow` field with the values `ellipsis` (the default) and
`fail`. Both shorten nothing the other would not; they differ in when they give up:

| | `ellipsis` | `fail` |
| --- | --- | --- |
| fits as authored | render it | render it |
| fits once shortened | render the shortened form | `text_does_not_fit` |
| cannot fit however short | `text_does_not_fit` | `text_does_not_fit` |

Shortening has two independent paths. Lines are dropped from the end of the block, the block kept
being the longest leading run of lines that fits the box height in the form it will be emitted, and
the marker is appended to the last retained line; a line that is wider than the box is shortened where it sits,
whether or not anything was dropped. Either path trims characters until the line and the marker fit.
The marker reports the **field**, not the line it sits on: it is appended whenever any line was
dropped, whether that line carried glyphs or was blank, and for a dropped line it lands at the end
of the last retained line whatever that line holds. A value whose every line is shown, unshortened, carries no marker.
The shortest form it can produce across the box is the marker alone. Down the box there is no single
shortest form, because a run's height depends on the ink at its edges and dropping a line changes
which ink that is: a longer run can fit where a shorter one does not, as when the first line's deep
descender lies inside a two-line block and below a one-line block. Shortening therefore succeeds
whenever `...` fits the box width and at least one leading run of lines, each judged in the form it
would be emitted with its own reservation, fits the box height, and fails otherwise. Every run SHALL
be judged before the item is refused; no run, the first line alone included, is refused on its own
account while a longer one fits. Height is shortened by whole lines only: characters are trimmed to
meet the box width, never to shed an accent or a descender. Two cases therefore reach the third row,
and neither is a separate rule:

- the box is narrower than `...` itself, so there is nothing shorter to produce;
- no leading run of lines, from the whole block down to its first line, fits the box height at the
  chosen size in its emitted form, reservation included.

An over-wide **line** is shortened in place, wherever it sits in the block: a line wider than the
box at the chosen size is trimmed until it and the marker fit, independently of the dropped-lines
path, so the marker may sit on a middle line while a later fitting line is emitted untouched. An
over-wide **word** reaches the policy exactly like an over-wide glyph: under `wrap: true` it is
kept whole, so at the chosen size it is a line wider than the box, and step 3 shortens or refuses
it. A box can be too narrow for a word or a glyph and still wide enough for the marker, and under
`ellipsis` that case renders the shortened form with `...`; it fails only when the marker does not
fit either. Under `fail` it fails as soon as the content overflows, marker or no marker.

Clipping SHALL NOT be an outcome of the policy: a box that cannot hold the shortest representable
form of its content is an error, not a label with half a glyph on it.

The policy SHALL be evaluated against the **measured block** the *Vertical fitting reserves the ink
each alignment can expose* requirement defines: the cap-height-to-baseline metric block plus the
reservation computed from the outer ink of the lines being judged, for the item's
`alignment.vertical`, including `center`. Size selection, the line budget and the `fail` verdict SHALL each judge
that one quantity on the lines it is deciding about, so none of them can accept a block another
refuses. A `center`-aligned item whose metric block fits its box but whose
block plus reservation does not SHALL be shortened under `ellipsis` and SHALL raise
`text_does_not_fit` under `fail`, and a box that no leading run of lines fits, each run in its emitted form
with its own reservation, SHALL raise `text_does_not_fit` under either. Because the reservation
follows the emitted ink, the verdict for one box can differ between two values of the same width and
line count: a box one cap height tall holds `HELIX` and refuses `Égypt`.

A glyph the rendering font supplies is measured wherever it inks, so the ADR-0050 consequence that a
glyph outside the font's ascender/descender band may clip is superseded: `Å` and `Ǻ`, which ink above
Inter's ascender, are reserved for like any other accent. One gap remains. A character the rendering
font does not map is drawn by Typst from a fallback font the measurement does not read, so its
position is measured from the rendering font's own missing-glyph outline rather than from what is
drawn, and its ink may clip. Handling it is #254's scope, not this requirement's.

This requirement supersedes the frozen `docs/SPEC.md` §3.1 bullet "Blank first/last lines are dropped
before rendering, so a leading or trailing newline does not push the visible text off centre; interior
blank lines are kept as spacing.", which the blank-line rule above replaces in full. It also supersedes
the frozen `docs/SPEC.md` §3.1 sentence "If the content still overflows
at `font_size.min`, the fitting lines are kept and the last is ellipsized" and its multiline wrap
paragraph, and the §4.1 clause "A range auto-shrinks the text to fit the box (0.5pt steps) and
truncates with an ellipsis if it still overflows", generalising both to every format and every
`font_size` spelling.

#### Scenario: A fully authored text is still laid out and still enforces its policy

- **WHEN** a `text` declares `size: [40, 10]` with a fixed `font_size` and `overflow: fail`, so
  neither axis asks for an intrinsic size, and its value does not fit
- **THEN** the render fails with reason `text_does_not_fit`
- **AND** with `overflow: ellipsis` it is broken and ellipsized to the 40 by 10 box, rather than
  emitted unfitted for the renderer to clip, which is what happens today

#### Scenario: A long word is split, not overflowed

- **WHEN** a `wrap: true` `text` carries a single word far wider than its box, and the box is
  tall enough for the resulting lines
- **THEN** the word is not split: it stays whole on one line, step 2 spends the `font_size` range
  on it, and whatever still does not fit at the floor is resolved by the item's `overflow` policy
- **AND** this scenario keeps its name from the superseded version, where the word was split
  character by character and neither policy was consulted

#### Scenario: An over-wide word shrinks whole instead of breaking

- **WHEN** a `wrap: true` `text` with `font_size: { min, max }` carries a value containing a word
  too wide for its box at `max` but fitting whole at some size in the range, in a box tall enough
  for the resulting lines
- **THEN** it renders that word whole, on one line, at the largest such size
- **AND** an implementation retaining the character-chunking loop accepts the first size whose
  height works and emits the word split across lines, and fails this scenario

#### Scenario: An over-wide word at the floor is shortened when the marker still fits

- **WHEN** a `wrap: true` `text` with `font_size: { min, max }` and `overflow: ellipsis` carries a
  word still wider than its box at `min`, in a box still wider than `...` at that size
- **THEN** it renders the shortened form with the `...` marker, because a shortened form exists
- **AND** the same item with `overflow: fail` fails with reason `text_does_not_fit`

#### Scenario: An over-wide word at a fixed size takes the overflow outcome

- **WHEN** a `wrap: true` `text` with a fixed `font_size` and `overflow: ellipsis` carries a word
  wider than its box, in a box still wider than `...` at that size
- **THEN** it renders the shortened form with the `...` marker rather than splitting the word
- **AND** the same item with `overflow: fail` fails with reason `text_does_not_fit`
- **AND** the same item with `overflow: ellipsis` in a box narrower than `...` fails with reason
  `text_does_not_fit`, because no shortened form exists

#### Scenario: No emitted line is ever a mid-word fragment without a marker

- **WHEN** any `wrap: true` `text` renders any value under either `overflow` policy
- **THEN** every emitted line carrying glyphs is either a whole word, words joined by single
  spaces, or a line carrying the `...` marker; a blank line carries no glyphs and no marker
- **AND** a mid-word fragment with no hyphen and no marker never appears

#### Scenario: An over-wide line is shortened where it sits, not only at the end

- **WHEN** a `wrap: true` `text` with `overflow: ellipsis` and a fixed `font_size` carries a
  value whose first line is wider than its box while its last line fits as authored, with no
  line dropped off the end of the block
- **THEN** the over-wide line is trimmed until it and the marker fit, and the fitting last line
  is emitted untouched
- **AND** the marker therefore sits mid-block rather than at the end of the last retained line

#### Scenario: An over-wide glyph is shortened when the marker still fits

- **WHEN** a `wrap: true` `text` with `overflow: ellipsis` and a fixed `font_size` carries a
  glyph wider than its box, in a box still wider than `...` at that size
- **THEN** it renders as `...`, because a shortened form exists
- **AND** the same item with `overflow: fail` fails with reason `text_does_not_fit`

#### Scenario: A box narrower than the marker cannot be shortened

- **WHEN** the same `ellipsis` item sits in a box narrower than `...` at its chosen size
- **THEN** the render fails with reason `text_does_not_fit`, because no shortened form exists
- **AND** no clipped marker is emitted

#### Scenario: A box too short for one line cannot be shortened

- **WHEN** a `text` with a fixed `font_size` of 20 sits in a box 40 wide and 2 units tall
- **THEN** the render fails with reason `text_does_not_fit` under either policy

#### Scenario: A hugging text never shortens on its own account

- **WHEN** a `content`-width `text` with no `max_w` renders in a frame wide enough for its available
  extent
- **THEN** it is laid out at its natural width with no truncation, because its box is its content
- **AND** shortening applies only when a cap or the available extent binds

#### Scenario: An empty value in a zero-width box is not an overflow

- **WHEN** a `content`-width `text` bound to an empty value resolves to a zero-wide box
- **THEN** it renders empty and no error is raised, because there is no content to shorten

#### Scenario: Shrinking happens before the policy

- **WHEN** a `text` with `font_size: { min: 8, max: 20 }` and `overflow: fail` carries a value that
  does not fit at 20 but fits at 12
- **THEN** it renders at 12 and no error is raised

#### Scenario: A leading blank line shrinks the chosen font

- **WHEN** a `wrap: true` `text` with `font_size: { min: 8, max: 20 }` receives a value of one blank
  line followed by **two** non-blank lines that need no wrapping, in a box tall enough for exactly two
  lines at 20 pt and for three at 14 pt
- **THEN** the block is three line boxes while the size is chosen, so 20 pt does not fit and 14 pt is
  selected
- **AND** all three are emitted at that size, the blank one included, so the visible text sits one line
  lower than the same value without the leading newline
- **AND** no overflow marker is added, because no line was dropped

#### Scenario: A hugging parent hugs the emitted lines, not the trimmed ones

- **WHEN** a `container` with `size: [20, content]` holds a `wrap: true` `text` with a fixed
  `font_size` whose value is a blank line followed by two non-blank lines
- **THEN** the container's intrinsic height is the block height of **three** lines
- **AND** it is the same count the font size was chosen against, because no trimmed lines remain for it
  to differ from: this scenario keeps its name from the superseded version, where those counts were two
  and three

#### Scenario: A hard line break survives when wrapping is off

- **WHEN** a `wrap: false` `text` receives a value of two non-blank lines, in a box tall enough for
  both and wide enough for each, so neither shrinking nor the overflow policy has anything to resolve
- **THEN** both lines are emitted, in order, and neither is broken at the box width
- **AND** an implementation keeping only the first input line would emit one

#### Scenario: An empty value is one empty line

- **WHEN** a `text` receives an empty value in a box that holds at least one line
- **THEN** it is laid out as one line carrying no glyphs, and its intrinsic height is one line box:
  one cap height, with no reservation, because a line carrying no glyphs carries no ink
- **AND** its intrinsic width is zero, so a hugging parent reserves height without reserving width

#### Scenario: A whitespace-only line keeps its line box

- **WHEN** a `wrap: true` `text` receives a value whose second of three lines contains only spaces
- **THEN** three line boxes are laid out, the middle carrying no glyphs, and wrapping does not collapse
  it away

#### Scenario: A dropped trailing blank earns the marker

- **WHEN** a `wrap: false` `text` with `overflow: ellipsis` receives `"message\n"` in a box tall enough
  for one line and wide enough for `message` but not for `message...`
- **THEN** the blank line does not fit and is dropped
- **AND** the emitted form carries the marker and fits the box width, so at least one character of
  `message` is removed to make room, rather than the label claiming the value was shown in full

#### Scenario: A dropped blank leaves the marker on a line with no glyphs

- **WHEN** a `wrap: false` `text` with `overflow: ellipsis` receives `"\nmessage"` in a box that holds
  one line and is at least as wide as `...`
- **THEN** the retained line is the blank one and `message` is dropped
- **AND** that line carries the marker, so the label reads `...`
- **AND** in a box narrower than `...` the same value is `text_does_not_fit` instead, under the
  unchanged rule that a box which cannot hold the shortest representable form is an error

#### Scenario: CRLF costs no width

- **WHEN** a `text` receives `"abc\r\nabc"`
- **THEN** it is laid out identically to `"abc\nabc"`, in line count, chosen size and intrinsic width
- **AND** no `\r` reaches measurement, where it would be charged the `.notdef` advance

#### Scenario: Alignment does not change the box

- **WHEN** the same `content`-width `text` renders once with `alignment.horizontal: left` and once
  with `center`
- **THEN** both are drawn into a box of the laid-out text width, and neither gets the frame remainder

#### Scenario: Centring is authored

- **WHEN** a `text` declares `size: [fill, 16.1]` with `alignment.horizontal: center` on a
  dynamic-width label clamped to `width.min`
- **THEN** its box spans the frame remaining from its anchor, and the text is centred within it

#### Scenario: The policy is independent of the format

- **WHEN** the same item and value are placed on a fixed-width `single`, a `sheet` slot, and an
  auto-length `single` clamped to `width.max`, each with the same resolved box
- **THEN** all three produce the same lines and the same overflow outcome

#### Scenario: A centred item that overflows only by its ink is an overflow

- **WHEN** a `center`-aligned `text` with `overflow: fail` and a fixed `font_size` carries a value
  whose metric block fits its box height but whose block plus reservation does not
- **THEN** the render fails with reason `text_does_not_fit`
- **AND** the same item with `overflow: ellipsis` drops or shortens lines until the block plus its
  reservation fits, or fails if no such form exists

#### Scenario: A longer run fits where the first line alone does not

- **WHEN** a `center`-aligned `wrap: false` `text` with a fixed `font_size` of 20, `line_spacing: 0.5`, `overflow: ellipsis` and a box 28 pt tall and wide enough for every line with the marker renders `g\u0323\nHELIX\nHELIX` (a `g` carrying a combining dot below)
- **THEN** it keeps two lines, the second ending in `...`: the three-line block needs about 34.55 pt, the two-line emitted form about 24.83 pt, because the first line's dot lies inside it, and the one-line form about 31.45 pt, because centring it must reserve twice the dot's depth
- **AND** the render is not refused, although the first line alone does not fit
- **AND** its ink is contained, as the *Vertical fitting reserves the ink each alignment can expose* requirement defines containment

#### Scenario: A box's verdict follows the ink the value carries

- **WHEN** a `top`-aligned `text` with a fixed `font_size` of 20, `overflow: fail` and a box as wide
  as either value needs and exactly `cap_height(20 pt)` tall renders `HELIX`, and then `Égypt`
- **THEN** `HELIX` renders, because none of its glyphs inks above cap height or below the baseline
- **AND** `Égypt` fails with reason `text_does_not_fit`, and fails the same way under
  `overflow: ellipsis`, because its one line is the only run there is, and it carries an accent and a
  descender the box cannot hold, and shortening never trims characters to shed them

### Requirement: Vertical fitting reserves the ink each alignment can expose

A text line's metric box runs cap height to baseline, so accented capitals ink above it and descenders ink below it, and every layout item is drawn into a clipped box of its resolved size. What a block needs beyond its metric box is the ink its own lines carry past its outer edges, and the renderer SHALL measure that rather than assume it. For a block of `n` lines at a candidate font size `s`, drawn with the font instance the renderer will render them with, the renderer SHALL define:

- `pitch(s)` = `line_spacing × s`, where `line_spacing` is the item's authored value or 1.2 when absent (`text-line-spacing`)
- `leading(s)` = `pitch(s) − cap_height(s)`, the paragraph leading the renderer emits for the item; between lines only
- `metric_block(n, s)` = `cap_height(s) + (n − 1) × pitch(s)`, leading between lines only
- `baseline_i(s)` = `cap_height(s) + (i − 1) × pitch(s)`, the depth of line `i`'s baseline below the block's metric top, for `i` from 1 to `n`
- `rise_i` and `fall_i`: how far line `i`'s highest ink lies above its baseline and its lowest ink below it. They SHALL be taken from the outlines of the glyphs line `i` is drawn with, meaning the line as the renderer shapes it: divided, as the renderer divides it, into segments of one writing direction and one script, where a character of no specific script (a combining mark, a digit, punctuation, a space) joins the segment around it, and each segment shaped on its own, at that font instance and size, with every glyph at its shaped position and vertical offsets included. A line that draws no ink, because it is blank or holds only spaces, contributes to neither.
- `a` = outer ink above = `max(0, maxᵢ(rise_i − baseline_i(s)))`: how far the block's highest ink rises above its metric top
- `d` = outer ink below = `max(0, maxᵢ(baseline_i(s) + fall_i) − metric_block(n, s))`: how far its lowest ink falls below its last baseline
- `reserve(vertical)` = `a + d` for `top` and `bottom`, and `2 × max(a, d)` for `center`

The font instance is the one the item renders with: the item's weight on the `wght` axis and `s` on the `opsz` axis, re-read at every candidate size, because the outlines move with both axes. The reservation SHALL come from that instance's outlines and the lines being judged, and from nothing else: not from a constant, a tolerance margin, a font-wide band or a sample of rendered pixels.

Only **outer** ink is reserved. Ink between the block's metric top and its last baseline is inside the box the fit already charges, whichever line draws it, so a descender on a first line or an accent on a last line reserves nothing while the pitch keeps it inside. The maxima run over every line, not only the first and the last, so a pitch small enough to push an interior line's ink past the block's edge is still measured.

A one-line block is one cap-height box whatever the pitch, so `metric_block(1, s)` is pitch-independent and the `text-line-spacing` single-line no-op holds at the fitting level with no special case.

A block of `n` lines at size `s` SHALL be treated as fitting a box of height `H` when `metric_block(n, s) + reserve(vertical) ≤ H`, both terms computed on those `n` lines at size `s`. Every judgement of height SHALL use this one comparison, each on the lines it is deciding about:

- **size selection** judges the lines broken at each candidate size;
- **the `ellipsis` line budget** keeps the largest `k`, from `n` down to 1, whose emitted form fits, where the emitted form of `k` lines is the first `k` lines with every over-wide line shortened in place and, when `k < n`, the marker appended to the `k`-th. The reservation is computed on that emitted form, the marker's glyphs included, because dropping or shortening a line changes the ink the block carries. Fitting is not monotonic in `k`, so the item SHALL be refused only when no `k` fits; there is no separate one-line check that could refuse it first. The previous closed-form budget `max(1, floor((H − reserve × s + leading(s)) / pitch(s)))` and its one-line floor are retired: the reserve is no longer known until the surviving lines are;
- **the `overflow: fail` verdict** judges the lines broken at the chosen size;
- **the intrinsic height** is `metric_block(n, s) + reserve(vertical)` on the lines finally emitted.

The fit comparison carries the renderer's existing tolerance of 0.01 pt, and since every judgement above is that comparison, each carries it. Every containment guarantee in this requirement is therefore bounded by that tolerance: ink may sit up to 0.01 pt outside the box. At 180 dpi that is one fortieth of a pixel, enough to change an antialiased edge pixel's coverage and not enough to cut a stroke.

**Containment** SHALL be judged against an unclipped reference and never against a second clipped render, because a clipped render in a taller box clips an under-reserved accent exactly as the exact box does. An item's ink is contained when the same label, rendered with nothing changed but that item's clip removed and with the item's box placed clear of every label edge so no ink it draws can leave the page, puts no ink in any raster row lying wholly outside the item's box. The rows each box edge falls in are the item's own rows, and are where the tolerance above lands. Removing only the clip keeps the text at the same raster phase as the clipped render, so the reference shows exactly the ink the clip would have cut.

`center` SHALL reserve twice the larger outer ink rather than the sum, because the block is centred on its metric box: the slack `(H − metric_block) / 2` left on each side must absorb the outer ink on that side alone. A block whose outer ink is on one side only, such as `É` alone, therefore reserves twice that ink when centred and once when `top`-aligned.

The reservation SHALL be applied identically whether the size was chosen from a `font_size` range or written as a fixed number. A fixed size cannot shrink, so on a fixed-size item the reservation is visible in the line count and in the verdict of the item's `overflow` policy, never in the size.

Because the reservation follows the ink, two values of the same width and line count can fit at different sizes, keep different numbers of lines, and resolve different `content` heights. That is the intended outcome, and it replaces the rule this requirement previously stated, that per-string glyph bounds SHALL NOT decide a size or a placement. What stays independent of the glyphs is where a `center`-aligned baseline sits, which the *Vertical alignment places a fixed metric box, inset only by the ink the block carries past its aligned edge* requirement owns, together with every other placement rule. That requirement reads the same `a` and `d` from the same emitted lines as the fit.

The reservation is part of the item's **intrinsic height** for every alignment, because that height is the room the block needs and clipped ink is not room it has. An item asking for a `content` height therefore resolves a box of `metric_block(n, s) + reserve(vertical)`, and its content sits within that box as its alignment says. An item whose height is authored, or comes from `fill` or `to`, is unaffected: its box is decided without reference to the intrinsic.

The guarantee covers every glyph the rendering font supplies, wherever it inks, including those that rise above the font's ascender or fall below its descender. A character the rendering font does not map is outside it, as the text-layout requirement states: Typst draws that character from a fallback font the measurement does not read, and it is #254's scope.

This requirement supersedes, in the frozen `docs/SPEC.md` §3.1, the "**`top` and `bottom` inset the block so its ink stays inside the slot**" bullet, the "**Two limits worth knowing**" bullet, and the definition of the `overflow` term in the wrapped line-count formula `floor((H − overflow + leading) / (cap_height + leading))`. The paragraph they sit under, "**What `alignment.vertical` aligns.**", and its lowercase bullet are superseded by the alignment requirement named above, and the blank-edge bullet by the text-layout requirement. The Typst-default leading (`0.65em`) an earlier revision of this requirement inherited is retired: `leading(s)` above is derived from the authored pitch, and the pitch itself comes from the `text-line-spacing` capability, which owns its default.

#### Scenario: Aligned edges are unchanged

- **WHEN** a `top`-aligned `text` with a fixed `font_size` of 20 and `overflow: fail` renders `HELIX` in a box exactly `cap_height(20 pt)` tall
- **THEN** it renders, because its reservation is zero
- **AND** before this requirement the same template was refused with `text_does_not_fit`, having reserved 0.4824em it did not use
- **AND** this scenario keeps its name from the superseded version, which held `top` and `bottom` to the font's bands; they now reserve the value's own ink, so the aligned edges do change

#### Scenario: Auto-shrink sees the emitted ink

- **WHEN** a `top`-aligned `text` with a `font_size` range renders `HELIX` and then `HÉLIX`, in a box wide enough for either at the range's maximum and too short for either there
- **THEN** `HÉLIX` settles at a smaller size than `HELIX`, because the two values have the same advances and only `HÉLIX` carries ink above cap height
- **AND** `HÉLIX`'s ink, its accent included, is contained

#### Scenario: A centred block auto-shrunk into a tight box keeps its descenders

- **WHEN** a 24 mm tape template whose `center`-aligned, `wrap: true` `text` item is 120 mm wide and fills the full 18.1 mm printable height, with `font_size: { min: 10, max: 32 }`, renders a value carrying a descender that breaks to two lines, such as "Kitchen Utensils and a much longer second line here"
- **THEN** the chosen size is the largest 0.5 pt step at which `metric_block(2, s) + 2 × max(a, d)` fits 51.31 pt, with `a` and `d` measured on the two lines broken at that step
- **AND** the block's ink is contained, the descender of `g` included, where the same template and value before #245 cut that descender mid-stroke on the box's final raster row

#### Scenario: A centred multiline block's line budget counts the reserve

- **WHEN** a `center`-aligned `wrap: false` `text` with a fixed `font_size` and `overflow: ellipsis` sits in a box exactly `metric_block(3, s)` tall and wide enough for each line with the marker, and renders `HELIX\nHELIX\nHELIX`, and then `HELIX\nHELIX\nHELgX`
- **THEN** the first value keeps all three lines and carries no marker, because its reservation is zero
- **AND** the second keeps two lines, the second ending in `...`, because its three-line form must reserve twice the depth of `g` and its two-line emitted form, marker included, fits
- **AND** with `overflow: fail` the first renders and the second fails with reason `text_does_not_fit`
- **AND** in each render that succeeds the kept block's ink is contained

#### Scenario: Ink between the lines reserves nothing

- **WHEN** a `top`-aligned two-line `text` with `size: [content, content]` at the default pitch is laid out at size `s` with `Hg\nÉH`, and then with `ÉH\nHg`
- **THEN** `Hg\nÉH` resolves a box exactly `metric_block(2, s)` tall
- **AND** `ÉH\nHg` resolves a box `metric_block(2, s) + a + d` tall, `a` being `É`'s accent height above cap height and `d` the depth of `g`

#### Scenario: A centred item asking for a content height grows by the reservation

- **WHEN** a `text` with `size: [content, content]` is laid out at size `s` with `HELIX`, and then with `Égypt`, first `top`-aligned and then `center`-aligned
- **THEN** `HELIX` resolves a box exactly `cap_height(s)` tall under either alignment
- **AND** `top`-aligned `Égypt` resolves `cap_height(s) + a + d`, its ink reaching the box's top and bottom edges and contained
- **AND** `center`-aligned `Égypt` resolves `cap_height(s) + 2 × max(a, d)` with its metric block centred, so the gaps between its ink and the box's edges are `max(a, d) − a` above and `max(a, d) − d` below
- **AND** this scenario keeps its name from the superseded version, where the growth was the font's bands whatever the value

#### Scenario: An asymmetric font reserves twice its larger overflow

- **WHEN** a `text` with `size: [content, content]` is laid out at size `s` with `É`, which inks above cap height and not below the baseline
- **THEN** `center`-aligned it resolves `cap_height(s) + 2a`, and `top`-aligned `cap_height(s) + a`
- **AND** this scenario keeps its name from the superseded version, where the asymmetry had to come from a font supplied through `LABELER_FONTS_DIR`; it now comes from the value, so the bundled font shows it

#### Scenario: The reservation is read from the instance rendered

- **WHEN** a `top`-aligned `text` with `size: [content, content]` and a fixed `font_size` renders `É` at `font_weight: 400`, and then at `font_weight: 700`
- **THEN** each resolved height is `cap_height(s)` plus the accent's height above cap height in the outlines of that weight's instance at that size
- **AND** the two heights differ, as the two instances' accents do

#### Scenario: A glyph outside the declared band still clips

- **WHEN** a `top`-aligned `text` with a fixed `font_size` renders `Ǻ`, whose ink rises above Inter's typographic ascender, in a box exactly as tall as its intrinsic height
- **THEN** the render succeeds and the glyph's ink is contained
- **AND** before this requirement the reservation stopped at the ascender and the top of the glyph was clipped
- **AND** this scenario keeps its name from the superseded version, under which such a glyph could clip; a glyph the font supplies no longer does, and the one remaining clip is the unmapped character below

#### Scenario: A value of ten thousand lines is shortened like a value of three

- **WHEN** a `top`-aligned `wrap: false` `text` with a fixed `font_size`, `overflow: ellipsis` and a box holding two lines at that size renders a value of 10,000 short lines
- **THEN** it keeps the first two lines, the second ending in `...`, and the render completes
- **AND** no line is measured for ink more than twice in reaching that answer, and a line whose baseline would lie more than 0.01 pt below the box's bottom edge is not measured for ink at all, since no run containing it can fit within the fit comparison's tolerance, because every line's hard break is laid out and the budget's work must grow with the line count rather than with its square

#### Scenario: The metric cutoff honours the fit tolerance

- **WHEN** a `top`-aligned `text` with a fixed `font_size` of 20 and `overflow: fail` renders `HELIX`, whose metric block is 14.55078125 pt and whose reservation is zero, in a box 14.54578125 pt tall, and then in a box 14.53578125 pt tall
- **THEN** it renders in the first box, 0.005 pt short of its metric block and within the 0.01 pt tolerance, and fails with reason `text_does_not_fit` in the second, 0.015 pt short
- **AND** the same two boxes give the same two outcomes under `overflow: ellipsis`, so the line budget's cutoff before measuring ink accepts and refuses exactly where the fit comparison does

#### Scenario: A mark after a change of script is measured in its own segment

- **WHEN** a `top`-aligned `text` with a fixed `font_size` of 20 renders `αH\u0301` (a Greek alpha, then `H` carrying a combining acute) in a box exactly as tall as its intrinsic height
- **THEN** it is inset at the top by exactly what `H\u0301` alone is inset by, because the renderer shapes the Latin segment `H\u0301` on its own and places the acute over the `H`
- **AND** its ink is contained
- **AND** a measurement shaping the line as a single segment places the acute 393 font units lower in the bundled font, under-reserving by 3.84 pt at this size, which is the clip this scenario exists to refuse

#### Scenario: A character the font does not map is outside the guarantee

- **WHEN** a `text` renders a value containing a character the rendering font has no glyph for
- **THEN** the item is fitted and rendered, and is not refused on that character's account
- **AND** its reservation at that position is the one the rendering font's own missing-glyph outline calls for, whatever the fallback glyph Typst draws there inks, which is #254's scope

#### Scenario: A centred item with headroom is unaffected

- **WHEN** a `center`-aligned text item rendering a single line, a value with no line breaks that fits its box width at `font_size.max`, sits in a box whose height is authored, `fill` or `to`, and that box is tall enough at `font_size.max` for both the measured reservation and the font-band reservation before this requirement: `metric_block(1, max) + 2 × max(a, d) ≤ H` and `metric_block(1, max) + 2 × max(u, d_font) × max ≤ H`, with `u` and `d_font` the font's ascent and descent overflows that revision reserved
- **THEN** the size the fitter chooses is decided by width alone, exactly as before, and the item resolves the same box, size and lines it did before
- **AND** the rendered output is byte-identical to what the same template and data produced before this requirement, because a centred block's placement never read the reservation

#### Scenario: A height-bound centred item picks a larger size

- **WHEN** a `center`-aligned `text` item with ample width and `font_size: { min: 10, max: 20 }` renders `HELIX` in a box 16 pt tall
- **THEN** it renders at 20 pt, because its metric block of 14.55 pt fits with nothing reserved
- **AND** before this requirement the same template and value rendered at 13.0 pt, the largest step at which `1.2099 × s ≤ 16`, so its output changes, and the unchanged-output scenario above does not cover it

#### Scenario: The default pitch tightens existing multi-line items

- **WHEN** a two-line `wrap: true` item declaring no `line_spacing` renders with the bundled font at its fitted size `s`
- **THEN** its metric block is `cap_height(s) + 1.2 × s`, not the `2 × cap_height(s) + 0.65 × s` an earlier revision reserved
