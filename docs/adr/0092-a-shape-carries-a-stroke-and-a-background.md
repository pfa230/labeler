# 92. A shape carries a stroke and a background

Date: 2026-08-31

## Status

Accepted (§5-6 superseded by [ADR-0093](0093-one-colour-type-and-vocabulary.md)). Issue [#280](https://github.com/pfa230/labeler/issues/280). Supersedes nothing.

## Context

A container could previously be outlined but never filled, so no layout could paint a solid block.
`Frame { thickness, rounded }` was stroke-only, and the renderer emitted it with no fill.
Every element on a label carried the same visual weight, and visual hierarchy had to come from font size alone.

The narrow fix would have been adding a `fill` key to `frame`. However, the engine had two shapes (`line`
and the container's frame rect), each with its own ad-hoc spelling for stroke thickness (`line.thickness`
and `frame.thickness`) and no way to specify color. Adding a fill to one shape would leave future shapes
(standalone rects, ellipses, paths) to invent further divergent spellings.

Furthermore, issue #280 originally proposed a monochrome-only keyword vocabulary on the assumption that
labeler targets monochrome thermal and laser printers. Constraining the template vocabulary to monochrome
would make device-specific print-path decisions at the template authoring layer with less information.
Under [ADR-0033](0033-capability-aware-rendering.md), device color conversion and binarization belong in the
print driver layer.

## Decision

1. **A uniform paint vocabulary across shapes.** `stroke: { thickness, color }` describes the outline
   and is accepted on every shape item. `background: <color>` describes the interior fill and is accepted
   on shapes that enclose area (`container`); `line` encloses nothing and refuses `background` and `rounded`.
2. **`stroke` is optional on all shapes.** "No outline" is spelled by omitting `stroke`. On a `container`,
   omitting `stroke` draws no outline around the container. On a `line`, omitting `stroke` renders no outline;
   if present, `stroke` defines the line's thickness and color.
3. **`container` carries paint attributes directly.** `stroke`, `background`, and `rounded` sit directly
   on `container` rather than inside a nested `frame` block.
4. **Corner rounding is a numeric radius.** `rounded: <number>` specifies corner radius in template units,
   applied identically to the stroke and background rects, and clamped at render time to half the shorter
   side (`min(w, h) / 2`). `rounded: bool` is removed.
5. **Color representation and project-owned CSS name table.** Colors are parsed from hex strings (`#rgb`,
   `#rgba`, `#rrggbb`, `#rrggbbaa` with case-insensitive hex digits and digit-doubling for 3/4-digit forms)
   and the 16 standard CSS Level 1 named colors (`black`, `silver`, `gray`, `white`, `maroon`, `red`,
   `purple`, `fuchsia`, `green`, `lime`, `olive`, `yellow`, `navy`, `blue`, `teal`, `aqua`), matched
   case-insensitively (`Red`, `red`, `RED`).
   
   The shape color table is project-owned and deliberately uses standard CSS Level 1 sRGB primaries rather
   than delegating to the renderer's color constants. Typst's built-in named colors define custom pastel/web
   hues (e.g. Typst `red` is `#ff4136`, `yellow` is `#ffdc00`, `blue` is `#0074d9`). Using standard CSS
   definitions (`#ff0000`, `#ffff00`, `#0000ff`) ensures that shape styling remains predictable, standard,
   and stable across rendering engine upgrades.
   `stroke.color` defaults to `#000000ff` (black). Colors serialize canonically as lowercase `#rrggbbaa`.
6. **Shape paint (`Color`) vs. Text ink (`Ink`).** Shape paint and text typography serve different purposes
   and maintain distinct, intentional color models:

   | Property | Shape Paint (`background`, `stroke.color`) | Text Typography (`text.ink`, [ADR-0091](0091-text-ink-is-a-full-colour.md)) |
   | --- | --- | --- |
   | **Color Standard** | CSS Level 1 16-color table | Typst 18-color palette |
   | **`red`** | `#ff0000ff` (CSS pure red) | `#ff4136` (Typst red) |
   | **`green`** | `#008000ff` (CSS green) | `#2ecc40` (Typst green) |
   | **`gray`** | `#808080ff` (CSS mid-gray) | `#aaaaaa` (Typst light gray) |
   | **`blue`** | `#0000ffff` (CSS pure blue) | `#0074d9` (Typst blue) |
   | **Name Matching** | Case-insensitive (`Red`, `RED`, `red`) | Case-sensitive (`red` only) |
   | **Read-Back Form** | Canonical `#rrggbbaa` | Authored string preserved verbatim |
   | **Dynamic Substitution** | Fixed scalar literals | Supports `{param}` parameter references |

   Shape paint enforces geometric predictability and canonical hex normalization across templates, while
   text ink is designed for text typography, token substitution, and verbatim string preservation.
7. **Breaking removals.** `container.frame`, `line.thickness`, and `rounded: <bool>` are removed.
    Templates using these legacy spellings are refused at load and quarantined.

## Consequences

- Templates can paint solid background blocks, colored borders, rounded containers, and colored divider lines.
- The shape model is clean and ready to generalize to additional shape types without breaking syntax changes.
- Existing templates using `frame` or bare `line.thickness` must be migrated to `stroke` / `background` / `rounded`.
- Device binarization remains the responsibility of the print driver pipeline.
