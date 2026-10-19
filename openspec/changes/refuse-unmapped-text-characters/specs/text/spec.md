## MODIFIED Requirements

### Requirement: Fonts

Text SHALL be measured and rendered in Inter, loaded from `InterVariable.ttf` in `LABELER_FONTS_DIR` (default `fonts/`); host system fonts SHALL never be used. The font file MUST carry the `wght` and `opsz` variation axes. Every measurement and render of an item SHALL use the instance at the item's weight (`wght`) and the candidate font size (`opsz`). A font file that cannot be read, cannot be parsed, or lacks either axis SHALL fail the render with `500 Internal`. A text value containing a drawable character Inter does not map SHALL fail before fitting with `422 UnsupportedLayoutItem` reason `text_character_unmapped` (`errors`). Line breaks, tabs and Unicode default-ignorable characters SHALL NOT require a drawable glyph.

#### Scenario: A font without an opsz axis fails loudly

- **WHEN** `LABELER_FONTS_DIR` holds an `InterVariable.ttf` with no `opsz` axis and a text item is rendered
- **THEN** the render fails with `500 Internal`

#### Scenario: Unmapped text is refused instead of shortened

- **WHEN** an active text item's resolved value contains `中`, which Inter does not map, under either overflow policy
- **THEN** rendering fails with `text_character_unmapped`, including when fitting would otherwise omit that character
- **AND** the same item with supported text renders normally when it fits
