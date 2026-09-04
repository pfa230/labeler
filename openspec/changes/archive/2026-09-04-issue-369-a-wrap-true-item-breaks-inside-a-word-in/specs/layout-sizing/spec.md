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
   ink reservation the *Vertical fitting reserves the ink each alignment can expose* requirement
   defines for the item's `alignment.vertical`. The text SHALL be re-broken at each candidate's glyph advances, as today's
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
it. This supersedes ADR-0059, under which a centred auto-width text on a dynamic-width label was
given the alignment slot as its box while a left-aligned one was given the laid-out width.

**`overflow`.** A `text` SHALL carry an `overflow` field with the values `ellipsis` (the default) and
`fail`. Both shorten nothing the other would not; they differ in when they give up:

| | `ellipsis` | `fail` |
| --- | --- | --- |
| fits as authored | render it | render it |
| fits once shortened | render the shortened form | `text_does_not_fit` |
| cannot fit however short | `text_does_not_fit` | `text_does_not_fit` |

Shortening has two independent paths. Lines that do not fit the block are dropped, and the marker is
appended to the last retained line; a line that is wider than the box is shortened where it sits,
whether or not anything was dropped. Either path trims characters until the line and the marker fit.
The marker reports the **field**, not the line it sits on: it is appended whenever any line was
dropped, whether that line carried glyphs or was blank, and for a dropped line it lands at the end
of the last retained line whatever that line holds. A value whose every line is shown, unshortened, carries no marker.
The shortest form it can produce is the marker alone, so shortening succeeds whenever `...` fits the
box width and the box holds at least one line, and fails otherwise. Two cases therefore reach the
third row, and neither is a separate rule:

- the box is narrower than `...` itself, so there is nothing shorter to produce;
- the box is shorter than one line at the chosen size, since a line's height comes from the font size
  and the line count is already at its floor of one.

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

The policy SHALL be evaluated against the **metric model** ADR-0045, ADR-0050 and ADR-0084 define:
the cap-height-to-baseline line box plus the ink reservation for the item's `alignment.vertical`,
including `center`, and not against glyph outlines. Widening that model widens what the policy
refuses, and both effects are intended: a `center`-aligned item whose block fits its box but whose
block plus reservation does not SHALL be shortened under `ellipsis` and SHALL raise
`text_does_not_fit` under `fail`, and a box that cannot hold one line plus its reservation at the
chosen size SHALL raise `text_does_not_fit` under either.

One ADR-0050 consequence stands and is not superseded: a glyph inking outside the font's own
ascender/descender band can still clip at any alignment. That is ink leaving a box the metric model
says it fits in, which no policy evaluated on metrics can see. Centred text clipping in a slot
shorter than `1.21 × font_size` is no longer one of them: such a slot is now an overflow, and the
policy resolves it.

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
- **THEN** it is laid out as one line carrying no glyphs, and its intrinsic height is one line box
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
