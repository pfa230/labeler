# Layout

## Purpose

Defines the layout tree of a template: the item types other than text internals (`qr`, `image`, `line`, `container`), placement and coordinates, how every box is sized, containers, flow packing, `repeat`, `when:` gating, rotation, shapes and paint, and the colour vocabulary.

## Requirements

### Requirement: The layout is an ordered tree of typed items

`layout` SHALL be an ordered list of items, each tagged by `type`: `text`, `qr`, `image`, `line` or `container`. A `container` nests further items under `items` (required, may be empty). Items SHALL be drawn back to front in list order, so a later sibling draws on top of an earlier one. The `text` item's own keys and fitting belong to `text`; everything here applies to it as to any boxed item.

#### Scenario: Later items draw on top

- **WHEN** a `container` with `background: black` is followed in the same list by a `text` overlapping it
- **THEN** the text is drawn over the black ground

### Requirement: Placement keys

Every boxed item (`text`, `qr`, `image`, `container`) SHALL accept these placement keys:

| Key | Type | Default | Rule |
| --- | --- | --- | --- |
| `at` | `[x, y]` | `[0, 0]` | Lower-left anchor in template units; components may be edge-relative. Refused on a packed child. |
| `size` | `[w, h]` | none | Each component a number, a `"{param}"` reference to a `length`, `number` or `integer` parameter, `content`, or `fill`. Any other value, including `auto`, is refused at load. |
| `to` | `[x, y]` | none | Opposite (upper-right) corner; extent is `to − at` after both resolve. Refused on a packed child. |
| `max_w`, `max_h` | number | none | Cap on a content or frame extent on that axis; must be greater than 0. |
| `rotate` | number (degrees) | none | Container only; see rotation. |
| `when` | map | none | Gate; see conditional visibility. |

`size` and `to` SHALL be mutually exclusive. A `text`, `qr` or `image` MUST set exactly one of them; a `container` that sets neither SHALL behave as `size: [fill, fill]`. A non-container item carrying `rotate` SHALL be refused at load.

#### Scenario: Both size and to are refused

- **WHEN** an item declares both `size: [10, 5]` and `to: [20, 10]`
- **THEN** the template is refused at load, naming the item

#### Scenario: A container with no extent fills its parent

- **WHEN** a `container` declares neither `size` nor `to`
- **THEN** it resolves exactly as `size: [fill, fill]`

### Requirement: Coordinates are bottom-left, y-up, and may be edge-relative

All coordinates SHALL use a bottom-left origin with y pointing up, in the template `unit`. A coordinate component whose sign bit is set SHALL be **edge-relative**, measured inward from the far edge: `x` resolves to `frame_width + x` and `y` to `frame_height + y`, so `-0.0` is the far edge exactly and `-2.0` is 2 units inside it. Edge-relative components SHALL apply to `at` and `to` (box and line) only, never to `size`, `max_w`, `max_h`, `padding` or `stroke.thickness`, and SHALL resolve against the current frame: the label, a container's padded inner box, or a rotated container's author canvas.

At render, an `at` or line endpoint resolving below or left of the frame's origin SHALL fail with `UnsupportedLayoutItem` reason `coord_out_of_frame`; a box reaching past the frame's far edge, or whose anchor lies beyond it, SHALL fail with `item_out_of_frame`; a line endpoint past the far edge SHALL fail with `line_endpoint_out_of_frame`. Comparisons SHALL use a tolerance of 0.0001 template units. The same placements are refused at load when the load-time frame already shows them.

#### Scenario: A right-anchored item

- **WHEN** an item on a label 100 wide declares `at: [-20.0, 0]` with `size: [20, 10]`
- **THEN** its box spans x = 80 to 100

#### Scenario: Negative zero is the far edge

- **WHEN** a `line` declares `at: [0, 5]` and `to: [-0.0, 5]` on a label 50 wide
- **THEN** it runs the full width, ending at x = 50

### Requirement: An extent comes from the author, the content, or the frame

Every extent on each axis SHALL take its behavior from its **source**:

| Source | Spellings | Extent |
| --- | --- | --- |
| author | a number; a `"{param}"` reference; a `to` whose corners are both non-negative or both edge-relative | as written |
| content | `content` | the item's intrinsic size |
| frame | `fill`; a `to` with a non-negative `at` and an edge-relative `to` | the available extent |
| author, conditional | a `to` with an edge-relative `at` and a non-negative `to` (a **shrinking `to`**) | `to − at − F`, only on a resolved axis |

The **available extent** on an axis SHALL be `frame − resolve(at) − inset`, where `inset` is the magnitude of an edge-relative `to` or zero. An item with no anchor (a packed child) SHALL have the whole frame extent available.

1. An authored extent SHALL be checked: one that does not fit its frame is refused (at load when written in the template; at render with `item_out_of_frame` when it comes from a request value). A content or frame extent SHALL be `min(source value, max_w/max_h, available)` and so never overflows.
2. `max_w`/`max_h` SHALL bind content and frame extents and SHALL be inert on authored ones. Pairing a cap with `to` SHALL NOT be an error.
3. Only content and frame extents SHALL demand an intrinsic size.
4. A content or frame extent of exactly zero SHALL render an empty box. An authored extent of zero or less written in the template SHALL be refused at load; one supplied through a parameter SHALL fail at render with `size_invalid`. A `to` that inverts SHALL fail with `edge_rect_inverted` (refused at load when it already inverts against the load-time frame), taking priority over `size_invalid`.

