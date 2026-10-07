# Text

## Purpose

The `text` layout item: its keys, fonts, line breaking, auto-shrink, overflow, line spacing, ink reservation, alignment inside its box, and colour.

## Requirements

### Requirement: Text item keys

A `text` item SHALL accept the keys below in addition to placement and `when` (owned by `layout`). A refused value SHALL fail the template at load with an error naming the item's layout path and the key.

| Key | Type | Default | Constraint |
|---|---|---|---|
| `value` | string, interpolated (`interpolation`) | required | |
| `font_size` | number, or `{ min, max }`, in points | required | every number > 0; `min` ≤ `max` |
| `font_weight` | integer, or `"{param}"` | 400 | literal: multiple of 100 from 100 to 900; reference: a declared `integer` parameter whose `default`, if any, meets the literal rule |
| `color` | colour (`layout` colour vocabulary) | black | |
| `wrap` | boolean | `false` | |
| `line_spacing` | number | 1.2 | finite and > 0 |
| `alignment.horizontal` | `left`, `center`, `right` | `left` | |
| `alignment.vertical` | `top`, `center`, `bottom` | `top` | |
| `overflow` | `ellipsis`, `fail` | `ellipsis` | |

#### Scenario: A weight off the hundreds is refused

- **WHEN** a `text` item declares `font_weight: 450`
- **THEN** the template fails to load with an error naming `font_weight`

#### Scenario: An inverted font range is refused

- **WHEN** a `text` item declares `font_size: { min: 12, max: 8 }`
- **THEN** the template fails to load

#### Scenario: A pitch other than a positive number is refused

- **WHEN** a `text` item declares `line_spacing` as `0`, `-0.5`, `.nan`, `"1.2"`, `"{pitch}"` or `true`
- **THEN** the template fails to load with an error naming `line_spacing`

### Requirement: Fonts

Text SHALL be measured and rendered in Inter, loaded from `InterVariable.ttf` in `LABELER_FONTS_DIR` (default `fonts/`); host system fonts SHALL never be used. The font file MUST carry the `wght` and `opsz` variation axes. Every measurement and render of an item SHALL use the instance at the item's weight (`wght`) and the candidate font size (`opsz`). A font file that cannot be read, cannot be parsed, or lacks either axis SHALL fail the render with `500 Internal`. A character Inter does not map SHALL be fitted and rendered, drawn by Typst from a fallback face, and lies outside the ink guarantee of "Measured block and ink reservation".

#### Scenario: A font without an opsz axis fails loudly

- **WHEN** `LABELER_FONTS_DIR` holds an `InterVariable.ttf` with no `opsz` axis and a text item is rendered
- **THEN** the render fails with `500 Internal`

### Requirement: Font weight resolution

A literal `font_weight` SHALL be the weight. A reference SHALL be resolved per render from the request data with defaults applied (`parameters`); the resolved integer SHALL be the weight. A supplied value SHALL meet the literal rule, else the render fails with `400 InvalidRequest` reason `param_value_invalid` naming the parameter. An absent value fails with `422 UnsupportedLayoutItem` reason `missing_field`.

#### Scenario: A referenced weight renders that weight

- **WHEN** `font_weight: "{w}"` is rendered with `w` of `700`, and then of `450`
- **THEN** the first render is measured and drawn at weight 700, and the second fails with `400 InvalidRequest` reason `param_value_invalid` naming `w`

### Requirement: Line breaking

A value's lines SHALL be its segments at `\n`, after `\r\n` is normalised to `\n`; a lone `\r` is not a break. Every line SHALL be laid out and emitted with its own line box, including blank first, last and interior lines; an empty value is one blank line.

With `wrap: false` each line SHALL be emitted as written, spaces preserved and never broken further. With `wrap: true` each line SHALL be broken at whitespace to the box width at the candidate size, never inside a word: a word wider than the box stays whole on its own line and is resolved by auto-shrink and then `overflow`. Wrapping SHALL collapse each whitespace run to one space and trim leading and trailing whitespace; a whitespace-only line becomes a blank line box.

#### Scenario: A hard break survives with wrapping off

- **WHEN** a `wrap: false` item renders two non-blank lines in a box tall enough for both and wide enough for each
- **THEN** both lines are emitted, in order, unbroken

#### Scenario: CRLF costs nothing

- **WHEN** an item renders `"abc\r\nabc"`
- **THEN** it lays out exactly as `"abc\nabc"`, in line count, chosen size and width

#### Scenario: A leading blank line is a line

- **WHEN** a `wrap: true` item with `font_size: { min: 8, max: 20 }` renders a blank line then two short lines, in a box holding two lines at 20 pt and three at 14 pt
- **THEN** 14 pt is chosen and all three lines are emitted, with no marker

### Requirement: Layout box

