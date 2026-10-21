## MODIFIED Requirements

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

## REMOVED Requirements

### Requirement: Fonts

**Reason**: A selectable family replaces the single required Inter instance.
**Migration**: Omitted `font` continues to select Inter.

## ADDED Requirements

### Requirement: Font selection

A text item's `font` SHALL select a family by the renderer's normalized family name, case-insensitively. A name absent from the available families SHALL fail template validation at the item's `font` path (`templates`). Measurement and emission SHALL use the face the renderer's matcher selects for the requested family at normal style, normal stretch and the item's weight. Variable axes, when present, SHALL use the resolved `font_weight` for `wght` and candidate point size for `opsz`, clamped to the face's axis bounds; fonts lacking either axis SHALL remain usable.

A text value containing a drawable character the selected face does not map SHALL fail before fitting with `422 UnsupportedLayoutItem` reason `text_character_unmapped` (`errors`). Line breaks, tabs and Unicode default-ignorable characters SHALL NOT require a drawable glyph.

#### Scenario: A family changes fitting and omission preserves Inter

- **WHEN** the same value and geometry are rendered with `font: Roboto`, with `font: Inter`, and with `font` omitted
- **THEN** each result is fitted and drawn using its selected face's metrics
- **AND** the omitted-family result equals the explicit Inter result

#### Scenario: Unknown family is invalid

- **WHEN** a text item names a family absent from the available fonts
- **THEN** the template is refused by template validation with a message naming that item's `font` path

#### Scenario: Static and weight-only variable fonts are usable

- **WHEN** a text item selects an available static family, or JetBrains Mono which has a weight axis but no optical-size axis
- **THEN** it is measured and rendered with the selected face and the same weight matching

#### Scenario: Unmapped text is refused instead of shortened

- **WHEN** an active item's resolved value contains a drawable character absent from its selected face under either overflow policy
- **THEN** rendering fails with `text_character_unmapped`, including when fitting would otherwise omit that character
- **AND** supported text in that family renders normally when it fits

### Requirement: Available font families

Inter, Source Sans 3, JetBrains Mono and Roboto Condensed SHALL be bundled with their licenses in `LABELER_FONTS_DIR` (default `fonts/`), selectable as `Inter`, `SourceSans3VF`, `JetBrains Mono` and `Roboto`, respectively. Custom TrueType and OpenType font files in `{LABELER_CONFIG_DIR}/fonts` SHALL add selectable families without rebuilding the service. Bundled families SHALL take precedence over custom files declaring the same family. No other family, host-installed or renderer-embedded, SHALL be selectable.

The available fonts and their bytes SHALL be held for the process lifetime; adding, changing or removing a file SHALL take effect only on restart. An absent custom-font directory SHALL add no families.

#### Scenario: A custom font survives disk removal until restart

- **WHEN** a template selects a custom family loaded from the config fonts directory and its file is removed while the service runs
- **THEN** the template continues to render with the loaded font
- **AND** after restart that template fails validation because the family is no longer available

#### Scenario: An installation without custom fonts uses the bundled set

- **WHEN** no custom-font directory exists and an installed host font has a family absent from the bundled set
- **THEN** the four bundled families are selectable and the host-only family is refused by template validation