A frame extent SHALL report `min(intrinsic, cap, available)` to its parent while taking the available extent as its box. The two axes are independent.

#### Scenario: A cap binds a chosen size and not a written one

- **WHEN** one item declares `size: [content, 10], max_w: 30` with an intrinsic width of 45, and another declares `size: [40, 10], max_w: 30`
- **THEN** the first resolves to 30 and the second to 40

#### Scenario: A cap binds a stretching to and not a constant one

- **WHEN** in a frame 50 wide one item declares `at: [0, 0], to: [-0.0, 10], max_w: 30` and another `at: [0, 0], to: [40.0, 10], max_w: 30`
- **THEN** the first resolves to 30 and the second to 40, and neither is refused

#### Scenario: A right-anchored to is a constant

- **WHEN** an item declares `at: [-20.0, 0], to: [-0.0, 10]` on a dynamic-width label
- **THEN** its width is 20 at every resolved label width

#### Scenario: A hugging parent over a stretching child

- **WHEN** a `container` with `size: [content, 10]` and `padding: 1.0` holds a `text` with `size: [fill, 8]` whose laid-out width is 40
- **THEN** the container is 42 wide and the text's box is 40
- **AND** if that container is instead `fill` on a label another item sizes to 80, the text's box becomes 78

#### Scenario: A capped stretching item reports its cap

- **WHEN** a dynamic-width `single` with `width: { min: 10, max: 120 }` carries only a `qr` at `[0, 0]` with `size: [fill, 10]`, `max_w: 20` and intrinsic width 50
- **THEN** the label is 20 wide and the qr's box is 20 wide

#### Scenario: A chosen size never overflows

- **WHEN** a `content`-sized `qr` with intrinsic size 15 sits at `[0, 0]` in a frame 10 wide
- **THEN** its extent is 10

#### Scenario: A written size that does not fit is refused

- **WHEN** a template declares `at: [100, 0]` with `size: [40, 10]` in a frame 120 wide, or `at: [-20.0, 0]` with `size: [30, 10]`
- **THEN** it is refused at load
- **AND** `at: [-20.0, 0]` with `size: [20, 10]` is accepted

#### Scenario: Zero written versus zero supplied

- **WHEN** a template declares `size: [0, 10]`
- **THEN** it is refused at load
- **AND** `size: ["{box_w}", 10]` with `box_w` defaulting to 10 loads, and a request supplying `box_w: 0` fails with `UnsupportedLayoutItem` reason `size_invalid`

#### Scenario: An empty value collapses a chosen extent

- **WHEN** a `content`-width `text` bound to an empty value renders
- **THEN** its box is zero wide and the render succeeds

#### Scenario: A to that inverts only at render

- **WHEN** a stretching `to` valid against `width.max` inverts against a smaller resolved label width
- **THEN** the render fails with `edge_rect_inverted`

#### Scenario: A statically degenerate to is refused

- **WHEN** a template declares `at: [10, 4], to: [10, 9]`
- **THEN** it is refused at load

### Requirement: Intrinsic sizes

An item's intrinsic size on an axis SHALL be its content's extent multiplied by a scale into the template `unit`:

| Item | Intrinsic size |
| --- | --- |
| `text` | its laid-out block (`text`) |
| `qr` | `(modules + 2 × quiet_zone) × module_size` |
| `image`, PNG/JPEG | pixel dimensions × `1/dpi` (unit `in`) or `25.4/dpi` (unit `mm`), using the template's `dpi` |
| `image`, SVG, per axis | the absolute `width`/`height` converted to the template unit (unitless or `px` uses the pixel scale above); otherwise that axis's `viewBox` extent at the pixel scale; percentage or font-relative dimensions fall through to the `viewBox` |
| `container` | its children combined by its arrangement, plus padding |
| `line` | none; a line contributes only through its endpoints |

An absolutely arranged container's intrinsic extent on an axis SHALL be the largest frame requirement among its active children. A `qr` with a content or frame extent and no `module_size` SHALL be refused at load. When a content or frame extent is demanded at render and an SVG has no extent on that axis, or an image's dimension header cannot be parsed, the render SHALL fail with `UnsupportedLayoutItem` reason `intrinsic_size_undefined`; this applies to `src` and `name` images alike and is never decided at load. An authored-extent image SHALL NOT have its dimensions read for sizing. `fit` SHALL NOT affect intrinsic size.

#### Scenario: A QR sizes a label

- **WHEN** a dynamic-width `single` carries only a `qr` at `[0, 0]` with `size: [content, content]`, `module_size: 0.6`, `quiet_zone: 0` and a payload of 25 modules
- **THEN** the label is 15 wide
- **AND** with `quiet_zone: 2` the qr is `(25 + 4) × 0.6 = 17.4`

#### Scenario: A QR without a pitch

- **WHEN** a `qr` declares `size: [content, content]` and no `module_size`
- **THEN** it is refused at load, naming `module_size`
- **AND** with `size: [15, 15]` it is accepted

#### Scenario: Image intrinsic sizes

- **WHEN** a `content`-sized image carries a 300 by 150 px PNG on a template with `unit: in`, `dpi: 300`
- **THEN** it is drawn 1.0 by 0.5
- **AND** an SVG with `width="1in" height="0.5in"` on a `mm` template is drawn 25.4 by 12.7
- **AND** an SVG with `width="20mm"` and no `height` in `size: [content, 10]` is drawn 20 by 10

#### Scenario: An SVG with no extent fails at render