A text SHALL be laid out against the box it will get: its own extent when authored, otherwise the available extent capped by `max_w`/`max_h` (`layout`); on a dynamic-width `single` the frame for this is `format.width.max`. Layout SHALL run for every active text item, including one whose box is fully authored. Breaks and size SHALL NOT be re-decided when the final box is larger than the box laid out against; the extra space is slack for alignment. The same box and value SHALL give the same lines and outcome on every format. `alignment.horizontal` SHALL position each line inside the box and SHALL NOT change the box.

#### Scenario: A fully authored box still enforces its policy

- **WHEN** an item with `size: [40, 10]`, a fixed `font_size` and `overflow: fail` carries a value that does not fit
- **THEN** the render fails with `text_does_not_fit`, and with `overflow: ellipsis` it is shortened to the 40 by 10 box

#### Scenario: Alignment does not change the box

- **WHEN** the same `content`-width item renders with `alignment.horizontal` of `left` and of `center`
- **THEN** both boxes are the laid-out text width

### Requirement: Auto-shrink

A `font_size` range SHALL select the largest size in `max`, `max − 0.5`, `max − 1`, … not below `min` at which every line, re-broken at that size, fits the box width and the measured block fits the box height. When no candidate fits, `min` SHALL be used and the `overflow` policy resolves the rest. A fixed `font_size` SHALL skip selection. Shrinking SHALL happen before the overflow policy is consulted.

#### Scenario: Shrinking happens before the policy

- **WHEN** an item with `font_size: { min: 8, max: 20 }` and `overflow: fail` carries a value that fits at 12 but not at 20
- **THEN** it renders at 12 with no error

#### Scenario: An over-wide word shrinks whole

- **WHEN** a `wrap: true` item carries a word too wide at `max` but fitting whole within the range
- **THEN** it renders that word unbroken on one line at the largest such size

#### Scenario: A height-bound item follows its ink

- **WHEN** a `top`-aligned item with a range renders `HELIX` and then `HÉLIX` in a box too short for either at `max`
- **THEN** `HÉLIX` settles at a smaller size than `HELIX`

### Requirement: Overflow policy

What does not fit at the chosen size SHALL be resolved by `overflow`:

| | `ellipsis` | `fail` |
|---|---|---|
| fits as laid out | render | render |
| fits once shortened | render the shortened form | `text_does_not_fit` |
| no shortened form fits | `text_does_not_fit` | `text_does_not_fit` |

Shortening SHALL keep the first `k` lines for the largest `k` whose emitted form fits the box height (measured block, reservation included), judging every `k` from all lines down to one, since a longer run can fit where a shorter one does not. In the emitted form every line wider than the box SHALL be trimmed character by character from its end until it plus the marker `...` fits, wherever it sits; when lines were dropped, line `k` SHALL be trimmed the same way and SHALL end in the marker, even when it is blank. Height SHALL be shortened by whole lines only. A value shown whole and unshortened SHALL carry no marker. No shortened form exists when the box is narrower than `...` at the chosen size or when no leading run fits the height. Clipping SHALL never be an outcome.

`text_does_not_fit` is `422 UnsupportedLayoutItem`; its message names the item's layout path.

#### Scenario: An over-wide line is shortened in place

- **WHEN** a `wrap: true` `ellipsis` item at a fixed size has a first line wider than its box and a last line that fits
- **THEN** the first line ends in `...` and the last is emitted untouched

#### Scenario: An over-wide word at a fixed size

- **WHEN** a `wrap: true` item at a fixed size carries a word wider than its box, in a box wider than `...`
- **THEN** `ellipsis` renders it shortened with `...`, `fail` fails with `text_does_not_fit`, and in a box narrower than `...` both fail

#### Scenario: A box too short for one line

- **WHEN** an item with fixed `font_size: 20` sits in a box 2 units tall
- **THEN** the render fails with `text_does_not_fit` under either policy

#### Scenario: A dropped blank line earns the marker

- **WHEN** a `wrap: false` `ellipsis` item renders `"message\n"` in a box one line tall, wide enough for `message` but not `message...`
- **THEN** the emitted line ends in `...`, with characters of `message` removed to make room
- **AND** `"\nmessage"` in the same box renders as `...` alone

#### Scenario: A longer run fits where the first line alone does not

- **WHEN** a `center`-aligned `wrap: false` `ellipsis` item at `font_size: 20`, `line_spacing: 0.5`, in a box 28 pt tall, renders `g` with a combining dot below (U+0323), then two lines of `HELIX`
- **THEN** it keeps two lines, the second ending in `...`, although one line alone would not fit

#### Scenario: An empty value in a zero-width box

- **WHEN** a `content`-width item bound to an empty value resolves to a zero-wide box
- **THEN** it renders empty with no error

### Requirement: Line spacing

