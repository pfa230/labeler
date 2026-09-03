# Authoring label templates

A template is one YAML file that describes one label. This guide walks from a blank file to a working
label through worked examples, in the order the concepts actually bite.

[`SPEC.md`](SPEC.md) is the normative reference for the behavior it documents: it states every rule
precisely, organised by subsystem. This guide is organised by what you are trying to do, and links
into the spec rather than restating it. Where this guide and the normative rules disagree, this guide
is the bug.

The normative rules live in two places. `SPEC.md` is frozen at commit `bc7b1ce` (2026-08-19, ADR-0057);
behavior added or changed after that date lives in `openspec/specs/<capability>/spec.md`. To look a
rule up: read `SPEC.md`, then check whether an `openspec/specs/` requirement names and supersedes that
section. If one does, it wins for that section; otherwise `SPEC.md` holds.

Every worked example below is a template that ships in this repo, under `catalog/` or
`tests/fixtures/templates/`, so the guide cannot drift from the shipped set, and every render shown
is that template's actual output. A handful of short snippets exist only to isolate one rule; those
are labelled where they appear.

## Contents

1. [The authoring loop](#1-the-authoring-loop)
2. [Anatomy of a template](#2-anatomy-of-a-template)
3. [A fixed-size label, end to end](#3-a-fixed-size-label-end-to-end)
4. [Auto-length tape labels: how the width is decided](#4-auto-length-tape-labels-how-the-width-is-decided)
5. [Sizing vocabulary: `content` and `fill`](#5-sizing-vocabulary-content-and-fill)
6. [`max_w` and `max_h` are caps](#6-max_w-and-max_h-are-caps)
7. [Text overflow: `ellipsis` and `fail`](#7-text-overflow-ellipsis-and-fail)
8. [Edge-relative coordinates and `to:`](#8-edge-relative-coordinates-and-to)
9. [Containers and shape paint: nesting, padding, strokes, backgrounds, options, rotation](#9-containers-and-shape-paint-nesting-padding-strokes-backgrounds-options-rotation)
10. [Why some mistakes are caught at startup and others at print time](#10-why-some-mistakes-are-caught-at-startup-and-others-at-print-time)
11. [Troubleshooting](#11-troubleshooting)

---

## 1. The authoring loop

A template that parses is not a label that looks right. Render it and look at it.

A fresh config directory starts with no templates — nothing is seeded — so put the YAML there
yourself before the first render.

```bash
# Serve, with auth off so curl works without a token.
LABELER_CONFIG_DIR=./config-dev LABELER_NO_AUTH=true cargo run --bin labeler

# In another shell: install the templates these examples use, then pick them up.
mkdir -p config-dev/templates
cp catalog/tape/brother/brother_24mm.yaml catalog/sheet/avery/avery5163.yaml \
   tests/fixtures/templates/*.yaml config-dev/templates/
curl -s -X POST localhost:8080/api/templates/reload

# Single (tape) templates render straight to PNG.
curl -s -X POST 'localhost:8080/api/render/label?format=png' \
  -H 'Content-Type: application/json' \
  -d '{"template":"brother_24mm","data":{"message":"Kitchen Utensils"}}' \
  -o out.png

# Sheet templates go through /api/batch and are always PDF.
curl -s -X POST localhost:8080/api/batch \
  -H 'Content-Type: application/json' \
  -d '{"template":"avery5163","mode":"download","labels":[{"data":{"message":"Hello"}}]}' \
  -o out.pdf
```

Edit the YAML, then `curl -s -X POST localhost:8080/api/templates/reload` to pick up the change
without a restart, and render again. Templates live in `{LABELER_CONFIG_DIR}/templates/`.

Check the render against your intent, not against "it returned 200": is the text inside the printable
area, is the QR square, did auto-shrink kick in when you did not expect it, is anything clipped at an
edge. Those are the failures a 200 hides.

## 2. Anatomy of a template

```yaml
name: My Label        # shown in the UI
unit: mm              # mm | in — every coordinate and size below is in this unit
dpi: 180              # raster resolution for PNG output
params: [ ... ]       # typed inputs, defaults, bounds, and UI hints (see §9)
format: { ... }       # the physical shape of the label (below)
layout: [ ... ]       # the tree of items to draw
```

A template's identifier is determined by its filename stem (e.g. `templates/my_label.yaml` has ID `my_label`). The `id:` and `group:` keys are **not** present in the YAML file and will be rejected with a 422 error if included.

Directory structure relative to the templates root determines group membership:
- `templates/my_label.yaml` is ungrouped (`group: null`).
- `templates/Warehouse/my_label.yaml` belongs to group `Warehouse`.
- `templates/Shipping/Pallets/my_label.yaml` belongs to nested group `Shipping/Pallets`.

An invalid template is quarantined on load while other templates continue to serve. Full field table:
[SPEC §3](SPEC.md#3-template-schema).

An unknown field at the top level, or on a layout item, is rejected — so a misspelled `paddding` on a
container fails loudly. That guard does **not** reach inside the nested objects: a typo within
`format`, `alignment` or `params` is dropped rather than reported. You still hear about it if
the field it was meant to set is required — a misspelled `format.height` fails as a *missing* field —
but a misspelled optional one (`alignment.vertcal`, `params.quiet_zne`) silently leaves the default in
place. If a setting appears to have no effect, check its spelling first.

`format` comes in two shapes, and almost every rule that surprises people follows from which one you
picked.

**`sheet`** — a grid of identical, fixed-size slots on a fixed page. You get a PDF.

```yaml
format:
  type: sheet
  paper_width: 8.5
  paper_height: 11.0
  label_width: 4.0
  label_height: 2.0
  positions:            # bottom-left corner of each slot, page origin bottom-left
    - [0.17, 8.5]
    - [4.33, 8.5]
```

**`single`** — one label. Its height is always fixed; its width is either fixed, or a `{min, max}`
range, which makes it **auto-length**: continuous tape, cut to fit the content.

```yaml
format:
  type: single
  width: { min: 10.0, max: 120.0 }   # auto-length. A bare number would be fixed-width.
  height: 18.1                       # printable height, narrower than the nominal tape
  media_width: 24                    # nominal tape width; print preflight only, no effect on geometry
```

Whether `format.width` is a number or a range is the single most consequential choice in the file. It
decides what a `fill` extent stretches to (§5), whether the measure pre-pass runs (§4), and which
errors can be caught at startup rather than at print time (§10).

## 3. A fixed-size label, end to end

`catalog/sheet/avery/avery5163.yaml` is the whole thing:

```yaml
name: Avery 5163 2" x 4" Shipping Label
unit: in
dpi: 300
params:
  - name: message
    type: string
    description: "Label message text"
format:
  type: sheet
  paper_width: 8.5
  paper_height: 11.0
  label_width: 4.0
  label_height: 2.0
  positions: [[0.17, 8.5], [4.33, 8.5], ...]
layout:
  - type: container
    at: [0.0, 0.0]
    size: [4.0, 2.0]
    padding: 0.15
    items:
      - type: text
        value: "{message}"
        at: [0.0, 0.0]
        size: [3.7, 1.7]
        font_size: { min: 10.0, max: 48.0 }
        wrap: true
        alignment: { horizontal: center, vertical: center }
```

Three things to take from it.

**The origin is bottom-left and y points up.** This is the first thing that surprises people, because
most graphics APIs put the origin at the top-left. `at: [0.0, 0.0]` is the bottom-left corner of the
frame; increasing `y` moves *up*. The renderer flips into Typst's top-left space for you
([SPEC §6](SPEC.md#6-coordinate-system)).

```
 y
 ^     +-------------------------------+  (4.0, 2.0)
 |     |                               |
 |     |            message            |
 |     |                               |
 |     +-------------------------------+
 (0,0)                                    --> x
```

**The frame is the slot, not the page.** Everything in `layout` is positioned inside one 4×2 inch
slot. The engine repeats that layout into every position in `positions` and paginates. You never
write page coordinates.

**A `font_size` range means auto-shrink.** `{min: 10.0, max: 48.0}` starts at 48pt and steps down in
0.5pt increments until the text fits its box; if it still overflows at 10pt, `overflow` decides what
happens (§7), and by default the last line is ellipsized. A bare number (`font_size: 12.0`) is fixed
and never shrinks, but it is not a licence to clip: a fixed size runs the same `overflow` policy, so
it ellipsizes at the box edge and errors when even the marker does not fit.

For a fixed-size label with a QR, `tests/fixtures/templates/avery5163_asset_tag.yaml` is the canonical
example; it is covered in §9 because it also demonstrates containers, parameters, conditional visibility, and rotation.

## 4. Auto-length tape labels: how the width is decided

`catalog/tape/brother/brother_24mm.yaml`:

```yaml
name: Brother 24mm Continuous Label (text only)
unit: mm
dpi: 180
params:
  - name: message
    type: string
    description: "Label message text"
format:
  type: single
  width: { min: 10.0, max: 120.0 }
  height: 18.1
  media_width: 24
layout:
  - type: container
    at: [0.0, 0.0]
    size: [fill, 18.1]
    padding: 1.0
    items:
      - type: text
        value: "{message}"
        at: [0.0, 0.0]
        size: [fill, 16.1]
        font_size: { min: 10.0, max: 32.0 }
        wrap: false
        alignment: { horizontal: center, vertical: center }
```

![Kitchen Utensils on 24mm tape](images/authoring-tape-basic.png)

Because `format.width` is a range, the label has no width until a request arrives with data. The
engine runs a **measure pre-pass** over the layout before rendering anything: it walks the items,
computes how far right the content reaches, clamps that to `[width.min, width.max]`, and only then
renders into the resulting frame. This pre-pass is the load-bearing concept of the whole engine, and
everything in §5 through §8 is a consequence of it.

### What contributes to the measured width

| Item | Contributes |
| --- | --- |
| `text` with `size: [content, …]` | `at.x` + the width its string actually needs at the chosen font size |
| `text`/`qr`/`image`/`container` with a numeric width | `at.x + width` — a constant you wrote |
| `qr` / `image` with `size: [content, …]` | `at.x` + the item's intrinsic dimension (QR matrix with quiet zone; image viewBox / pixels) |
| `container` with `size: [content, …]` | `at.x` + padding + whatever its children measured to |
| anything with `size: [fill, …]` | exactly what the same item written `content` contributes: a `fill` extent reports its own content upward, then takes the frame |
| `line` | the larger of its two endpoints' `x`, and nothing else |
| anything anchored with an edge-relative `at.x` (§8) | only its **inset** — the narrowest label it fits on |
| a `text` or `container` whose `to.x` is edge-relative | its content, **plus** the right margin that `to.x` asks for (`to: [-2.0, …]` reserves 2 units to the right of the content) |
| a `qr` or `image` whose `to.x` is edge-relative | its intrinsic size, **plus** the right margin |

Four rows are worth dwelling on, because they are the ones that trip people up.

**`fill` and `content` contribute the same width, and differ afterwards.** Both report what the item
measured, so both decide the label the same way; the difference is what the item does once the width
exists. A `content` item keeps its own size, and a `fill` item spreads to the frame remaining from its
anchor. On a tape that is exactly the width its text asked for the two are indistinguishable, which is
why the difference only shows up at the ends of the range: when the label clamps up to `width.min`, or
`max_w` caps it, the extra width is slack a `fill` box has and a `content` box does not, and
`alignment.horizontal: center` centres in it. That is why the tape above writes `fill` and not
`content`: with a short message the label widens to `width.min: 10.0` and the text sits in the middle
of it, where a `content`-width text would sit against the left padding.

**A `line` contributes only the coordinates you wrote, never any content of its own.** A rule drawn to
a fixed `x` of 40 does hold the label to at least 40 wide — that is a number you asked for. But a rule
drawn to the right *edge* (§8) contributes nothing, so it spans whatever the text decided rather than
deciding it. That is the difference between a divider that follows the content and one that pins the
label open.

**A right-anchored item cannot define the width it is anchored to.** If `at.x` is edge-relative, its
position depends on the final width, which is what the pre-pass is trying to compute. Circular. So the
item contributes only its inset — the narrowest label on which it would still fit — and nothing more.

**Containers compute intrinsic content across rotation.** A rotated container (`rotate: 90 | 180 | 270`)
measures its subtree in author space and maps through the axis swap, contributing its author-height to
the physical width when rotated 90° or 270°.

Fixed-width `single` templates and `sheet` templates skip the pre-pass entirely. Their frame is known
before the request arrives.

## 5. Sizing vocabulary: `content` and `fill`

Every box layout item (`text`, `qr`, `image`, `container`) specifies its dimensions via `size: [width, height]`
or `to: [x, y]` (see §8).

On each axis of `size`, you write one of:
- **A numeric dimension**: e.g. `20.0` or `1.5in`. Statically fixed.
- **`content`**: Hugs the item's own intrinsic dimension.
  - On `text`: Hugs the text rendered width or height.
  - On `qr`: Hugs the generated QR matrix size (`(modules + 2 × quiet_zone) × module_size`, with `quiet_zone` defaulting to 0).
  - On `image`: Hugs the SVG physical / viewBox dimensions, or raster image pixel dimensions at target DPI.
  - On `container`: Hugs the bounding box of its active children plus padding.
- **`fill`**: Stretches to occupy the remaining space in the parent frame from the item's anchor:
  `parent_frame - resolved_anchor`. If an item is at `at: [10.0, 0.0]` in a 50mm container,
  `size: [fill, ...]` resolves to `40.0mm`; at `at: [-12.0, 0.0]` it resolves to `12.0mm` in any
  frame, because a right-anchored anchor leaves exactly its own inset. The remainder is not clamped
  at zero: an anchor that resolves outside its frame is refused (§10), not stretched to nothing.

When omitted, a `container` defaults to `size: [fill, fill]`.

> [!NOTE]
> The keyword `auto` was previously overloaded with multiple conflicting meanings and is now removed.
> Template definitions containing `auto` will be rejected at parse time. Use `content` to hug the item's
> own size, or `fill` to stretch to the frame.

## 6. `max_w` and `max_h` are caps

`max_w` and `max_h` bound the resolution of `content` and `fill` on their respective axes across validation,
measurement, and rendering ([ADR-0053](adr/0053-max-bounds-cap.md), [ADR-0080](adr/0080-unify-size-resolution.md)).

Two rules:

**They cap dynamic resolution; they never clamp a fixed number.** `size: [40.0, …]` with `max_w: 30.0` is 40 wide. If
you want 30, write 30. A `max_*` is an upper bound on a dynamically resolved value, not a constraint on an
explicit literal you wrote.

**They cap content and fill alike.** For a `text` with `size: [content, 18.1]`, `max_w: 30.0` limits how much
content-driven width the text may claim. For a `container` with `size: [fill, 18.1]`, `max_w: 30.0` caps the
stretched width to at most 30mm even if 80mm remains in the parent frame.

`tests/fixtures/templates/brother_24mm_max_w_cap.yaml`:

```yaml
params:
  - name: code
    type: string
  - name: message
    type: string
format: { type: single, width: { min: 10.0, max: 150.0 }, height: 18.1 }
layout:
  - type: qr
    value: "{code}"
    at: [1.0, 0.0]
    size: [18.1, 18.1]
  - type: text
    value: "{message}"
    at: [21.1, 0.0]
    size: [content, 18.1]
    max_w: 30.0
    font_size: { min: 8.0, max: 32.0 }
    alignment: { horizontal: left, vertical: center }
```

![QR and a capped, ellipsized message](images/authoring-max-w-cap.png)

The message is "Label maker on the third shelf", which would want far more than 30mm. The cap bounds
the box the text is laid out against, so it ellipsizes to "Label maker on the t..." at its `min` of
8pt instead of running the tape out to `width.max`. The cap does not then become the width: a
`content` extent reports its content, and the shortened line measures 29.6857mm, so the label comes
out at 50.7857mm (21.1 + 29.6857) rather than at 51.1mm. A cap is an upper bound on what the content
may claim, never a width in its own right.

`max_w` and `max_h` also apply beside stretching `to:` extents (§8) to cap frame-dependent spans against large frames; on authored shrinking `to` extents, caps are inert.

## 7. Text overflow: `ellipsis` and `fail`

Every `text` is laid out against its box: it breaks into lines, a `font_size` range shrinks until the
block fits, and whatever is still too big is `overflow`'s business. The field is optional and takes
two values, which shorten the same things and differ only in when they give up:

| | `ellipsis` (default) | `fail` |
| --- | --- | --- |
| fits as authored | render it | render it |
| fits once shortened | render the shortened form | `text_does_not_fit` |
| cannot fit however short | `text_does_not_fit` | `text_does_not_fit` |

Shortening keeps the lines that fit and appends `...` to the last, trimming characters until it fits.
The marker alone is the shortest form there is, so `ellipsis` reaches the bottom row in exactly two
cases: a box narrower than `...` itself, and a box shorter than one line at the chosen font size.
Neither produces a half-drawn glyph — clipping is never an outcome of the policy, it is an error.

`overflow` applies to every `font_size` spelling. A fixed size has nothing to shrink, so it reaches
the policy sooner than a range does, but it runs the same one.

The policy is judged on the metric model — the cap-height-to-baseline line box — not on glyph
outlines, so it does not see ink that leaves a box the metrics say it fits in. The two standing cases
are in §11.

```yaml
- type: text
  value: "{critical_asset_id}"
  at: [5.0, 5.0]
  size: [50.0, 10.0]
  font_size: 10.0
  overflow: fail        # guarantees no partial asset IDs are printed
```

Use `overflow: fail` on barcodes, regulatory labels, asset identifiers, and shipping tags where truncated
content could lead to scanning failures or data loss.

### Text wrapping (`wrap`) and hard breaks

Text layout items support an optional `wrap: bool` flag (default: `false`):
- **Hard newlines always survive:** Any newline (`\n`) in input data creates a new line box, whether `wrap` is `true` or `false`.
- **Soft wrapping (`wrap: true`):** When a line exceeds the width of its box, it is softly wrapped to subsequent lines at word boundaries (or character boundaries for words wider than the box).
- **No soft wrapping (`wrap: false`):** Lines are not broken beyond authored newlines. If an individual line exceeds the box width, `overflow` handles it.
- **Form inputs in the web UI:** Whether the print form displays a multi-line `<textarea>` or a single-line `<input>` is controlled entirely by declaring `multiline: true` on the parameter in `params:`. Every field referenced by the layout must be declared in `params:`, and a `wrap: true` layout item reading a `multiline: false` parameter keeps a single-line `<input>`.

### Line spacing (`line_spacing`)

Text layout items support an optional `line_spacing: float` multiplier (default: `1.2`):
- **Baseline-to-baseline pitch:** `pitch = line_spacing * font_size`. The spacing between consecutive lines is directly proportional to the font size.
- **Tighter or looser lines:** Values below `1.2` (such as `0.99` or `1.0`) produce tighter line spacing for dense badges or multi-line descriptions, allowing auto-shrink to settle at larger font sizes within a height-constrained box. Values above `1.2` (such as `1.4` or `1.5`) create more open leading.
- **Single-line invariant:** For single-line text, `line_spacing` has no effect on block height or rendering.
- **Validation:** Must be a finite positive number (`> 0`). Unitless multiplier.

```yaml
- type: text
  value: "{description}"
  at: [0.0, 0.0]
  size: [60.0, 20.0]
  font_size: { min: 8.0, max: 14.0 }
  wrap: true
  line_spacing: 1.0     # tighter 1.0x line pitch for multi-line description
```

## 8. Edge-relative coordinates and `to:`

### Edge-relative coordinates

A coordinate component that is **sign-negative** is measured inward from the frame's far edge instead
of outward from its origin ([ADR-0051](adr/0051-edge-relative-and-corner-placement.md)):

| Component | Sign | Measured from |
| --- | --- | --- |
| `x` | non-negative | left edge |
| `x` | sign-negative | **right** edge: `frame_width + x` |
| `y` | non-negative | bottom edge |
| `y` | sign-negative | **top** edge: `frame_height + y` |

`-0.0` (or `-0` in YAML) is the far edge exactly; `-2.0` is 2 units inside it. The test is the *sign
bit*, not `< 0`, precisely so that `-0.0` can mean "the far edge" — in floating point, `-0.0 < 0.0` is
false.

Edge-relative components apply to `at` and to `to`, never to `size`, `max_w`, `max_h`, `padding` or
`thickness`, and they resolve against the **current** frame: the label (for a sheet template, one
slot — never the page), a container's padded inner box, or a rotated container's swapped canvas.

The motivating case is anything that should span the full width of a label whose width you do not
know. `tests/fixtures/templates/brother_24mm_lines_divider.yaml`:

```yaml
params:
  - name: line1
    type: string
  - name: line2
    type: string
layout:
  - type: container
    at: [0.0, 0.0]
    size: [fill, 18.1]
    padding: 1.0
    items:
      - type: text
        value: "{line1}"
        at: [0.0, 8.6]
        to: [-0.0, 16.1]        # spans to the right edge of the padded inner box
        font_size: { min: 8.0, max: 20.0 }
        alignment: { horizontal: center, vertical: center }
      - type: line
        at: [0.0, 8.05]
        to: [-0.0, 8.05]        # full-width rule
        stroke:
          thickness: 0.2
      - type: text
        value: "{line2}"
        at: [0.0, 0.0]
        to: [-0.0, 7.5]
        font_size: { min: 6.0, max: 14.0 }
        alignment: { horizontal: center, vertical: center }
```

![Two centered lines separated by a full-width rule](images/authoring-divider.png)

Both text boxes span the full width, so each centers independently, and the rule spans whatever width
the longer line settled on. Nothing here names a width.

### `to:` instead of `size:`

Every box item takes exactly one of `size` or `to`. `to` names the opposite (top-right) corner, and
the size falls out: `size = to - at`, after both corners resolve.

Reach for `to` when the corner is the thing you actually know. `to: [-0.0, 16.1]` above says "span to
the right edge, top out at 16.1" — with `size` you would have to write the width, which is the number
you do not have. Reach for `size` when the dimension is the thing you know: a 1.3-inch QR is a
`size`, not a corner.

Rules:

- `size` and `to` are mutually exclusive, and exactly one is required. A `container` that gives
  neither defaults to `size: [fill, fill]`.
- `max_w` / `max_h` are allowed alongside `to`: they cap stretching `to` extents, and are inert on authored shrinking `to` extents.
- `to` must resolve above and to the right of `at`. A corner that is already inverted against the
  widest frame the label could have is rejected at load (`template_validation_failed`); one that only
  inverts against the width a particular request resolved to is `edge_rect_inverted` at render.
- A **zero**-width box is legal at render time (an empty data value can measure to exactly the item's
  own `at.x`); a **negative** one is not.
- `line` uses `at` and `to` as its two endpoints, not as a box; it has no `size`, `fit`, or rotation.
  The endpoints must differ after resolution.

## 9. Containers and shape paint: nesting, padding, strokes, backgrounds, options, rotation

A `container` groups items and establishes a new coordinate frame. Its children are positioned
relative to its **padded inner box**, so `at: [0, 0]` inside a container with `padding: 1.0` is 1 unit
in from the container's own bottom-left corner. Sizes and edge-relative coordinates inside resolve
against that inner box too.

Shapes in the template model (`container` and `line`) support paint attributes:
- **`shape: rect | ellipse | circle`**: Accepted only on `container`, defaults to `rect`.
  `ellipse` fills the container's resolved box; `circle` is the same ellipse refused unless that
  box is square (within `1e-4`). `rounded` is refused on `ellipse` and `circle`.
- **`stroke: { thickness, color }`**: Accepted on any shape (`container` outline or `line`).
  `thickness` is required and must be at least 0.0001. `color` is optional and defaults to
  `black`. `stroke` itself is optional; on a `line`, omitting it draws nothing and is not an error.
  On a `rect` container the stroke's inner edge (half the thickness inside the box) clips children,
  so child ink reaching the border is cut inside the stroke; on `ellipse`/`circle` children clip to
  the rectangular box and may paint over the curve.
- **`background: <color>`**: Fills the shape's interior. Accepted only on shapes that enclose an
  area (`container`).
- **`rounded: <radius>`**: Rounds the corners of the `rect` container's stroke and background and
  clips children to the rounded rectangle. Numeric radius in template units (e.g. `1.5` or `0.05`),
  which must be at least 0.0001 and is clamped at render time to half the shorter side. Square
  corners are spelled by omitting the key; `rounded: 0` is refused. `rounded` is refused on
  `ellipse` and `circle`, which have no corners and clip children to the rectangular box.
- **Colors**: Specified on `stroke.color`, `background`, and `text.color` alike using one unified
  vocabulary (surrounding whitespace is ignored): 3-, 4-, 6-, or 8-digit hex strings (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`), one of the 16
  standard CSS Level 1 named colors (`black`, `silver`, `gray`, `white`, `maroon`, `red`, `purple`,
  `fuchsia`, `green`, `lime`, `olive`, `yellow`, `navy`, `blue`, `teal`, `aqua`) matched case-insensitively,
  or a parameter reference `"{param}"` to a `string` or `enum` parameter. `text` items accept `color` to
  paint glyphs (omitted or `null` defaults to black); the legacy `ink` field is removed, and any template
  declaring `ink` is quarantined as broken at startup.

```yaml
- type: container
  at: [0.0, 0.0]
  size: [4.0, 2.0]
  padding: 0.15              # uniform; or [top, right, bottom, left]
  stroke:                    # optional outline
    thickness: 0.02
    color: '#000000'         # optional hex or named color (defaults to black)
  background: '#f0f0f0'      # optional fill color
  rounded: 0.1               # optional corner radius in template units
  items: [ ... ]
```

### Conditional visibility (`when:`) and parameters

Declare the parameters a template supports in `params:`, then gate containers (or any layout items)
using `when:` conditions. This is how one template serves several layouts or optional elements (ADR-0055).

```yaml
params:
  - name: orientation
    type: enum
    values: [horizontal, vertical]
    default: horizontal
  - name: outline
    type: enum
    values: [yes]
```

When a request arrives, each item carrying a `when:` map renders only if all conditions match the
resolved parameter values. `tests/fixtures/templates/avery5163_asset_tag.yaml` uses three top-level
containers: an outline-only one that draws a border, a `horizontal` one, and a `vertical` one:

```yaml
layout:
  - type: container
    when:
      outline: yes
    at: [0.0, 0.0]
    size: [4.0, 2.0]
    stroke:
      thickness: 0.02
    items: []
  - type: container
    when:
      orientation: horizontal
    at: [0.0, 0.0]
    size: [4.0, 2.0]
    items:
      - type: qr
        value: "{url}"
        at: [0.1, 0.45]
        size: [1.3, 1.3]
        params: { quiet_zone: 0.0 }
      - type: line
        at: [1.5, 0.0]
        to: [1.5, 2.0]
        stroke:
          thickness: 0.01
      # ... id under the QR, name/tags/description in the right column
```

An unmatched gate removes the whole subtree — it is not rendered, and it is not measured either.
Missing-field validation is lazy: only the active branch requires its parameter fields, so an inactive
branch's `{tokens}` may be absent from the request without causing a `422 MissingField`.

No parameter type carries an implicit default. If a parameter declares no `default:`, it is absent unless
supplied in the request. If an active layout item references an omitted parameter with no declared default,
the request fails with `422 MissingField`. Unreferenced parameters in inactive branches do not fail.

### Datetime parameters, system clock, and formatting

A template can access the render clock via `{sys.now}` or declare parameters of `type: datetime` with an optional `time: true` boolean modifier:

```yaml
params:
  - name: printed_on
    type: datetime
    default: "{sys.now}"
    description: "Print Date"
  - name: expiry_timestamp
    type: datetime
    time: true
    description: "Expiration Timestamp"
```

What to know:
- **System render clock.** Use `{sys.now}` to output the render instant as an ISO 8601 date (`%Y-%m-%d`), or `{sys.now:<format_name>}` to format it using a named strftime pattern configured in the `datetime_formats` app setting (e.g. `{sys.now:short_date}`, `{sys.now:iso_date}`).
- **Datetime parameters.** Declare a parameter of `type: datetime` when the caller must be able to choose or override the instant (e.g. for reprinting).
  - Bare `{param_name}` outputs the ISO 8601 date (`%Y-%m-%d`).
  - `{param_name:<format_name>}` formats the date/time using the named pattern from `datetime_formats` (e.g. `{printed_on:short_date}`, `{expiry_timestamp:time}`).
  - **Defaults.** Datetime parameters support explicit string defaults, such as `default: "{sys.now}"` to default to the render date (which resolves to local midnight on that date), a token with a time pattern like `default: "{sys.now:iso_timestamp}"`, or a literal ISO 8601 date/time string. If no default is declared, the parameter is required and omitting it returns `422 MissingField`. Note that bare `{sys.now}` renders `%Y-%m-%d`, so defaulting a `time: true` parameter with bare `{sys.now}` resolves to `00:00` local time rather than the wall clock; to include time in the default, use a pattern that formats the time.
- **Format syntax and restrictions.**
  - A format is attached with a colon (`:`), never a dot. Attaching a format to a value that is neither `sys.now` nor a declared `type: datetime` parameter (e.g. `{title:short_date}`, `{vars.qr_base_url:long_date}`) is a load-time rejection.
  - `{datetime}` is an ordinary parameter name, not a reserved word; like every bare token, it must be declared in `params:` to be read by the layout.
  - The old dotted spellings `{datetime.<name>}` and `{sys.now.<name>}` are load-time rejections that point to the replacement `{sys.now:<name>}`.
- **UI controls.** The web UI renders a date picker (`<input type="date">`) when `time: false` (or omitted), and a date-and-time picker (`<input type="datetime-local">`) when `time: true`. The control is seeded from `default` if declared, and left empty otherwise.
- **Overrides in batches and CSV imports.** Requests can provide ISO date strings (`YYYY-MM-DD`, `YYYY-MM-DDTHH:MM`, `YYYY-MM-DDTHH:MM:SS`, or RFC 3339 timestamps with timezone offsets) to override specific labels.
- **`time:` picks the control, not the output.** Nothing stops a template from declaring `time: false` and then printing `{printed_on:time}`; the date picker has no time to give, so that prints `00:00`. Pair a token that shows a time with `time: true`, and check the result the way you check any other template: render it and look.
- **Which one to reach for.** Use `{sys.now}` / `{sys.now:<name>}` when the label should always say when it was printed and the caller has no say in it. Declare a `datetime` parameter when the caller must be able to choose the instant. Both use the same `datetime_formats` patterns.

### Flow layout (`flow: { direction, gap, wrap, line_gap, overflow }`)

A `container` may declare a `flow` block to pack its children sequentially in order rather than positioning each child with absolute `at` or `to` coordinates ([ADR-0083](adr/0083-packed-children-flow-layout.md)):

```yaml
- type: container
  at: [0.0, 0.0]
  size: [fill, 18.1]
  padding: 1.0
  flow:
    direction: row             # required: row | column
    gap: 2.0                   # optional: space between adjacent children (default 0)
  items:
    - type: qr
      value: "{code}"
      size: [14.0, 14.0]
    - type: text
      value: "{title}"
      size: [content, 14.0]
      font_size: { min: 8.0, max: 14.0 }
```

What to know:

- **`direction: row` vs `direction: column`.**
  - `direction: row` makes the horizontal axis primary: packs along `+x` from the padded inner box's left edge, aligning items to the top edge.
  - `direction: column` makes the vertical axis primary: packs along `−y` from the padded inner box's top edge downward (top-to-bottom), aligning items to the left edge.
- **Packed children carry no coordinates.** Direct children of a flow container are anchorless:
  - They **must not** specify `at` or `to` (rejected at load time).
  - A `line` item cannot be a packed child (rejected at load time).
  - They can specify `size` (`content`, `fill`, or numeric constants), `max_w`, `max_h`, `when`, and container properties (`padding`, `shape`, `stroke`, `background`, `rounded`, nested `flow`).
- **Gaps appear only between occupying children.** Gaps are placed between active children with positive primary extent. Gated-off children (`when`) leave no hole. Active zero-extent children (e.g. empty strings) advance nothing and add no extra gap.
- **The two `fill` outcomes on packed children:**
  - **Alone in a container:** An uncapped `size: [fill, ...]` child stretches to the container's padded inner extent.
  - **Beside a sibling:** An uncapped `fill` child claims the entire inner extent, advancing the cursor past the container frame and failing at render with `item_out_of_frame`. When pairing `fill` with siblings, cap it using `max_w` (in a row) or `max_h` (in a column).
- **Wrapping `content`-sized text in a row:** A text child with `wrap: true` and `size: [content, ...]` measures its wrapped line box against the container's full padded inner width, so its content width equals the inner width. Consequently, it can only be packed first or alone; placed after a sibling (e.g. following a QR code), its width plus the preceding sibling's width overruns the container and fails with `item_out_of_frame`. To place multiline text beside a sibling, give it an explicit numeric width (or cap it).
- **Nested flow containers.** A flow container inside another flow container packs its assembled extent (sum of child sizes + gaps) into the parent flow.

To continue a row on another line, give the container a resolved width and enable `wrap`. This short
snippet isolates wrapping; the first two 14mm boxes fill the first line exactly, and the third starts
6mm lower plus the 1mm `line_gap`:

```yaml
- type: container
  at: [0, 0]
  size: [30, 20]
  flow:
    direction: row
    gap: 2
    wrap: true
    line_gap: 1
    overflow: fail
  items:
    - { type: text, value: "A", size: [14, 6], font_size: 8 }
    - { type: text, value: "B", size: [14, 6], font_size: 8 }
    - { type: text, value: "C", size: [14, 6], font_size: 8 }
```

Line breaks are decided from the child boxes, not their reported content requirements. Each line
advances by its tallest drawn box, and `line_gap` appears only between lines. A row needs a resolved
container width to wrap; a column needs a resolved height. `overflow: fail` keeps the default
`item_out_of_frame` error if the stack of lines does not fit. `overflow: trim` instead omits the first
child that does not fit and every child after it, without marking or reporting the trim. Because that
removal must not resize the container, `trim` requires both container axes to be resolved.

### Rotation

A container may set `rotate` to turn a portrait design onto a landscape slot: the "read by turning the
label" layout ([ADR-0036](adr/0036-container-rotation.md)). The `vertical` branch of the asset tag is
the same information authored in a 2×4 portrait canvas and rotated onto the 4×2 slot:

```yaml
  - type: container
    when:
      orientation: vertical
    at: [0.0, 0.0]
    size: [4.0, 2.0]
    rotate: 90
    items:
      - type: qr
        value: "{url}"
        at: [0.2, 2.3]        # authored in the swapped [2, 4] canvas
        size: [1.6, 1.6]
      # ...
```

![The horizontal and vertical branches side by side](images/authoring-asset-tag.png)

What to know:

- **Container-only and orthogonal.** `rotate` on any other item type, or a value that is not a
  multiple of 90, is a validation error. Degrees are counter-clockwise.
- **The footprint stays in parent coordinates.** `at` and the extent describe the container's box in
  its parent exactly as if it were not rotated. Rotation is purely an inner transform.
- **The inner canvas swaps for 90 and 270.** Author children against `[inner_h, inner_w]`. Padding is
  author-space and rotates with the design; the drawn `stroke` and `background` do not rotate.
- **Sizing under rotation.** Rotated containers and their descendants support `content` and `fill` as
  long as required parent axes are resolved. When axes swap (90° and 270°), child width resolves against
  inner container height and child height against inner container width. Flow containers beneath rotation
  pack in author space.

## 10. Why some mistakes are caught at startup and others at print time

Templates are validated when the server loads them, and a file that fails is quarantined and
reported rather than served. So a
`422` from a template that loaded fine looks like a contradiction. It is not.

The split is about **geometry**. Data-driven failures — a missing field, an undecodable image, an
interpolation token that resolves to nothing — are per-request on every format, because the data
arrives with the request. What differs between formats is when the *layout* can be checked.

On a fixed-width label every coordinate is a constant once the file parses, so load-time validation of
the geometry is exact: anything that can be wrong about the layout is wrong then.

On an **auto-length** label the frame width is not known until the measure pre-pass runs, which needs
the request's data. Load-time validation therefore checks the widest case: it bounds coordinates
against `format.width.max`, so a template that could never fit is still rejected at startup. The
render path then re-checks against the width this particular request actually resolved to
([ADR-0051](adr/0051-edge-relative-and-corner-placement.md) §7).

The practical consequence:

| Decidable at load | Only decidable per request |
| --- | --- |
| An absolute coordinate past `width.max` | A coordinate that fits `width.max` but not *this* label's width |
| An extent that shrinks as the label grows, written on an axis whose width this label does not fix | Whether the content measured wide enough to leave room |
| A numeric `size` component that is `<= 0`, an invalid `fill` or `content` specification | A `to`-spanning box that collapsed because its data was empty |
| An inverted `to` against the `width.max` frame | A `fill` or shrinking `to` extent whose anchor left no room for this data |

So "it loaded fine but 422s on one label" is never a mystery: either the data is missing or unusable,
or, on an auto-length label, it produced a width the layout cannot live in. Either way the failing
*data* is what to look at, not the template.

## 11. Troubleshooting

Four error codes carry a stable `details.reason` slug naming the specific cause; match on the slug,
never on the message text ([SPEC §10.1](SPEC.md#101-detailsreason)). Everything else is identified by
`code` alone. Every row below is a slug except `MissingField` and `InvalidEnumValue`, which are codes and carry no slug.

| Reason / Code | What actually happened | Usual fix |
| --- | --- | --- |
| `intrinsic_size_undefined` | An `image` was asked for its own size (`content` or `fill`) and its dimension metadata gives no extent on that axis, or cannot be parsed. A `qr` without `module_size` never reaches here: that demand is visible in the file, so it is refused at load as `template_validation_failed`. | Give the SVG an absolute `width`/`height` or a `viewBox`, check the image data and its MIME type, or write a numeric `size`. |
| `text_does_not_fit` | Under `overflow: fail`, the text overflowed its box. Under `overflow: ellipsis`, it still overflowed at the shortest form there is: the box is narrower than `...`, or shorter than one line at the chosen font size (§7). | Widen or heighten the box, lower `font_size.min`, or — under `fail` only — switch to `ellipsis`. |
| `max_size_invalid` | The binding `max_*` is not `> 0`. | It is your `max_*` at fault here — check the number. |
| `size_invalid` | An authored `size` component resolves to `<= 0`. | Check for a `0` or a negative value in `size` or its parameter. |
| `edge_rect_inverted` | `to` or a shrinking `to` extent did not resolve above and to the right of `at` — for *this* request's frame. A corner already inverted against `width.max` is caught at load as `template_validation_failed` instead. | Check signs: `to: [-0.0, …]` is the right edge, `to: [0.0, …]` is the left one. |
| `coord_out_of_frame` / `item_out_of_frame` | A resolved coordinate or box falls outside the frame. On an auto-length label this can be per-request; see §10. | Remember children resolve against the container's **padded inner** box, not its outer size. |
| `line_endpoint_out_of_frame` | A line endpoint resolved outside the frame. Endpoints are errors, not clipped. | Same as above; check the container frame you are actually in. |
| `dimension_exceeds_limit` | A resolved label dimension exceeds the `max_label_dimension_mm` application setting (default 1000mm) or is `<= 0`. | Check requested dimension parameters or update the setting. |
| `MissingField` (422) | A `{token}` or data-bound image `name` has no value in the request `data` and no declared `default` in `params:`. | Check parameter spelling. An inactive `when:` branch never demands its unreferenced parameters (§9). |
| `InvalidEnumValue` (422) | An `enum` parameter was supplied with a value not in its declared `values`. | Check allowed enum values declared in the template's `params:`. |
| `template_validation_failed` (at startup) | Structural validation. The message carries the JSON path to the offending item. | Read the path; it names the exact item. |

Two visual failures that return `200` and still need fixing:

**Text is smaller than expected.** A `font_size` range shrank it to fit. The box is too small, or on a
`top`/`bottom`-aligned item the ink reservation ate the room: those alignments inset the block by the
font's overflow at that edge so descenders and accents cannot clip, which costs height
([ADR-0050](adr/0050-ink-reservation-at-slot-edges.md)). `center` is not inset, and can still clip in
a slot shorter than about `1.21 × font_size`.

**Lowercase text sits lower in its slot than all-caps.** Vertical alignment positions a fixed metric
box running cap-height to baseline, so the space above the baseline is reserved whether or not the
string uses it. This is inherent to baseline alignment and is what every other renderer produces
([ADR-0045](adr/0045-vertical-text-alignment.md)).

---

## Where to go next

- [`SPEC.md`](SPEC.md) — the normative reference for every field, rule, and error code.
- [`adr/`](adr/) — why each decision up to 2026-08-31 is the way it is; frozen, with newer reasoning
  in the change that made it.
- `catalog/` — the shipped starter templates, the best base to copy from.
- `tests/fixtures/templates/` — templates that exist to demonstrate engine features (QR layouts,
  text wrapping, sheet options, rotation, edge-relative placement, interpolation).