- **WHEN** a `content`-sized image carries an SVG with neither absolute dimensions nor a `viewBox`
- **THEN** the template loads and each render fails with `intrinsic_size_undefined`
- **AND** the same unreadable bytes in an item with `size: [20, 10]` reach the renderer and fail as `RenderFailed` `typst_compile_failed`

### Requirement: An item requires of its frame the smallest extent that contains it

An item's **requirement** on an axis SHALL be the smallest frame extent it fits in. Writing `a` and `b` for the insets of an edge-relative `at` and `to`, and **claim** for the item's extent (for a frame source, `min(intrinsic, cap, available)`):

| Placement | Requirement |
| --- | --- |
| `size`, `at` non-negative | `at + claim` |
| `size`, `at` edge-relative | `a` |
| `to`, both non-negative | `to` |
| `to`, both edge-relative | `a` |
| `to`, `at` non-negative, `to` edge-relative | `at + claim + b` |
| shrinking `to` | `max(a, to)` |
| `line` | the larger of its endpoints' requirements (`v` for non-negative, inset for edge-relative) |
| packed child | `claim` |

On a dynamic-width `single` the label width SHALL be the largest requirement among active top-level items, clamped into `[width.min, width.max]`. A requirement SHALL NOT depend on any other item's position, so reordering an absolutely arranged list changes only draw order. An inactive item imposes no requirement.

#### Scenario: A box requires its far edge

- **WHEN** a `text` at `[10, 0]` declares `size: [40, 10]`
- **THEN** its requirement is 50

#### Scenario: A right-anchored item requires its inset

- **WHEN** a dynamic-width `single` with `width: { min: 10, max: 120 }` carries a `text` at `[-40.0, 0]` with `size: [content, 10]` that lays out 30 wide
- **THEN** the label is 40 wide and the text box runs from x = 0 to 30

#### Scenario: A stretching to requires its margin

- **WHEN** a dynamic-width label carries a `text` at `[0, 0]` with `to: [-2.0, 10]` laid out 30 wide
- **THEN** the label is 32 wide and the box is 30

#### Scenario: A line requires its furthest endpoint

- **WHEN** a `line` runs from `[10, 4]` to `[-0.0, 4]` on a dynamic-width label resolving to 10
- **THEN** both endpoints meet and the render fails with `line_degenerate`

#### Scenario: Requirements compose through containers

- **WHEN** a `container` at `[5, 0]` with `size: [content, 10]` and `padding: 1.0` holds a `container` at `[2, 0]` with `size: [content, 8]` holding a `text` at `[3, 0]` with `size: [7, 6]`
- **THEN** the inner container is 10 wide, the outer 14, and the outer's requirement is 19

### Requirement: Axis resolution and the shrinking to

A frame axis is **resolved** when its extent is known before the items inside it are sized. The label's height axis SHALL be resolved; its width axis SHALL be resolved on a `sheet` and a fixed-width `single` and unresolved on a dynamic-width `single`. A container's inner axis SHALL be resolved when its extent on that axis is authored; or is a frame source under an edge-relative `at`; or, otherwise, when its enclosing axis is resolved and its source is not `content`. A packed child follows the last clause. `rotate: 90` or `270` SHALL swap the two axes' states; `180` SHALL NOT.

A shrinking `to` SHALL be accepted only on a resolved axis and refused at load otherwise, with a message saying the extent shrinks as the label grows. Where accepted it is an authored extent.

#### Scenario: A shrinking to on a dynamic width

- **WHEN** an item declares `at: [-20.0, 0], to: [90.0, 10]` on a dynamic-width `single`
- **THEN** it is refused at load
- **AND** on a fixed-width `single` 100 wide it resolves to 10 wide with requirement 90, and on a frame 80 wide it is refused

#### Scenario: A right-anchored fill container resolves its inner axis

- **WHEN** a dynamic-width `single` carries a `container` at `[-40.0, 0]` with `size: [fill, 20]` holding a child `at: [-20.0, 0], to: [30.0, 10]`
- **THEN** the template loads
- **AND** the same child inside a container at `[0, 0]` with `size: [fill, 20]`, or inside a `size: [content, 20]` container, is refused

#### Scenario: A quarter turn moves the state

- **WHEN** a top-level `container` with `rotate: 90` and `size: [fill, 20]` on a dynamic-width `single` holds a child `at: [0, -20.0], to: [5, 90.0]`
- **THEN** it is refused; with `size: [40, 20]` it is accepted; with `rotate: 180` the child `at: [0, -8.0], to: [5, 15.0]` is accepted

### Requirement: Dynamic width bounds must be ordered

On a dynamic-width `single`, both resolved `width` bounds SHALL first be judged as dimensions (non-positive, non-finite or above `max_label_dimension_mm` fails with `422 UnsupportedLayoutItem` reason `dimension_exceeds_limit`). If the resolved `min` then exceeds the resolved `max`, the render SHALL fail with `400 InvalidRequest` reason `width_bounds_inverted`, before any item is measured, with a message naming both values with the unit and each referenced parameter. Bounds resolve from request values, else defaults, else literals, and the check applies to every render path, including thumbnails and each label of a batch (where it is a per-label failure). Equal bounds SHALL render at that width.

#### Scenario: A supplied max below min

- **WHEN** a template declares `width: { min: 10.0, max: "{max_width}" }` and a render supplies `max_width: 5`
- **THEN** the response is `400` `InvalidRequest` with reason `width_bounds_inverted`, naming `max_width`, `5` and `10`
- **AND** `max_width: 0` instead gives `422` `dimension_exceeds_limit`
- **AND** `max_width: 10` renders a label 10 wide

