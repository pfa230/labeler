# Text

## Purpose

The `text` layout item: its keys, fonts, line breaking, auto-shrink, overflow, line spacing, ink reservation, alignment inside its box, and colour.

## Requirements

### Requirement: Text item keys

A `text` item SHALL accept the keys below in addition to placement and `when` (owned by `layout`). A refused value SHALL fail the template at load with an error naming the item's layout path and the key.

| Key | Type | Default | Constraint |
|---|---|---|---|
| `value` | string, interpolated (`interpolation`) | required | |
| `font` | string, literal family name | Inter | see "Font selection" |
| `font_size` | number, or `{ min, max }`, in points | required | every number > 0; `min` ≤ `max` |
| `font_weight` | integer, or `"{param}"` | 400 | literal: multiple of 100 from 100 to 900; reference: a declared `integer` parameter whose `default` meets the literal rule |
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

### Requirement: Font weight resolution

A literal `font_weight` SHALL be the weight. A reference SHALL be resolved per render from the request data with defaults applied (`parameters`); the resolved integer SHALL be the weight. A supplied value SHALL meet the literal rule, else the render fails with `400 InvalidRequest` reason `param_value_invalid` naming the parameter.

#### Scenario: A referenced weight renders that weight

- **WHEN** `font_weight: "{w}"` is rendered with `w` of `700`, and then of `450`
- **THEN** the first render is measured and drawn at weight 700, and the second fails with `400 InvalidRequest` reason `param_value_invalid` naming `w`

### Requirement: Line breaking

A value's lines SHALL be its segments at `\n`, after `\r\n` and lone `\r` are normalised to `\n` before measurement. Every line SHALL be laid out and emitted with its own line box, including blank first, last and interior lines; an empty value is one blank line.

With `wrap: false` each line SHALL be emitted as written, spaces preserved and never broken further. With `wrap: true` each line SHALL be broken at whitespace to the box width at the candidate size, never inside a word: a word wider than the box stays whole on its own line and is resolved by auto-shrink and then `overflow`. Wrapping SHALL collapse each whitespace run to one space and trim leading and trailing whitespace; a whitespace-only line becomes a blank line box.

#### Scenario: A hard break survives with wrapping off

- **WHEN** a `wrap: false` item renders two non-blank lines in a box tall enough for both and wide enough for each
- **THEN** both lines are emitted, in order, unbroken

#### Scenario: CRLF costs nothing

- **WHEN** an item on a dynamic-width single label renders `"abc\rabc"` or `"abc\r\nabc"`
- **THEN** it lays out and renders exactly as `"abc\nabc"`, including line count, chosen size and auto-length label width

#### Scenario: A leading blank line is a line

- **WHEN** a `wrap: true` item with `font_size: { min: 8, max: 20 }` renders a blank line then two short lines, in a box holding two lines at 20 pt and three at 14 pt
- **THEN** 14 pt is chosen and all three lines are emitted, with no marker

### Requirement: Layout box

A text SHALL be laid out against the box it will get: its own extent when authored, otherwise the available extent capped by `max_w`/`max_h` (`layout`); on a dynamic-width `single` the frame for this is `format.width.max`. Layout SHALL run for every active text item, including one whose box is fully authored. Breaks and size SHALL NOT be re-decided when the final box is larger than the box laid out against; the extra space is slack for alignment. When primary-axis packed-fill allocation makes a text's final box smaller on either axis, it SHALL be fitted again against that box using "Line breaking", "Auto-shrink" and "Overflow policy". The same box and value SHALL give the same lines and outcome on every format. `alignment.horizontal` SHALL position each line's shaped advance box using the horizontal reservation and insets in "Measured block and ink reservation"; that reservation SHALL contribute to a `content` width.

#### Scenario: A fully authored box still enforces its policy

- **WHEN** an item with `size: [40, 10]`, a fixed `font_size` and `overflow: fail` carries a value that does not fit
- **THEN** the render fails with `text_does_not_fit`, and with `overflow: ellipsis` it is shortened to the 40 by 10 box

#### Scenario: Alignment does not change the box

- **WHEN** the same `content`-width item whose emitted lines have no horizontal ink overhang renders with `alignment.horizontal` of `left` and of `center`
- **THEN** both boxes are the widest emitted line's shaped advance width

#### Scenario: A packed title fits its allocated width

- **WHEN** a row leaves a wrapped fill title 18 units of width after a badge and gap
- **THEN** its lines and selected font size match that text laid out in an authored 18-unit-wide box with equal height

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

For horizontal measurement, each line SHALL have its shaped advance `advance_i` and its ink interval `[left_i, right_i]` relative to that advance's origin, using the same font instance and shaped glyph positions as emission. A line without ink contributes its advance and no ink interval. With `metric_width = maxᵢ(advance_i)`, the renderer SHALL use:

