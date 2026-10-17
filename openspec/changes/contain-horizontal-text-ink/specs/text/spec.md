## MODIFIED Requirements

### Requirement: Layout box

A text SHALL be laid out against the box it will get: its own extent when authored, otherwise the available extent capped by `max_w`/`max_h` (`layout`); on a dynamic-width `single` the frame for this is `format.width.max`. Layout SHALL run for every active text item, including one whose box is fully authored. Breaks and size SHALL NOT be re-decided when the final box is larger than the box laid out against; the extra space is slack for alignment. The same box and value SHALL give the same lines and outcome on every format. `alignment.horizontal` SHALL position each line's shaped advance box using the horizontal reservation and insets in "Measured block and ink reservation"; that reservation SHALL contribute to a `content` width.

#### Scenario: A fully authored box still enforces its policy

- **WHEN** an item with `size: [40, 10]`, a fixed `font_size` and `overflow: fail` carries a value that does not fit
- **THEN** the render fails with `text_does_not_fit`, and with `overflow: ellipsis` it is shortened to the 40 by 10 box

#### Scenario: Alignment does not change the box

- **WHEN** the same `content`-width item whose emitted lines have no horizontal ink overhang renders with `alignment.horizontal` of `left` and of `center`
- **THEN** both boxes are the widest emitted line's shaped advance width

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

Horizontal `left` alignment SHALL inset the advance block by `l` from the left edge, `right` by `r` from the right edge, and `center` SHALL center each line's advance box with no inset. Punctuation SHALL keep its ordinary shaped position without hanging into the item's margin. Fitted ink SHALL lie inside the item's box to within the tolerance, for every glyph Inter supplies.

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
