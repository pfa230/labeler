## MODIFIED Requirements

### Requirement: Line breaking

A value's lines SHALL be its segments at `\n`, after `\r\n` and lone `\r` are normalised to `\n` before measurement. Every line SHALL be laid out and emitted with its own line box, including blank first, last and interior lines; an empty value is one blank line.

With `wrap: false` each line SHALL be emitted as written, spaces preserved and never broken further. With `wrap: true` each line SHALL be broken at whitespace to the box width at the candidate size, never inside a word: a word wider than the box stays whole on its own line and is resolved by auto-shrink and then `overflow`. Wrapping SHALL collapse each whitespace run to one space and trim leading and trailing whitespace; a whitespace-only line becomes a blank line box.

#### Scenario: A hard break survives with wrapping off

- **WHEN** a `wrap: false` item renders two non-blank lines in a box tall enough for both and wide enough for each
- **THEN** both lines are emitted, in order, unbroken

#### Scenario: CRLF costs nothing

- **WHEN** an item renders `"abc\rabc"` or `"abc\r\nabc"`
- **THEN** it lays out and renders exactly as `"abc\nabc"`, including line count, chosen size and auto-length label width

#### Scenario: A leading blank line is a line

- **WHEN** a `wrap: true` item with `font_size: { min: 8, max: 20 }` renders a blank line then two short lines, in a box holding two lines at 20 pt and three at 14 pt
- **THEN** 14 pt is chosen and all three lines are emitted, with no marker