- `x_i = 0` for `left`, `(metric_width − advance_i) / 2` for `center`, and `metric_width − advance_i` for `right`;
- `l = max(0, maxᵢ(−(x_i + left_i)))`, `r = max(0, maxᵢ(x_i + right_i − metric_width))`, over lines with ink;
- `horizontal_reserve = l + r` for `left` and `right`, `2 × max(l, r)` for `center`.

Lines SHALL fit a box of height `H` when `metric_block + reserve ≤ H + 0.01 pt`, and width `W` when `metric_width + horizontal_reserve ≤ W + 0.01 pt`, computed on the lines being judged. Size selection, wrapping and ellipsis candidate width checks, the line budget and the `fail` verdict SHALL use the same shaped-width and ink comparisons. The item's intrinsic height SHALL be `metric_block + reserve` and its intrinsic width `metric_width + horizontal_reserve`, both on the lines finally emitted; an empty value has width 0 and height `c`. Ink inside the metric block reserves nothing.

Horizontal `left` alignment SHALL inset the advance block by `l` from the left edge, `right` by `r` from the right edge, and `center` SHALL center each line's advance box with no inset. Punctuation SHALL keep its ordinary shaped position without hanging into the item's margin. Fitted ink SHALL lie inside the item's box to within the tolerance, for every glyph the selected face supplies.

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

#### Scenario: Shaped advances determine a dynamic label's width

- **WHEN** a dynamic-width single label renders a content-width line of repeated `W` characters shortened with `...`
- **THEN** its width is derived from the emitted line's shaped advances and horizontal ink reservation

#### Scenario: Horizontal center reserves twice the larger side

- **WHEN** one emitted line has advance `A`, left overhang `l` greater than zero and no right overhang
- **THEN** its content width is `A + l` with left or right alignment and `A + 2l` with center alignment

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

### Requirement: Font selection

A text item's `font` SHALL select a family by the renderer's normalized family name, case-insensitively. A name absent from the available families SHALL be refused. Measurement and emission SHALL use the face the renderer's matcher selects for the requested family at normal style, normal stretch and the item's weight. Variable axes, when present, SHALL use the resolved `font_weight` for `wght`, normal stretch (100) for `wdth` and candidate point size for `opsz`, clamped to the face's axis bounds; fonts lacking any of these axes SHALL remain usable.

A text value containing a drawable character the selected face does not map SHALL fail before fitting with `422 UnsupportedLayoutItem` reason `text_character_unmapped` (`errors`). Line breaks, tabs and Unicode default-ignorable characters SHALL NOT require a drawable glyph.

#### Scenario: A family changes fitting and omission preserves Inter

- **WHEN** the same value and geometry are rendered with `font: Roboto`, with `font: Inter`, and with `font` omitted
- **THEN** each result is fitted and drawn using its selected face's metrics
- **AND** the omitted-family result equals the explicit Inter result

#### Scenario: Unknown family is invalid

- **WHEN** a text item names a family absent from the available fonts
- **THEN** the template is refused

#### Scenario: Static and weight-only variable fonts are usable

- **WHEN** a text item selects an available static family, or JetBrains Mono which has a weight axis but no optical-size axis
- **THEN** it is measured and rendered with the selected face and the same weight matching

#### Scenario: Unmapped text is refused instead of shortened

- **WHEN** an active item's resolved value contains a drawable character absent from its selected face under either overflow policy
- **THEN** rendering fails with `text_character_unmapped`, including when fitting would otherwise omit that character
- **AND** supported text in that family renders normally when it fits

### Requirement: Available font families

Inter, Source Sans 3, JetBrains Mono and Roboto Condensed SHALL be bundled with their licenses in `LABELER_FONTS_DIR` (default `fonts/`), selectable as `Inter`, `SourceSans3VF`, `JetBrains Mono` and `Roboto`, respectively. Custom TrueType and OpenType font files in `{LABELER_CONFIG_DIR}/fonts` SHALL add selectable families without rebuilding the service. A custom face whose normalized family name matches a bundled family name, case-insensitively, SHALL NOT be loaded. No other family, host-installed or renderer-embedded, SHALL be selectable.

The available fonts and their bytes SHALL be held for the process lifetime; adding, changing or removing a file SHALL take effect only on restart. An absent custom-font directory SHALL add no families.

#### Scenario: A custom font survives disk removal until restart

- **WHEN** a template selects a custom family loaded from the config fonts directory and its file is removed while the service runs
- **THEN** the template continues to render with the loaded font
- **AND** after restart that template fails validation because the family is no longer available

#### Scenario: An installation without custom fonts uses the bundled set

- **WHEN** no custom-font directory exists and an installed host font has a family absent from the bundled set
- **THEN** the four bundled families are selectable and the host-only family is refused by template validation
