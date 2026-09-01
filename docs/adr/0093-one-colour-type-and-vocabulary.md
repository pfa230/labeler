# 93. One colour type and vocabulary across text and shapes

Date: 2026-08-31

## Status

Accepted. Issue [#291](https://github.com/pfa230/labeler/issues/291). Supersedes [ADR-0091](0091-text-ink-is-a-full-colour.md)'s vocabulary and naming clauses and [ADR-0092](0092-a-shape-carries-a-stroke-and-a-background.md) §5-6.

## Context

Labeler recently introduced color capabilities across two separate efforts:
1. [ADR-0091](0091-text-ink-is-a-full-colour.md) added foreground text coloring as `text.ink`, using an `Ink` type backed by Typst's 18-color palette (case-sensitive, supporting `{param}` dynamic references and preserving authored strings).
2. [ADR-0092](0092-a-shape-carries-a-stroke-and-a-background.md) added shape outline and background coloring as `stroke.color` and `background`, using a `Color` type backed by a 16-color CSS Level 1 table (case-insensitive, canonical `#rrggbbaa` normalization, literals only).

This produced two divergent color models within the engine:
- `red` on `text.ink` rendered Typst's `#ff4136`, while `red` on `container.background` rendered CSS pure red `#ff0000`.
- Text supported parameter references (`{brand}`) while shape strokes and backgrounds rejected them.
- Text used the field name `ink`, while shapes used `color` (`stroke.color`, `background: <color>`).
- In addition, "ink" already has an established typographic meaning in Labeler ([ADR-0043](0043-ink-based-vertical-alignment.md), [ADR-0050](0050-ink-reservation-at-slot-edges.md), [ADR-0084](0084-centred-text-reserves-its-ink.md)) referring to physical glyph ink bounds and edge reservations.

Having two parsers, two color tables, two wire models, and divergent naming created unnecessary complexity and authoring confusion.

## Decision

1. **A single domain type `Color`.** Replace `Ink` and the previous `Color` struct with a unified `Color` struct holding private fields `{ spelling: String, rgba: [u8; 4] }` accessed via `spelling()` and `rgba()`.
2. **One 16-color CSS Level 1 table.** Colors are parsed from the 16 standard CSS Level 1 names (`black`, `silver`, `gray`, `white`, `maroon`, `red`, `purple`, `fuchsia`, `green`, `lime`, `olive`, `yellow`, `navy`, `blue`, `teal`, `aqua`), matched case-insensitively, and `#`-prefixed hex strings in 3, 4, 6, or 8 hexadecimal digits (`#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`) with digit-doubling for short forms. Non-CSS names (such as `eastern` and `orange`), unquoted/unprefixed hex, and malformed strings are refused.
3. **The text field is `color`.** Rename `LayoutItem::Text.ink` to `color`. `ink:` is no longer accepted; templates using `ink` are quarantined as broken at startup and refused at the write endpoint.
4. **Dynamic `{param}` references across all color fields.** `text.color`, `container.background`, and `stroke.color` (on `line` and `container`) all accept `DynamicValue<Color>`, allowing `{param}` references to `string` and `enum` parameters. Referenced parameters are derived in the template input list as not interpolated.
5. **Authored spelling preservation on read-back.** Serialization returns the authored string verbatim (e.g. `red` -> `"red"`, `#F0F` -> `"#F0F"`, `{brand}` -> `"{brand}"`). Default `stroke.color` reports `"black"`, while absent `text.color` and `background` omit the key.
6. **Standard sRGB alignment.** Text color values use standard CSS Level 1 sRGB primaries, ensuring identical paint values across text glyphs, container backgrounds, and strokes.
7. **Failure reason `color_param_invalid`.** When a referenced parameter value cannot be parsed as a color at render time, single renders return `400 InvalidRequest` / `color_param_invalid`, and batches return `422 BatchInvalid` with failure reason `color_param_invalid`.

## Consequences

- Authors have a single, unified color vocabulary and syntax across all template elements.
- Identical color literals (`red`, `blue`, etc.) produce identical pixel outputs whether applied to text, lines, or container fills.
- Parameter-driven theming and branding work identically for shapes and text.
- **Breaking template change**: Templates using `ink:` on text items must be updated to `color:`. `ink:` is refused without alias or warning; unmigrated templates fail validation and are quarantined at startup with an unknown-field error naming `layout[i]` and `ink`.
- **Silent value shift for text colours**: Named colours on text items now denote standard CSS Level 1 values rather than Typst's typography constants (`red` shifts from `#ff4136` to `#ff0000`, `green` from `#2ecc40` to `#008000`, `yellow` from `#ffdc00` to `#ffff00`, `gray` from `#aaaaaa` to `#808080`). The Typst-only names `orange` and `eastern` are no longer accepted on text items and must be replaced by hex strings.
- **Read-back change for API clients**: `GET /templates/{id}` returns the authored spelling verbatim (e.g. `"#F0F"`, `"{brand}"`) rather than normalizing shapes to lowercase canonical `#rrggbbaa`. Omitted `stroke.color` materializes as `"black"`, while omitted `text.color` and `background` remain omitted.