#### Scenario: Ordering is checked before items

- **WHEN** the same template has a `text` sized `"{w}"`, and a render supplies `max_width: 5` and `w: 0`
- **THEN** the response is `400 width_bounds_inverted`
- **AND** with `max_width: 40` it is `422 UnsupportedLayoutItem` `size_invalid`

### Requirement: Load-time validation runs the render-time sizing rules

Load SHALL validate geometry with the same sizing rules render uses, against a frame built from the template alone: `width.max` for a dynamic width, and each geometry parameter reference instantiated from its `default` (a number, or a string parsing as one), else its `min`, else `0`. Load SHALL NOT measure text, encode QR codes, decode images or run a flow arrangement; a content extent SHALL stand in at its available extent. A refusal at load therefore depends only on the template.

Structural validation SHALL traverse every branch whether or not its `when:` gate could match. A packed child SHALL be checked at load as if it were its container's only child, so an accumulation of siblings past the inner box is a render-time failure only.

#### Scenario: An invalid inactive branch is refused

- **WHEN** an item behind `when: { debug: true }`, with `debug` defaulting to `false`, declares `size: [0, 10]`
- **THEN** the template is refused at load

#### Scenario: A geometry parameter without a default

- **WHEN** `size: ["{box_w}", 10]` names a parameter with `min: 12` and no `default`
- **THEN** load validates that axis as 12

#### Scenario: An accumulation is a render failure

- **WHEN** a flow container with inner width 20 and `gap: 2` holds two `content`-width texts
- **THEN** the template loads; values measuring 5 and 6 render; values measuring 14 and 6 fail with `item_out_of_frame`

### Requirement: Render failures locate the item

A render-time layout failure message SHALL name the item's layout path (`layout[0].items[2]`). Inside a repeat instance the path SHALL append `#` and the zero-based element index after the repeating container (`layout[0].items[0]#3`). Load-time refusals name the authored path without an index.

#### Scenario: An instance is named

- **WHEN** the third instance of a repeat at `layout[0].items[0]` overflows its strip
- **THEN** the message names `layout[0].items[0]#2`

### Requirement: The qr item

A `qr` SHALL accept `value` (required, interpolated, must not be blank), placement, `when`, and an optional `params` block with `error_correction`, `module_size` and `quiet_zone`; unknown keys inside `params` SHALL be ignored. `error_correction` is `L`, `M`, `Q` or `H`, case-insensitive and trimmed, defaulting to `M` when absent or blank; any other value SHALL fail at render with `UnsupportedLayoutItem` reason `qr_error_correction_invalid`. `module_size` is the module pitch in the template unit and MUST be greater than 0. `quiet_zone` is a count of modules, default 0, MUST be at least 0, and need not be whole. The symbol SHALL be drawn with exactly `quiet_zone` modules of margin on each side, as an SVG scaled `contain` into the item's box, whether or not `module_size` is set. A payload that cannot be encoded SHALL fail with `RenderFailed` reason `qr_generation_failed`.

#### Scenario: The quiet zone is the requested margin

- **WHEN** a `qr` with `size: [15, 15]`, no `module_size` and `quiet_zone: 2` renders
- **THEN** the symbol carries a 2-module margin on each side

#### Scenario: A bad error correction level fails at render

- **WHEN** a `qr` declares `params: { error_correction: X }`
- **THEN** the template loads and the render fails with `qr_error_correction_invalid`

### Requirement: The image item

An `image` SHALL set exactly one of `src` or `name`, plus placement, `when`, and `fit` (`contain` default, `cover`, `stretch`). `src` is a path under `{LABELER_CONFIG_DIR}/assets/` (interpolated) whose extension (`png`, `jpg`, `jpeg`, `svg`, case-insensitive) decides the format. `name` SHALL name a declared `string` parameter, match `^[a-zA-Z0-9_-]+$`, and be unique among its siblings; its value MUST be a base64 `data:` URI of type `image/png`, `image/jpeg`, `image/jpg` or `image/svg+xml`. The image SHALL be drawn into its box by `fit` and clipped to the box. There is no URL fetching.

Render failures, all `UnsupportedLayoutItem` unless noted: missing data key `422 MissingField`; an array value `field_value_not_scalar`; not a base64 data URI `image_data_invalid`; unsupported MIME or extension `image_format_unsupported`; asset missing `image_asset_missing`; unreadable `image_asset_unreadable`; path escaping the assets directory `image_asset_path_escapes`; assets directory unresolvable `assets_dir_unavailable`.

#### Scenario: An escaping asset path is refused

- **WHEN** an image declares `src: "../secret.png"`
- **THEN** the render fails with `image_asset_path_escapes`

#### Scenario: Both sources are refused

- **WHEN** an image declares both `src` and `name`, or neither
- **THEN** the template is refused at load

### Requirement: The line item

A `line` SHALL accept `at` (default `[0, 0]`), `to` (required), optional `stroke` and `when`, and nothing else; it has no box, `size`, `fit` or rotation. Endpoints SHALL resolve in the current frame, may be edge-relative, MUST lie inside the frame, and MUST differ after resolution (within 0.0001); otherwise load refuses the template, or render fails with `coord_out_of_frame`, `line_endpoint_out_of_frame` or `line_degenerate`. An absolute endpoint past `width.max` SHALL be refused at load. A line without `stroke` SHALL draw nothing but is still checked. A line SHALL NOT be a packed child.

