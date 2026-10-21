# Spec Delta

## MODIFIED Requirements

### Requirement: An extent comes from the author, the content, or the frame

Every extent on each axis SHALL take its behavior from its **source**:

| Source | Spellings | Extent |
| --- | --- | --- |
| author | a number; a `"{param}"` reference; a `to` whose corners are both non-negative or both edge-relative | as written |
| content | `content` | the item's intrinsic size |
| frame | `fill`; a `to` with a non-negative `at` and an edge-relative `to` | the available extent |

A boxed item pairing an edge-relative `at` with a non-negative `to` on the same axis SHALL be refused at load.

The **available extent** on an axis SHALL be `frame − resolve(at) − inset`, where `inset` is the magnitude of an edge-relative `to` or zero. An item with no anchor SHALL use the availability defined by "Packed children".

1. An authored extent SHALL be checked: one that does not fit its frame is refused (at load when written in the template; at render with `item_out_of_frame` when it comes from a request value). A content or frame extent SHALL be `min(source value, max_w/max_h, available)` and so never overflows.
2. Only content and frame extents SHALL demand an intrinsic size.
3. A content or frame extent of exactly zero SHALL render an empty box. An authored extent of zero or less written in the template SHALL be refused at load; one supplied through a parameter SHALL fail at render with `size_invalid`. A `to` that inverts SHALL fail with `edge_rect_inverted` (refused at load when it already inverts against the load-time frame).

A frame extent SHALL report `min(intrinsic, cap, available)` to its parent while taking the available extent as its box. The two axes are independent.

#### Scenario: A cap binds only a chosen size

- **WHEN** an item declares `size: [content, 10], max_w: 30` with an intrinsic width of 45
- **THEN** it resolves to 30
- **AND** `size: [40, 10], max_w: 30`, `size: [content, 10], max_h: 5` and `at: [0, 0], to: [-0.0, 10], max_w: 30` are each refused at load

#### Scenario: An edge-relative at with a non-negative to is refused

- **WHEN** an item declares `at: [-20.0, 0], to: [90.0, 10]`
- **THEN** the template is refused at load, on a fixed-width label as on a dynamic-width one

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

### Requirement: Packed children

A **packed child** is a direct child of a flow container. It SHALL NOT carry `at` or `to`, and SHALL NOT be a `line`; each is refused at load naming the child's path. For `fill` on the primary axis, its available extent SHALL be the current line's remaining extent after earlier occupying boxes and their gaps, less the pending gap before this child, clamped at zero. Other extents SHALL use the whole padded inner box. Initial measurement SHALL use this allocation, in template order.

#### Scenario: A capped fill child fits beside a sibling

- **WHEN** a `row` flow container with inner width 30 and `gap: 2` holds a 10-wide child then a `size: [fill, 4]` child with `max_w: 8`
- **THEN** the second box is 8 wide at x = 12 and both boxes fit

#### Scenario: A fill title takes the remainder of a row

- **WHEN** a `row` flow container with inner width 30 and `gap: 2` holds a 10-wide child then a `size: [fill, 4]` child
- **THEN** the second box is 18 wide at x = 12 and both boxes fit

#### Scenario: A column consumes remaining height

- **WHEN** a `column` flow container with inner height 30 and `gap: 2` holds a 10-tall child then a `size: [4, fill]` child
- **THEN** the second box is 18 tall and starts 2 below the first child's bottom edge

### Requirement: The assembled extent of a flow container

A flow container's intrinsic size SHALL be its padding plus its **assembled extent**: on the primary axis the largest line total (sum of occupying children's requirements plus gaps between them); on the secondary axis the sum of line extents (each the largest requirement on that line) plus `line_gap`s between lines. With no occupying or drawn child it SHALL be zero on both axes, which renders an empty box. Flow containers SHALL nest and SHALL pack identically on every format. Intrinsic assembly SHALL be determined against the initial sizing frame; final smaller-box text fitting SHALL NOT recompute that assembly or resize ancestors.

#### Scenario: A content-sized flow container hugs

- **WHEN** a `row` flow container with `size: [content, content]`, `gap: 2`, `padding: 1` holds children 10 and 6 wide, 4 tall
- **THEN** it is 20 wide and 6 tall

#### Scenario: A wrapped container assembles from its lines

- **WHEN** a wrapping `row` with inner width 20, `gap: 2`, `line_gap: 1` packs 8×4, 8×6 and 8×5 into two lines
- **THEN** its assembled extent is 18 wide and 12 tall