For an item at font size `s`, the baseline-to-baseline pitch SHALL be `line_spacing × s`, with `line_spacing` 1.2 when undeclared. A single-line item SHALL render identically at any `line_spacing`.

#### Scenario: An authored pitch lands on the render

- **WHEN** an item with `line_spacing: 0.99` renders `"Hxy\nHxy"`
- **THEN** the two lines' ink bands are 0.99 font sizes apart on the rendered image, and 1.2 font sizes apart with no `line_spacing`

### Requirement: Measured block and ink reservation

For `n` lines at size `s`, with `c` the instance's cap height and `p` the pitch, the renderer SHALL use:

- `metric_block = c + (n − 1) × p`; line `i`'s baseline lies `c + (i − 1) × p` below the metric top;
- `rise_i`, `fall_i`: how far line `i`'s ink rises above and falls below its baseline, from the glyph outlines of the instance rendered, each run of one script and direction shaped on its own (marks, digits, punctuation and spaces join the surrounding run); a line with no ink contributes nothing;
- `a = max(0, maxᵢ(rise_i − baseline_i))`, `d = max(0, maxᵢ(baseline_i + fall_i) − metric_block)`;
- `reserve = a + d` for `top` and `bottom`, `2 × max(a, d)` for `center`.

Lines SHALL fit a box of height `H` when `metric_block + reserve ≤ H + 0.01 pt`, computed on the lines being judged; size selection, the line budget and the `fail` verdict SHALL all use this comparison, and their width check carries the same 0.01 pt tolerance. The item's intrinsic height SHALL be `metric_block + reserve` and its intrinsic width the widest line, both on the lines finally emitted; an empty value has width 0 and height `c`. Ink inside the metric block reserves nothing. Fitted ink SHALL lie inside the item's box to within the tolerance, for every glyph Inter supplies.

#### Scenario: Ink between the lines reserves nothing

- **WHEN** a `top`-aligned `content`-height item lays out `Hg\nÉH` and then `ÉH\nHg`
- **THEN** the first is exactly `metric_block(2)` tall and the second `metric_block(2) + a + d`

#### Scenario: Centre reserves twice the larger side

- **WHEN** a `content`-height item lays out `É`
- **THEN** it is `c + 2a` tall `center`-aligned and `c + a` tall `top`-aligned

#### Scenario: The cutoff honours the tolerance

- **WHEN** a `top`-aligned `HELIX` at fixed size 20 (metric block 14.55078125 pt) with `overflow: fail` renders in boxes 14.54578125 pt and 14.53578125 pt tall
- **THEN** it renders in the first and fails with `text_does_not_fit` in the second, and `ellipsis` gives the same two outcomes

#### Scenario: The box's verdict follows the value's ink

- **WHEN** a `top`-aligned item at fixed size 20 with a box exactly cap height tall renders `HELIX`, then `Égypt`
- **THEN** `HELIX` renders and `Égypt` fails with `text_does_not_fit` under either policy

#### Scenario: A mark after a change of script

- **WHEN** a `top`-aligned item at size 20 renders `α` followed by `H` with a combining acute (U+0301), in a box exactly its intrinsic height
- **THEN** it is inset at the top by what the accented `H` alone is inset by, and its ink is contained

### Requirement: Vertical alignment

`alignment.vertical` SHALL place the block's metric box, from the first line's cap-height top to the last baseline, inside the item's box. `center` SHALL centre it with no inset, so the baseline depends only on box, size, pitch and line count. `top` SHALL place the metric top `a` below the top edge, and `bottom` the last baseline `d` above the bottom edge, `a` and `d` taken from the emitted lines as in "Measured block and ink reservation". An inset SHALL never be negative: a block whose ink stops short of its aligned metric edge is not pulled toward it.

#### Scenario: Unaccented capitals sit flush

- **WHEN** a `top`-aligned item renders `HELIX` in a box taller than its cap height
- **THEN** its metric top lies on the box's top edge

#### Scenario: An accent insets by its own height

- **WHEN** a `top`-aligned item renders `Émile` in a box exactly its intrinsic height
- **THEN** its metric top is inset by `É`'s accent height above cap height, and the accent is not clipped

#### Scenario: A centred baseline ignores the glyphs

- **WHEN** one `center`-aligned item renders `HELIX`, `Émile`, `testj` and `gyp` at one fixed size
- **THEN** all four put the baseline on the same raster row

#### Scenario: Short ink is not pulled to the edge

- **WHEN** a `top`-aligned item renders `ace` and then `HELIX` in the same box
- **THEN** both put the baseline on the same raster row

### Requirement: Text colour

`color` SHALL paint the item's glyphs and SHALL NOT affect sizing, breaking or fitting.

#### Scenario: Colour does not move text

- **WHEN** two otherwise identical items differ only in that one declares `color: red`
- **THEN** both resolve the same box, font size and lines