#### Scenario: An out-of-frame endpoint is refused

- **WHEN** a `line` on a label 50 wide declares `to: [60, 5]`
- **THEN** the template is refused at load

### Requirement: Containers establish a padded frame

A `container` SHALL accept placement, `when`, `padding`, `items`, `rotate`, `flow`, `repeat`, `shape`, `stroke`, `background` and `rounded`. `padding` is a number (uniform) or `[top, right, bottom, left]`, each at least 0, default 0. Children SHALL be sized and placed against the container's **padded inner box**, its resolved box less padding, clamped at zero; padding is inside the container's box.

Padding that leaves no room in the box as load resolves it SHALL be refused at load. At render, a zero inner box SHALL render the container as an empty box; each child still applies its own rules there, so a child with an authored extent that does not fit, a `line` needing room, or a `text` with non-empty content (`text_does_not_fit`) still fails.

#### Scenario: Zero inner box at render

- **WHEN** a container's resolved width equals its horizontal padding and its only active child is a `content`-width `text` bound to an empty value
- **THEN** it renders as an empty box and the render succeeds
- **AND** an active child `image` with `size: [1, 1]` there fails its authored-extent check
- **AND** a template declaring `size: [10, 10]` with `padding: 6.0` is refused at load

### Requirement: Container rotation

`rotate` SHALL be accepted only on a `container` and MUST normalise (modulo 360, within 0.001 degrees) to 0, 90, 180 or 270; any other value is refused at load. Rotation is counter-clockwise; author corners map to physical corners as R90 BL→BR, BR→TR, TR→TL, TL→BL; R180 BL→TR; R270 BL→TL. A rotated container SHALL be placed and bounds-checked in its parent exactly as an unrotated one; rotation transforms only its inner content, so nested rotations compose. For 90 and 270 the author canvas SHALL be `[inner_h, inner_w]`. Padding SHALL be author-space. The container's own paint SHALL stay axis-aligned in the parent frame.

A rotated container's intrinsic size SHALL be computed in author space (children combined by its arrangement, plus padding) and then swapped for 90 and 270. `content` and `fill` SHALL be accepted on and under a rotated container. `max_w` on a child names the author axis.

#### Scenario: A rotated container hugs rotated content

- **WHEN** a `container` with `rotate: 90` and `size: [content, content]` and `padding: 2.0` holds a `text` at author `[3, 1]` with `size: [10, 4]`
- **THEN** its author-space intrinsic size is 17 by 9 and its footprint in the parent is 9 wide by 17 tall
- **AND** with `rotate: 180` the footprint is 17 by 9

#### Scenario: A non-orthogonal rotation is refused

- **WHEN** a container declares `rotate: 45`, or a `text` declares `rotate: 90`
- **THEN** the template is refused at load

### Requirement: A flow block packs a container's children

A `container` MAY carry `flow`, which selects packing for its direct children; without it children are placed by their own coordinates.

| Key | Type | Default | Rule |
| --- | --- | --- | --- |
| `direction` | `row` or `column` | required | Primary axis. `row` packs along +x from the inner box's left edge; `column` packs downward from its top edge. |
| `gap` | number ≥ 0 | 0 | Space between adjacent occupying children on a line. |
| `wrap` | boolean | `false` | Whether a child that does not fit the room left on its line starts a new line. |
| `line_gap` | number ≥ 0 | 0 | Space between lines; inert without `wrap`. |
| `overflow` | `fail` or `trim` | `fail` | What happens to a child packed past the inner box. |

A missing or unknown `direction`, a negative or non-finite `gap`/`line_gap`, an unknown `overflow`, or `flow: null` SHALL be refused at load naming the key's path. `wrap: true` SHALL be refused unless the container's primary inner axis is resolved, and `overflow: trim` unless both inner axes are resolved, each naming the key. Children SHALL align to the inner box's top edge in a `row` and left edge in a `column`. A flow container under `rotate` SHALL pack in author space.

#### Scenario: A column packs downward

- **WHEN** a `column` flow container with `gap: 2` holds three children 4 tall
- **THEN** the first child's top is the inner box's top, each next one 2 below the previous, all at the left edge

#### Scenario: Derived refusals

- **WHEN** a `row` flow container declares `wrap: true` with `size: [content, 10]`
- **THEN** it is refused naming `wrap`; a `column` with the same size loads
- **AND** `overflow: trim` with `size: [content, 10]` or `[20, content]` is refused naming `overflow`, and with `[20, 10]` loads

### Requirement: Packed children

A **packed child** is a direct child of a flow container. It SHALL NOT carry `at` or `to`, and SHALL NOT be a `line`; each is refused at load naming the child's path. It SHALL be sized against the padded inner box as an anchorless item, exactly as the same item at `[0, 0]` in an absolutely arranged container. An uncapped `fill` child therefore takes the whole inner extent on that axis: alone it fills the container, and beside a sibling it overflows. A packed container giving no extent is `[fill, fill]` like any container.

#### Scenario: A fill child beside a sibling overflows

- **WHEN** a `row` flow container with inner width 30 and `gap: 2` holds a 10-wide child then a `size: [fill, 4]` child
- **THEN** the second box is 30 wide at x = 12 and the render fails with `item_out_of_frame`
- **AND** with `max_w: 8` on the second child it is 8 wide and fits

#### Scenario: Two sizeless packed containers collide

