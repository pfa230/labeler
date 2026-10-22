# Spec Delta

## MODIFIED Requirements

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