- **WHEN** a `row` flow container holds two `container` children with neither `size` nor `to`
- **THEN** the render fails with `item_out_of_frame` naming the second
- **AND** with `size: [content, content]` they pack side by side

### Requirement: Packing order, lines and occupancy

Children SHALL be packed in template order. A child **occupies** the primary axis when it is active and its box's primary extent is greater than zero. The first occupying child on a line SHALL sit at the inner box's leading edge, and each later one a `gap` past the previous occupying child's trailing edge; no gap leads or trails a line. With `wrap: true` an occupying child whose box does not fit the room left on the line SHALL start a new line; a child is never split. Each line SHALL begin one `line_gap` past the previous line's tallest drawn box.

A gated-off child SHALL occupy, draw and contribute nothing. An active child with zero primary extent SHALL occupy nothing, consume no gap, be drawn at the leading edge the next occupying child would take, stay on the current line, contribute its secondary extent, and still raise its own errors.

#### Scenario: A gated-off child leaves no hole

- **WHEN** a `column` with `gap: 1` holds three 3-tall texts and the middle one's gate fails
- **THEN** the third is drawn 1 below the first

#### Scenario: An empty value leaves no double gap

- **WHEN** a `row` with `gap: 2` holds three `content`-width texts and the middle value is empty
- **THEN** the third is exactly 2 right of the first

#### Scenario: A row wraps

- **WHEN** a wrapping `row` with inner width 20, `gap: 2`, `line_gap: 1` holds three 8 by 4 children
- **THEN** two sit on the first line and the third starts the second line at the left edge, 1 below the first line

### Requirement: The assembled extent of a flow container

A flow container's intrinsic size SHALL be its padding plus its **assembled extent**: on the primary axis the largest line total (sum of occupying children's requirements plus gaps between them); on the secondary axis the sum of line extents (each the largest requirement on that line) plus `line_gap`s between lines. With no occupying or drawn child it SHALL be zero on both axes, which renders an empty box. Flow containers SHALL nest and SHALL pack identically on every format.

#### Scenario: A content-sized flow container hugs

- **WHEN** a `row` flow container with `size: [content, content]`, `gap: 2`, `padding: 1` holds children 10 and 6 wide, 4 tall
- **THEN** it is 20 wide and 6 tall

#### Scenario: A wrapped container assembles from its lines

- **WHEN** a wrapping `row` with inner width 20, `gap: 2`, `line_gap: 1` packs 8×4, 8×6 and 8×5 into two lines
- **THEN** its assembled extent is 18 wide and 12 tall

### Requirement: Packing past the inner box

Each packed child SHALL be checked twice against the padded inner box: its own extents (at load and render) and its arranged position (at render). Both SHALL fail with `UnsupportedLayoutItem` reason `item_out_of_frame` on either axis, never `coord_out_of_frame`. The own-extent check runs first and always fails. Under `overflow: trim` the first child whose arranged box does not fit, and every child after it, SHALL be left undrawn, out of the assembled extent, and unreported; trimmed children are still sized, so a trimmed `text` reading a missing field still fails `MissingField`, while a trimmed authored-size `image` or `qr` reads nothing.

#### Scenario: An accumulated overrun fails

- **WHEN** a `row` with inner width 20 and `gap: 2` packs children 12 and 9 wide
- **THEN** the render fails with `item_out_of_frame`
- **AND** under `overflow: trim` the second child is not drawn and the render succeeds

#### Scenario: Trim drops every later child

- **WHEN** a trimming `row` with inner width 20 and `gap: 2` packs children 8, 8 and 2 wide
- **THEN** the first two are drawn and the third is not

#### Scenario: A child too large fails even under trim

- **WHEN** a packed child's parameter-supplied extent exceeds the inner box
- **THEN** the render fails with `item_out_of_frame` under either policy

### Requirement: repeat draws a container once per list element

A `container` MAY carry `repeat:`, the bare name of a declared `type: list` parameter. Each of these SHALL be refused at load naming the key and the container's layout path: `repeat` written empty (null); naming an undeclared parameter; naming a parameter of another type (also naming its type); on a container whose parent has no `flow`, including the layout root; naming a list an enclosing repeat already repeats. `repeat` on other item types is an unknown key. Through a template write each of these, and the two scope refusals below, SHALL be `422 TemplateInvalid` reason `template_parse_failed`.

An active repeating container SHALL produce one packed instance per element, in element order, in the authored container's place among its siblings. An empty list produces none and is not an error. An absent list with no usable default SHALL fail `422 MissingField` naming it. The container's own `when:` SHALL be evaluated once, before binding, and gates every instance; a gated-off repeat reads nothing. There is no instance cap; overflow is the flow policy's. Each instance is sized on its own. Load checks the repeated subtree once as a single instance.

#### Scenario: Three tags render three pills

- **WHEN** a `row` with `gap: 1` holds a `size: [content, content]` container repeating `tags` around a text `{tags}`, and a request sends `["A", "B", "C"]`
- **THEN** three pills `A`, `B`, `C` are drawn left to right, each hugging its element
- **AND** `tags: []` draws the strip with no pills, and omitting `tags` with no default is `422 MissingField`

#### Scenario: A sizeless repeat fills its strip

- **WHEN** the pill omits `size` and `to` and a request sends `["A", "B"]`
- **THEN** the render fails with `item_out_of_frame` naming the second instance

#### Scenario: Misplaced repeats are refused

- **WHEN** a `repeat:` sits on a root-level container, on a child of a non-flow container, names a `string` parameter, or nests over the list an ancestor repeats
- **THEN** the template is refused at load
- **AND** a nested repeat over a different list loads and nests its instances

### Requirement: Inside a repeat the name is one element

Within a repeating container and its descendants, the repeated name SHALL denote the current element, a string. A bare `{p}` in a `text` or `qr` `value` or an `image` `src` SHALL print the element. `{p:join(...)}` and `{p:<format>}` inside the scope SHALL be refused at load, naming the token and the item's layout path (the latter as a format applied to a non-instant). A `when:` key naming `p` inside the scope SHALL compare the element. Typed slots (`size`, `max_w`, `max_h`, `font_weight`, colour references, image `name`) SHALL still refuse a list parameter inside the scope. In nested repeats each name binds its own element. Outside every scope the parameter remains a list.

#### Scenario: Nested scopes bind two names

- **WHEN** a repeat over `tags` holds a flow container whose child repeats `codes` and prints `{tags}-{codes}`, with `tags: [A, B]` and `codes: ["1", "2"]`
- **THEN** four texts read `A-1`, `A-2`, `B-1`, `B-2` in that order

#### Scenario: A gate inside the scope compares the element

- **WHEN** a text inside a repeat over `tags` carries `when: { tags: KIDS }` and the request sends `["KIDS", "SPARES"]`
- **THEN** the text is drawn in the first instance only

### Requirement: when gates an item

Any item MAY carry `when:`, a map of conditions. The item is **active** only when every condition matches the label's resolved parameter value, compared as text; a condition on an absent parameter is false. An inactive item, and everything inside an inactive container, SHALL be excluded from measurement and rendering: it imposes no requirement, paints nothing, raises no error, and a field read only by inactive items is not required.

At load, each of these SHALL be refused: `when: {}`; an empty or whitespace-only key or value; a value that is null, a sequence or a mapping; a key naming an undeclared parameter; a value outside an `enum` parameter's `values`; a key naming a `list` parameter outside a repeat scope (including on the repeating container itself), naming the key and the item's layout path. A condition value is held as its scalar text, so `true` and `"true"` are one condition. `when:` written with no value (null) SHALL be treated as absent.

#### Scenario: An inactive branch does not require its fields

- **WHEN** a text whose value reads `{v_text}` sits behind a gate that does not match and the request omits `v_text`
- **THEN** the label renders without `MissingField`

#### Scenario: Refused gates

- **WHEN** a container carries `when: {}`, `when: { mode: "" }`, `when: { tags: KIDS }` over a declared list, `when: { size: medium }` for an enum without `medium`, or a key naming no declared parameter
- **THEN** the template is refused at load

#### Scenario: A scalar condition is its text

- **WHEN** a container carries `when: { bold: true }` for a boolean `bold` resolving to true
- **THEN** it is drawn, as with `when: { bold: "true" }`

### Requirement: Shapes and paint keys

`container` and `line` are shapes; only a `container` has an interior.

| Key | On | Default | Rule |
| --- | --- | --- | --- |
| `stroke` | container, line | no outline | `{ thickness, color }`; see stroke. |
| `background` | container | no fill | A colour. |
| `rounded` | container with `shape: rect` | square corners | Corner radius in template units. |
| `shape` | container | `rect` | `rect`, `ellipse` or `circle`; anything else is refused naming the accepted set. |

Paint on any other item, `background` or `rounded` on a `line`, and `rounded` on `ellipse`/`circle` SHALL be refused at load. `stroke`, `background`, `rounded`, `shape` and `stroke.color` written as null SHALL be refused naming the field. Every combination of stroke and background SHALL render as declared. Paint SHALL NOT be inherited by children.

#### Scenario: Fill only, outline only

- **WHEN** one container declares `background: "#000000"` and no stroke, and another `stroke: { thickness: 0.02 }` and no background
- **THEN** the first is a solid black block with no outline and the second an outline with the interior showing through, in PNG and PDF alike

#### Scenario: Null paint is refused

- **WHEN** a container declares `stroke: null`, `background: null` or `rounded: null`
- **THEN** the template is refused at load naming the field

### Requirement: A stroke is a thickness and a colour

`stroke` SHALL accept only `thickness` (required, template units, finite and at least 0.0001) and `color` (a colour, default `black`). A missing, null, zero, negative, non-finite or too-small thickness, a null `color`, or any other key SHALL be refused at load; a missing thickness and a null one SHALL be reported distinctly. The stroke SHALL be centred on the painted boundary, SHALL NOT affect any size or position, and its outer half SHALL be clipped only by an ancestor or the label.

#### Scenario: A thickness alone draws black

- **WHEN** a container declares `stroke: { thickness: 0.02 }`
- **THEN** the outline is `#000000` at 0.02 units

#### Scenario: Out-of-range thickness

- **WHEN** a stroke declares thickness `0`, `-0.5`, `0.00001`, `.nan` or `.inf`
- **THEN** the template is refused at load
- **AND** `0.0001` is accepted

### Requirement: The corner radius is authored

`rounded` SHALL be finite and at least 0.0001 (zero, NaN, infinity and smaller values are refused). It SHALL be the same for outline and fill, independent of the stroke, and SHALL be reduced at render to half the shorter side of the resolved box when larger.

#### Scenario: An oversized radius is clamped

- **WHEN** a container resolving to 4.0 by 2.0 declares `rounded: 5.0`
- **THEN** its corners render with radius 1.0

### Requirement: Geometry, paint order and clipping

`rect` SHALL paint the container's outer box (padding band included); `ellipse` SHALL paint the ellipse inscribed in it; `circle` SHALL paint the same and require a square box (extents within 0.0001). Paint SHALL be drawn background, then stroke, then children. Geometry SHALL NOT change where children sit or how large they are.

At `rect` the container SHALL clip its children at the inner edge of its stroke, following the rounded corner (the stroke's inner curve, or the painted curve when there is no stroke). At `ellipse` and `circle` children SHALL be clipped only to the rectangular box, so they may cross the curve and draw over the stroke. A container's clip SHALL NOT cut its own paint.

A `circle` whose both extents are fixed by the template (numbers, or a `to` with both corners non-negative or both edge-relative) SHALL be refused at load when not square. Every active `circle` SHALL be checked at render, failing with `422 UnsupportedLayoutItem` reason `circle_box_not_square` (a per-label failure in a batch) and a message naming the container's path. A gated-off circle is not checked.

#### Scenario: A circle sized from content is judged at render

- **WHEN** a container declares `shape: circle` and `size: [content, content]` in a non-square frame
- **THEN** it loads, renders when its box resolves square, and fails with `circle_box_not_square` otherwise
- **AND** `shape: circle` with `size: [14, 12]` is refused at load

#### Scenario: A parameter-sized circle is judged per request

- **WHEN** a circle declares `size: ["{w}", 12]` with `w` defaulting to 12
- **THEN** it loads and renders without `w`, and a request supplying `w: 14` fails with `circle_box_not_square`

#### Scenario: A stroked rectangle cuts child ink

- **WHEN** a `rect` container with `stroke: { thickness: 1.0 }` holds a child reaching its edge
- **THEN** the child is placed as without a stroke and its ink is cut 0.5 units in

#### Scenario: A round geometry does not clip to its curve

- **WHEN** an `ellipse` container holds a text wide enough to cross the curve
- **THEN** the text renders in full within the box

### Requirement: The colour vocabulary

A colour SHALL be exactly one of: one of sixteen names, case-insensitive; a hex string `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` (digits case-insensitive, short forms expanding by doubling, alpha composited in PNG and PDF); or a `"{param}"` reference. Leading and trailing whitespace SHALL be stripped first; anything else (including inner whitespace, a hex without `#`, other digit counts, non-hex digits, non-strings, empty or all-whitespace strings, and other names such as `orange`) SHALL be refused at load naming the file, the item's layout path and the field. The fields taking a colour are `text.color`, `stroke.color` and `background`, and a name denotes the same value on each:

| Name | Value | Name | Value |
| --- | --- | --- | --- |
| `black` | `#000000` | `silver` | `#c0c0c0` |
| `white` | `#ffffff` | `gray` | `#808080` |
| `red` | `#ff0000` | `maroon` | `#800000` |
| `yellow` | `#ffff00` | `olive` | `#808000` |
| `lime` | `#00ff00` | `green` | `#008000` |
| `aqua` | `#00ffff` | `teal` | `#008080` |
| `blue` | `#0000ff` | `navy` | `#000080` |
| `fuchsia` | `#ff00ff` | `purple` | `#800080` |

#### Scenario: Names and hex forms

- **WHEN** a template writes `RED`, `" red "`, `"#f0f"` or `"#FF00FF80"`
- **THEN** they denote `#ff0000`, `#ff0000`, `#ff00ff` and `#ff00ff` at half alpha

#### Scenario: Unreadable colours are refused

- **WHEN** a template writes `"#ff00f"`, `"ff00ff"`, `"re d"`, `""`, `16711680` or `chartreuse`
- **THEN** the template is refused at load naming the value

### Requirement: Colour parameter references

A colour reference SHALL name a declared `string` or `enum` parameter, or be refused at load naming the item's path, the field and the parameter (and its type). At render the resolved value SHALL be stripped and read as a name or hex colour. A value that is not a colour, is all whitespace, or is itself a `"{name}"` reference SHALL fail with `400 InvalidRequest` reason `color_param_invalid` naming the parameter, with no fallback colour; in a batch it is a per-label failure with that code and reason.

#### Scenario: A referenced colour renders

- **WHEN** a container declares `background: "{brand}"` and a request supplies `brand: " navy "`
- **THEN** the interior is `#000080`

#### Scenario: A bad resolved colour fails

- **WHEN** a request supplies `brand: "octarine"` or `brand: "{other}"` for a colour reference
- **THEN** the single render fails `400` with `color_param_invalid` naming `brand`
- **AND** in a two-label batch where only the second is bad, the batch fails `422 BatchInvalid` with one failure at `index` 1 and nothing is produced

### Requirement: Layout read-back

`GET /api/templates/{id}` SHALL report the layout as authored: a packed child with neither `at` nor `to`; every other boxed item with its `at`, including a defaulted one; a repeating container once, with its `repeat` key. A literal colour SHALL be reported as the decoded YAML string, unnormalised (case, digit count and surrounding whitespace kept); a colour reference as `"{name}"`. `stroke.color` SHALL always be reported (`"black"` when omitted); `text.color` and `background` only when written. Resubmitting the response unchanged SHALL be accepted.

#### Scenario: Colours read back as written

- **WHEN** a template with `stroke: { thickness: 0.2, color: "#F0F" }`, `background: " {brand} "` and a text `color: " red "` is read back
- **THEN** the response reports `"#F0F"`, `"{brand}"` and `" red "`
- **AND** a stroke written without `color` reports `"black"`
