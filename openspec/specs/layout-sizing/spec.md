## Purpose

Defines how every layout item's box is sized: where an extent comes from, what a node reports to its
parent, what it requires of its frame, and how text is laid out inside the box it gets. One
algorithm, applying to every item type, both axes, and every format.

## Requirements

### Requirement: An extent comes from the author, from the content, or from the frame

Every extent on every axis SHALL come from exactly one of three **sources**. The source, not the item
type, decides how the extent behaves.

| Source | Spellings | Extent |
| --- | --- | --- |
| **author** | a number, a parameter reference, a `to` whose corners are both non-negative or both sign-negative | what the author wrote |
| **content** | `content` | the item's intrinsic size |
| **frame** | `fill`, a `to` with a non-negative `at` and a sign-negative `to` | the space available |
| **author**, conditionally | a `to` with a sign-negative `at` and a non-negative `to` | the corner subtraction, permitted only on a resolved axis (below) |

The table is total: those four rows classify every spelling of every axis. The last row is authored in
every respect once permitted, and the condition on it is the only place the resolved-axis state is
consulted.

The **available extent** on an axis SHALL be `frame extent − resolve(at) − inset`, where
`resolve(at)` is `at`'s component when non-negative and `frame extent + at` when sign-negative, and
`inset` is the far-edge margin a `to` reserves (`−to`'s component) or zero. For a sign-negative `at`
of inset `a` the frame terms cancel and the available extent is `a − inset`, independent of the
frame: the anchor is the box's low edge, so a right- or top-anchored item has only the space between
its anchor and the far edge, less any margin it reserves there.

An item with **no anchor** SHALL have the frame extent itself available on each axis. It has nothing
to subtract and no inset to reserve, so both terms of the formula are absent rather than zero. A
packed child (`flow-layout`) is the only such item, and this is stated rather than left to the formula
degenerating, because an arrangement decides where such an item goes and never how large it is: the
whole frame is what it is offered on purpose. Every rule below then applies to it unchanged, on both
axes, keyed to its source exactly as for an anchored item.

Four rules follow from the source alone, and are stated here once rather than repeated per spelling:

1. **An authored extent is checked; a content or frame extent is clamped.** An authored extent that
   does not fit its frame is an authoring error. A content or frame extent is
   `min(source value, max_w/max_h, available extent)` and therefore cannot overflow.
2. **`max_w` and `max_h` bind content and frame extents, and are inert on authored ones.** A cap
   bounds a size the engine chose; it never contradicts a number the author wrote. `max_*` alongside
   `to` SHALL NOT be an error: whether it binds follows from that `to`'s source.
3. **Only content and frame extents demand an intrinsic size.** An authored extent needs none,
   because the author supplied the number. This is why an `image` with `size: [20, 10]` never has its
   dimensions read.
4. **A content or frame extent of exactly zero renders an empty box; an authored zero is refused.**
   A blank data value legitimately clamps to nothing, and blank optional fields are ordinary input in
   CSV-driven printing. A number the author wrote as zero is a mistake, refused where it is written.

A **frame** extent additionally SHALL report upward `min(intrinsic, max_w/max_h, available extent)`
while taking the available extent downward. That single asymmetry is what `fill` means, and it is
what lets an item that stretches to a label still be the item that sizes it.

The report is bounded by the same cap and available extent as any other clamped extent, and not by
the raw intrinsic size. A `qr` with `size: [fill, 10]`, an intrinsic width of 50 and `max_w: 20`
reports 20, not 50: reporting the unbounded intrinsic would let an item ask a label for more width
than it will then occupy, breaking both guarantees below.

This requirement supersedes the `size` and `max_w`/`max_h` rows of the frozen `docs/SPEC.md` §4
placement table; the §4 passage from "`auto` size resolves to `min(max_{w,h}, fallback)`" through
"(`line` does not use `size`; see §4.1)"; the §4 paragraph beginning "A fallback (or a
`max_*`-capped resolution) of exactly `0`"; the §4.1 `container` clause "size defaults to
`auto`/`auto` = fill parent"; and the §3.1 sentence "`auto` item width on a dynamic-width label
resolves to the content width (`label_width - at.x`)".

#### Scenario: A cap binds a chosen size and not a written one

- **WHEN** one item declares `size: [content, 10], max_w: 30` with an intrinsic width of 45, and
  another declares `size: [40, 10], max_w: 30`
- **THEN** the first resolves to 30 and the second to 40

#### Scenario: A cap binds a stretching `to` and not a constant one

- **WHEN** one item declares `at: [0, 0], to: [-0.0, 10], max_w: 30` in a frame 50 wide, and another
  declares `at: [0, 0], to: [40.0, 10], max_w: 30`
- **THEN** the first resolves to 30 and the second to 40
- **AND** neither is rejected for pairing `max_w` with `to`

#### Scenario: `fill` and a stretching `to` are the same node

- **WHEN** two otherwise identical items declare `size: [fill, 10]` and `at: [0, 0], to: [-0.0, 10]`
  in the same frame
- **THEN** both resolve to the same rectangle, because both are frame-source extents with a zero inset

#### Scenario: A right-anchored `to` is a constant

- **WHEN** an item declares `at: [-20.0, 0], to: [-0.0, 10]` on a dynamic-width label
- **THEN** its width is 20 on every resolved label width, because both corners are sign-negative and
  the two frame terms cancel
- **AND** its requirement is 20

#### Scenario: A cap binds a stretching height

- **WHEN** an item declares `size: [10, fill], max_h: 6` in a frame 20 tall
- **THEN** its resolved height is 6

#### Scenario: A hugging parent over a stretching child needs no iteration

- **WHEN** a `container` with `size: [content, 10]` and `padding: 1.0` holds a `text` with
  `size: [fill, 8]` whose laid-out width is 40
- **THEN** the child reports its intrinsic 40 upward, so the container's extent is 42 and the child
  then takes the padded inner box, which is 40
- **AND** `fill` and `content` are indistinguishable under a hugging parent: the child is drawn at its
  own intrinsic size, because that is what the parent sized itself to
- **AND** when that container's width is `fill` on a label another item sizes to 80, the child's box
  becomes 78 and its alignment gains slack

#### Scenario: A capped stretching item reports its cap, not its intrinsic

- **WHEN** a dynamic-width `single` with `width: { min: 10, max: 120 }` carries a `qr` at
  `at: [0, 0]` with `size: [fill, 10]` and `max_w: 20`, whose intrinsic width is 50
- **THEN** it reports 20 upward, so the label resolves to 20, not 50
- **AND** its box is 20 wide, so what it required and what it occupies agree

#### Scenario: A chosen size never overflows its frame

- **WHEN** a `content`-sized `qr` whose intrinsic size is 15 sits at `at: [0, 0]` in a frame 10 wide
- **THEN** its extent is 10, not 15, and it renders inside the frame

#### Scenario: A written size that does not fit is refused

- **WHEN** a template declares `at: [100, 0]` with `size: [40, 10]` on a frame 120 wide
- **THEN** it fails validation, because 140 exceeds the frame and no clamping applies

#### Scenario: A written size under a right-anchored anchor is refused when it exceeds the inset

- **WHEN** a template declares `at: [-20.0, 0]` with `size: [30, 10]`
- **THEN** it fails validation, because the box runs `[F − 20, F + 10]` and fits no frame extent
- **AND** the same item with `size: [20, 10]` is accepted

#### Scenario: An authored extent is never measured

- **WHEN** an `image` declares `size: [20, 10]` and its SVG carries no usable dimensions
- **THEN** it renders, because nothing asked for its intrinsic size

#### Scenario: An empty value collapses a chosen extent to zero

- **WHEN** a `content`-width `text` bound to an empty data value renders
- **THEN** its box is zero wide and the render succeeds

#### Scenario: An inverted `to` reports inversion, not an invalid size

- **WHEN** a stretching `to` is valid against `width.max` at load and its corners invert against a
  smaller resolved label width at render
- **THEN** the render fails with reason `edge_rect_inverted`, not `size_invalid`

#### Scenario: A statically degenerate `to` is refused at load

- **WHEN** a template declares `at: [10, 4], to: [10, 9]`, so the box's width resolves to zero for
  every frame extent and every request
- **THEN** it fails validation and is quarantined, as a malformed placement
- **AND** this is distinct from a box whose extent collapses to zero only for a particular request's
  data, which renders empty

#### Scenario: A written zero is refused where it is written

- **WHEN** a template declares `size: [0, 10]`
- **THEN** it fails validation at load and is quarantined, surfacing as `TemplateInvalid` with reason
  `template_validation_failed`
- **AND** a template declaring `size: ["{box_w}", 10]` with `box_w` defaulting to 10 loads, and a
  request supplying `box_w: 0` fails with `UnsupportedLayoutItem` reason `size_invalid`

#### Scenario: An item with no anchor is offered the whole frame

- **WHEN** a packed child of a flow container whose padded inner box is 30 by 10 declares
  `size: [fill, 4]`
- **THEN** its available width is 30, so its box is 30 wide
- **AND** the same child declaring `size: [content, 4]` resolves to `min(intrinsic, 30)`
- **AND** neither result depends on where the arrangement puts it

#### Scenario: A frame extent reports its content and takes the frame

- **WHEN** a dynamic-width `single` with `width: { min: 10, max: 120 }` carries one `fill`-width
  `text` at `at: [0, 0]` whose laid-out width is 44, and a `qr` at `at: [50, 0]` with
  `size: [content, content]` whose intrinsic width is 15
- **THEN** the text reports 44 upward, so the label resolves to 65
- **AND** the text then takes the frame, so its box is 65 wide and its alignment has slack

### Requirement: An intrinsic size is a content extent times a scale

A node's intrinsic size on an axis SHALL be its content's extent in the content's own units,
multiplied by the **scale**, which is the size of one content unit expressed in the template `unit`.
Scale is therefore template-units-per-content-unit, not a resolution: for content measured in device
pixels it is `1/dpi` on a template whose `unit` is `in`, and `25.4/dpi` on one whose `unit` is `mm`.
Text metrics are measured in points and SHALL likewise be converted to the template unit: divide by
72 for `in`, or multiply by `25.4/72` for `mm`.
An SVG's absolute `width`/`height` carry their own units and SHALL be converted to the template
`unit` when they differ. A unitless absolute dimension or one expressed in `px` SHALL use the same
one-device-pixel scale as a `viewBox` extent. A percentage or font-relative dimension is not absolute
and SHALL fall through to that axis's `viewBox`; without one, the axis has no extent. A node has an
intrinsic size when both terms are determinable, and does not when either is missing. Item type does
not enter into the resolution rule once those terms have been supplied.

| Item | Extent | Scale |
| --- | --- | --- |
| `text` | glyph advances and the **emitted** line count, from the layout below | `font_size` in points, converted to the template unit, required |
| `qr` | the module grid the payload encodes to | `module_size` |
| `image`, raster | pixel dimensions | one device pixel in template units, from the required `dpi` |
| `image`, SVG, **per axis** | that axis's absolute `width` or `height` if present, else that axis's `viewBox` extent | an absolute dimension's own unit converted to the template `unit`; a `viewBox` extent one device pixel, as for a raster |
| `container` | its children combined by its **arrangement**, plus padding | 1 |
| `line` | none: two endpoints, no box | — |

A `container` combines its children by its **arrangement**, and the two arrangements combine them
differently:

- Under the **absolute** arrangement, the default, its extent on an axis is the largest frame
  requirement among its active children on that axis. Children are placed by their own coordinates, so
  the one reaching furthest decides.
- Under the **flow** arrangement, selected by a `flow` block (`flow-layout`), its extent on an axis is
  the **assembled extent** that capability defines. Children are packed in order, so what they need
  together is not what any one of them needs.

Padding is added on each axis under both, and a rotation swaps the completed author-space pair under
both. Nothing else in this requirement distinguishes them: the children themselves are sized
identically either way, because an arrangement decides position and never extent.

A `line` is contribution-only and is never asked for an intrinsic size at all, so it is outside this
rule rather than an exception to it. Failures that prevent content from being obtained or produced
retain their existing reasons: a QR payload that cannot be encoded is `qr_generation_failed`, and a
missing/unreadable image source or invalid MIME/base64 remains the corresponding `image_*` reason.
Given content that passed those gates, there SHALL be exactly three ways for a demanded intrinsic to
be unavailable:

- **No scale.** A `qr` asks for a content or frame extent without declaring `module_size`. This SHALL
  be refused at load: `module_size` is to a QR what `font_size` is to a text, and `font_size` is
  mandatory. The engine SHALL NOT invent a module pitch.
- **No extent.** An SVG carries, for the axis being asked, neither an absolute dimension nor a
  `viewBox` extent, so it declares no content on that axis. The judgement is per axis like every other
  intrinsic: an SVG with `width="20mm"` and no `height` supplies a width to an item spelling
  `size: [content, 10]`, and only an item that also asks for its height is refused.
- **Unreadable dimension metadata.** The bytes are present and pass the checks an authored-extent
  image already passes, MIME or extension and base64 decoding, but their dimensions header cannot be
  parsed as the format they claim, so no extent can be read from them. This is not a full-image
  validation: corruption after readable dimensions SHALL pass sizing and retain today's later
  `typst_compile_failed` outcome. An authored-extent image never has even its dimensions inspected for
  sizing and reaches the renderer regardless, which is unchanged. A `src`-bound and a `name`-bound
  image SHALL behave identically here.

The three differ in **when** they are caught, and the difference is the load/render boundary this
capability already draws. A missing scale is a property of the template alone, so it SHALL be refused
at load and the template quarantined. The other two depend on bytes a request supplies, or on a
`src`-bound asset read at render, so they SHALL fail at render with reason
`intrinsic_size_undefined`, which names the outcome, an intrinsic that was demanded and could not be
produced, rather than either cause. They SHALL NOT be refused at load: a `name`-bound image has no
bytes until a request supplies them, so refusing a `src`-bound one at load would make the two sources
diverge for no gain.

`fit` decides how an image is drawn inside its resolved box and SHALL NOT affect its intrinsic size.

`module_size` SHALL be a length in the template `unit` giving the pitch of one module, and
`quiet_zone` a count of modules defaulting to zero, which need not be a whole number since it is
consumed as a length multiple. A QR's intrinsic size is `(modules + 2 × quiet_zone) × module_size`.

The generated SVG SHALL carry the requested margin, not a boolean approximation of it. The current
generator asks the encoder for its own quiet zone when the value merely exceeds zero, which yields
four modules whatever the author wrote; the symbol SHALL instead be generated with no encoder quiet
zone and its canvas expanded by `quiet_zone` **modules of the symbol's own grid** on each side.

The SVG is unitless and is drawn `fit: contain` into whatever box the item resolves to, so the margin
is expressed in grid units and needs no length. This is what keeps `quiet_zone` meaningful on a QR
that never asks for an intrinsic size: a numeric or constant-`to` QR may set `quiet_zone` without
setting `module_size`, and its margin is still `quiet_zone` modules of the grid. `module_size` enters
only the intrinsic-size arithmetic above, which such an item does not reach. This supplies the
complete post-change meanings of the fields named in the frozen `docs/SPEC.md` §4.1 `qr` clause and
supersedes that clause to this extent. The current implementation treats `module_size` as a minimum
generated-SVG pixel pitch per module and reads `quiet_zone` only for whether it exceeds zero, in which
case the encoder's own four-module zone applies. A `quiet_zone: 0.0` keeps its present meaning; a
positive one becomes that many modules rather than four. No template in the catalog sets either
field.

This requirement supersedes the frozen `docs/SPEC.md` §4 clause "`qr` and `image` have no fallback at
all: neither has a natural content footprint to shrink to", and the §4.1 `qr` and `image` clauses so
far as they bear on size. It delivers the capability deferred in ADR-0051 §11 and tracked as
[#149](https://github.com/pfa230/labeler/issues/149), which is closed as superseded.

#### Scenario: A QR without a declared pitch is refused

- **WHEN** a template declares a `qr` with `size: [content, content]` and no `module_size`
- **THEN** it fails validation and is quarantined, naming `module_size`
- **AND** the same `qr` with `size: [15, 15]` is accepted, because an authored extent needs no scale

#### Scenario: A QR sizes a label to itself

- **WHEN** a dynamic-width `single` carries only a `qr` at `at: [0, 0]` with
  `size: [content, content]`, `module_size: 0.6` and `quiet_zone: 0`, whose payload encodes to 25
  modules
- **THEN** its intrinsic size is 15 by 15 and the label is 15 wide
- **AND** the same item with `quiet_zone: 2` is `(25 + 4) × 0.6 = 17.4`

#### Scenario: The emitted symbol carries the requested margin

- **WHEN** a `qr` declares `quiet_zone: 2`
- **THEN** the generated SVG's canvas is the symbol's module grid expanded by 2 modules on each side
- **AND** an implementation retaining today's generator, which asks the encoder for its own quiet zone
  whenever the value exceeds zero, emits 4 modules and fails this scenario
- **AND** a `qr` with `size: [15, 15]`, `quiet_zone: 2` and no `module_size` is accepted and emits the
  same 2-module margin, because the margin is grid units and needs no length

#### Scenario: An image hugs its own pixels

- **WHEN** an `image` with `size: [content, content]` carries a 300 by 150 pixel PNG on a template
  with `unit: in` and `dpi: 300`
- **THEN** it is drawn 1.0 by 0.5

#### Scenario: An SVG supplies its extent from absolute units or a viewBox

- **WHEN** a `content`-sized `image` carries an SVG whose `width` and `height` are `20mm` and `10mm`
  on a template with `unit: mm`
- **THEN** it is drawn 20 by 10
- **AND** the same item carrying an SVG with no `width`/`height` but `viewBox="0 0 300 150"` on a
  template with `unit: in` and `dpi: 300` is drawn 1.0 by 0.5
- **AND** an SVG declaring `width="1in"` and `height="0.5in"` on a template with `unit: mm` is drawn
  25.4 by 12.7, its units converted rather than its numbers copied
- **AND** an SVG declaring `width="20mm"` and no `height`, in an item spelling `size: [content, 10]`
  on a `mm` template, is drawn 20 by 10: the demanded axis has what it needs and the absent one is
  never asked
- **AND** a unitless `width="300"` or `width="300px"` on an `in`, 300-dpi template supplies 1.0,
  while `width="50%"` is ignored in favour of the `viewBox` width

#### Scenario: Unreadable image bytes fail the same way

- **WHEN** a `content`-sized `image` carries valid base64 labelled `image/png` whose dimensions header
  cannot be parsed as a PNG
- **THEN** the render fails with reason `intrinsic_size_undefined`
- **AND** a `src`-bound image with the same malformed content fails identically
- **AND** the same bytes in an item with `size: [20, 10]` are not inspected for their size, so the
  sizing path raises nothing; the renderer still parses them when it compiles the page, and that
  failure surfaces as `RenderFailed` / `typst_compile_failed`, exactly as it does today
- **AND** a content-sized PNG whose dimensions header is readable but whose later pixel data is
  corrupt also reaches the renderer and fails as `typst_compile_failed`, because sizing is header-only

#### Scenario: An SVG that declares no content fails at render

- **WHEN** a `content`-sized `image` carries an SVG with neither absolute dimensions nor a `viewBox`
- **THEN** the template loads and is served
- **AND** each render of it fails with `UnsupportedLayoutItem` reason `intrinsic_size_undefined`

#### Scenario: A container's arrangement decides the combination

- **WHEN** a `content`-width `container` with no padding holds two children each resolving to 10 wide,
  the first at `at: [0, 0]` and the second at `at: [4, 0]`
- **THEN** its intrinsic width is 14, the largest of the two requirements
- **AND** the same two children as packed children of a `row` flow container with `gap: 0` give it an
  intrinsic width of 20, because a flow container assembles what its children need together

#### Scenario: A line has no intrinsic size

- **WHEN** any item asks a `line` for an intrinsic size
- **THEN** there is none to give: a line contributes through its endpoints, by the next requirement
- **AND** an implementation modelling measured nodes uniformly must keep "no intrinsic size"
  distinguishable from "an intrinsic size of zero"

### Requirement: An item requires of its frame the smallest extent that contains it

Every resolved coordinate SHALL impose one requirement on its frame. An item with no coordinate
imposes none, and requires its claim alone.

| A resolved coordinate | Requires |
| --- | --- |
| non-negative, value `v` | `frame ≥ v` |
| sign-negative, inset `a` | `frame ≥ a` |

An item's **requirement** on an axis SHALL be the largest of the requirements imposed by each of its
own resolved extremes, and, when its extent is a frame source, the frame its claim needs in order to
fit (`at + claim + inset`). Writing `a` and `b` for the insets of a sign-negative `at` and `to`, the
six placement spellings, the `line` and the packed child fall out:

| Placement | Requirement | Why |
| --- | --- | --- |
| `size`, `at` non-negative | `at + claim` | the far edge is `at + claim` |
| `size`, `at` sign-negative | `a` | only the low edge binds; `claim ≤ a − inset` is a check, not a requirement |
| `to`, both non-negative | `to` | the far corner is a plain coordinate |
| `to`, both sign-negative | `a` | both corners track the frame; only the low edge binds |
| `to`, `at` non-negative and `to` sign-negative | `at + claim + b` | the extent `F − b − at` must reach the claim |
| `to`, `at` sign-negative and `to` non-negative | `max(a, to)` | the low edge needs `a`, and the far corner is a plain `to` |
| `line` | the larger of its two endpoints' requirements | two endpoints, no box between them |
| a packed child | `claim` | no anchor, so no term to add to it |

The last row of the box table is the one that does not simplify to the anchor's inset: its far corner
does not track the frame, so it imposes its own plain requirement alongside the anchor's. That
spelling is permitted only on a resolved axis, so its requirement never sizes a frame, but it is
still bounds-checked against the frame it has.

**Claim** is what the item offers as its extent for this purpose: its resolved extent for an author
or content source, and `min(intrinsic, max_w/max_h, available extent)` for a frame source. That is
the same bounded report-upward rule the first requirement states, applied here, and the bound is what
keeps a frame-source item from requiring more than it will occupy.

A sign-negative anchor whose far edge also tracks the frame imposes no requirement beyond its own
inset, because that edge can never pass the frame's: an authored extent wider than the available
extent was already refused, and a content or frame extent is clamped to it. This is ADR-0051 §4's
"clause 1" outcome, and ADR-0051 §8's separate restatement for containers, both now derived rather
than asserted.

On a dynamic-width `single` (`format.width: { min, max }`) the label's width SHALL be the largest
requirement among the top-level items, clamped into `[width.min, width.max]`. Two properties follow
and are worth stating because each has a different reason:

- **No requirement exceeds `width.max`.** A content or frame claim is clamped by the available extent
  against a frame of `width.max`. An authored extent is not clamped at all, and is instead refused at
  load when it does not fit `width.max`, the widest the label can ever be.
- **No item's box is smaller than the claim it was laid out at.** The label is the maximum of the
  requirements, and each requirement is by definition the smallest frame the item fits in.

An item's requirement SHALL NOT depend on the position of any other item in the list. Sizes SHALL be
exchanged per node and keyed by node, never by consuming a positional list in traversal order, so
there is no `auto_length_cursor_mismatch` failure to raise. This holds for a packed child too, and is
worth saying because it is the one item whose **position** does depend on its siblings: what it
requires is still its own claim, and its container combines those requirements without any of them
having consulted another.

This requirement supersedes the frozen `docs/SPEC.md` §6 paragraphs beginning "On a dynamic-width
`single` the final width is not known until the measure pass runs" and "A `to`-sized `qr` or `image`
is the exception", the §6 sentence "A right-anchored `at.x` cannot be combined with an `auto` or
frame-dependent width on a dynamic-width template", and the §3.1 auto-length paragraph so far as it
concerns which width an item contributes.

#### Scenario: A box requires its far edge

- **WHEN** a `text` at `at: [10, 0]` declares `size: [40, 10]` and its laid-out text is 25 wide
- **THEN** its requirement is 50, not 35, so a dynamic label resolves to 50 and the box fits

#### Scenario: A right-anchored item requires only its inset

- **WHEN** a dynamic-width `single` with `width: { min: 10, max: 120 }` carries a `text` at
  `at: [-40.0, 0]` with `size: [content, 10]` whose text would lay out 30 wide unbounded
- **THEN** its available extent is 40, so it lays out at 30 and its claim is 30
- **AND** its requirement is 40, so the label is 40 wide and its box runs from x = 0 to x = 30

#### Scenario: `fill` under a right-anchored anchor is its inset

- **WHEN** an item declares `at: [-12.0, 0]` with `size: [fill, 10]` in any frame at least 12 wide
- **THEN** its extent is 12 on every frame width, and its requirement is 12

#### Scenario: A stretching `to` requires its margin as well as its content

- **WHEN** a dynamic-width label carries a `text` at `at: [0, 0]` with `to: [-2.0, 10]` whose
  laid-out width is 30
- **THEN** its requirement is 32 and the label resolves to 32
- **AND** its box is 30 wide, leaving the 2 units it reserved

#### Scenario: A line requires its furthest endpoint

- **WHEN** a `line` runs from `at: [10, 4]` to `to: [20, 4]`
- **THEN** its requirement is 20
- **AND** a line from `at: [10, 4]` to `to: [-0.0, 4]` requires 10, and on a label that resolves to
  exactly 10 both endpoints meet, failing at render with reason `line_degenerate`

#### Scenario: A packed child requires its claim

- **WHEN** a packed child of a `row` flow container resolves to 12 wide
- **THEN** its requirement on that container's padded inner box is 12, with no anchor term
- **AND** it is 12 whether the child is packed first, last, or in the middle

#### Scenario: Requirements compose through containers

- **WHEN** a `container` at `at: [5, 0]` with `size: [content, 10]` and `padding: 1.0` holds a
  `container` at `at: [2, 0]` with `size: [content, 8]` holding a `text` at `at: [3, 0]` with
  `size: [7, 6]`
- **THEN** the inner container's intrinsic width is 3 + 7 = 10, the outer's is 1 + 2 + 10 + 1 = 14,
  and its requirement is 5 + 14 = 19

#### Scenario: The vertical axis works the same way

- **WHEN** a `container` with `size: [20, content]` holds a `text` at `at: [0, 6]` with
  `size: [10, 4]`, a `text` at `at: [0, -8.0]` with `size: [10, 5]`, and a `line` from `at: [1, 2]`
  to `to: [1, 9]`
- **THEN** the three requirements are 10, 8 and 9, so the container's intrinsic height is 10
- **AND** an implementation that ignored child offsets would give 5

#### Scenario: Reordering the layout does not change the result

- **WHEN** the items of an **absolutely arranged** list on a dynamic-width label are reordered without
  changing any of them
- **THEN** the rendered label is identical apart from draw order
- **AND** this is a property of the absolute arrangement, not of layout in general: a flow container's
  `items` are packed in the order they are written, so reordering them moves the children
  (`flow-layout`)

### Requirement: A frame's axis is resolved unless something inside it decides its size

A frame's axis SHALL be **resolved** when its extent is known before the items inside it are sized.
It SHALL be resolved when the item establishing it has an authored extent on that axis; when the
extent is a frame source under a **sign-negative anchor**, whose available extent is `a − inset` and
therefore frame-independent; and otherwise when the enclosing frame's axis is resolved and the
extent's source is not `content`. A `content` axis is never resolved, because its extent is derived
from the very children being sized.

An item with **no anchor**, which is a packed child (`flow-layout`), has no sign-negative anchor, so
the second clause cannot reach it and the third decides it: its axis is resolved when its enclosing
frame's axis is resolved and its extent's source is not `content`. This is stated rather than left to
the reader to derive from an absence, and it matters for a packed `container`, whose own children read
the state it establishes. The rule that consults this state is unaffected by any of it: a shrinking
`to` is a `to`, a packed child may carry no `to`, so that rule can never be reached through one.

The second clause matters and is easy to miss. A container at `at: [-40, 0]` with `size: [fill, 20]`
is 40 wide on a dynamic-width page before any child is sized, because the frame terms cancel, so its
inner axis is resolved and a shrinking `to` beneath it is legal. Treating every frame source under an
unresolved parent as unresolved would refuse that, which is the coarse syntactic judgement
`width_is_frame_dependent` makes today and which this change exists to replace.

The page's height axis SHALL always be resolved. Its width axis SHALL be resolved on a `sheet` and on
a fixed-width `single`, and unresolved on a dynamic-width `single`.

A `container` with `rotate: 90` or `270` SHALL swap the two axes' resolved state along with the
canvas, so an unresolved physical width becomes an unresolved author height. `rotate: 180` swaps
nothing.

This state is deliberately **conservative**: a frame-source axis carrying a cap that binds at every
possible frame extent is constant in fact, and is still treated as unresolved, because deciding
otherwise means carrying an interval per axis and proving the cap binds across the whole of
`[width.min, width.max]`. The refusal costs an author nothing, since a container whose extent is
always the same number can spell it as that number.

**The only rule that consults it.** A `to` with a sign-negative `at` and a non-negative `to` resolves
to `to − at − F`: it grows *narrower* as the frame grows, has no claim that could size a frame, and
inverts once `F` exceeds `to − at`. It SHALL be permitted only on a resolved axis, where the frame is
a constant before the item is sized, and SHALL be refused with `TemplateInvalid` otherwise. Where
permitted it is an authored extent in every respect: its extent is the corner subtraction, `max_*` is
inert on it, and it demands no intrinsic size.

#### Scenario: A packed container takes its state from its enclosing frame

- **WHEN** a packed `container` with `size: [fill, 20]` sits in a flow container whose own width axis
  is resolved, and holds a child declaring `at: [-10.0, 0], to: [30.0, 5]`
- **THEN** the template is accepted: the packed container has no anchor, so its width is resolved
  because its enclosing frame's is and its source is not `content`
- **AND** the same packed container spelling `size: [content, 20]` leaves that axis unresolved, so the
  same child is refused

#### Scenario: A shrinking `to` is refused on a dynamic width

- **WHEN** an item declares `at: [-20.0, 0], to: [90.0, 10]` on a dynamic-width `single`
- **THEN** the template fails validation, and the message says the extent shrinks as the label grows
- **AND** the same item on a fixed-width `single` 100 wide resolves to 10, since `at` resolves to 80

#### Scenario: A shrinking `to` still requires its far corner

- **WHEN** a fixed-width `single` 100 wide carries an item declaring `at: [-20.0, 0], to: [90.0, 10]`
- **THEN** its corners resolve to 80 and 90, so its extent is 10
- **AND** its requirement is `max(20, 90) = 90`, not 20: the far corner is a plain coordinate and
  must lie inside the frame
- **AND** the same item on a frame 80 wide is refused, because 90 does not fit

#### Scenario: A right-anchored stretching container has a resolved inner axis

- **WHEN** a dynamic-width `single` carries a `container` at `at: [-40.0, 0]` with `size: [fill, 20]`
  holding a child declaring `at: [-20.0, 0], to: [30.0, 10]`
- **THEN** the template is accepted: the container is 40 wide before any child is sized, because the
  frame terms cancel, so its inner axis is resolved
- **AND** the same child inside a container at `at: [0, 0]` with `size: [fill, 20]` is refused

#### Scenario: A hugging container leaves its inner axis unresolved

- **WHEN** a `container` with `size: [content, 20]` on a fixed-width label holds an item declaring
  `at: [-10.0, 0], to: [30.0, 5]`
- **THEN** the template fails validation, because the container's inner width comes from its children

#### Scenario: A quarter turn carries the state to the other axis

- **WHEN** a top-level `container` with `rotate: 90` and `size: [fill, 20]` on a dynamic-width
  `single` holds a child declaring `at: [0, -20.0], to: [5, 90.0]`
- **THEN** the template fails validation, because the author canvas's height takes its state from the
  container's physical width, which the label is still solving for
- **AND** `rotate: 270` fails identically, while `size: [40, 20]` is accepted

#### Scenario: 180 degrees does not swap the state

- **WHEN** a top-level `container` with `rotate: 180` and `size: [fill, 20]` on a dynamic-width
  `single` holds a child declaring `at: [0, -8.0], to: [5, 15.0]`
- **THEN** the template is accepted, because the height axis is resolved and 180 leaves it there
- **AND** the same child declaring `at: [-8.0, 0], to: [15.0, 5]` is refused

### Requirement: A container establishes a padded frame, and rotation swaps it

A `container`'s children SHALL be sized against its **padded inner box**, its resolved extent less
its padding, clamped at zero on each axis. Where they are then placed SHALL be decided by the
container's **arrangement**: by each child's own coordinates under the **absolute** arrangement, the
default, and by packing them in order under the **flow** arrangement, which a `flow` block selects
(`flow-layout`). Sizing is identical under both, and this requirement's rules below govern both. A zero inner box SHALL render an empty container, whether
or not it has active children.

That rule governs the **container**, and nothing else. Every child retains every rule that applies to
it in the zero frame it was given: authored extents are still checked, bounds are still enforced,
intrinsics are still demanded where an axis asks, `line` endpoints must still differ, and a `text`
still enforces its `overflow` policy. Several of those can fail there, and none is suppressed:

- an active `text` with non-empty content is the extreme case of "cannot fit however short", so it
  raises `text_does_not_fit` under either policy;
- an active item with an **authored** extent that does not fit, such as `size: [1, 1]` in a zero-wide
  inner box, fails the authored-extent check exactly as it would in any other frame;
- an active `line` fails its bounds or degeneracy checks if its endpoints require room the frame does
  not have.

What renders empty is a child whose own extent resolves to zero, which is what a content or frame
source does in a zero frame. There is no precedence rule between the container and its children: the
container renders, and each child's own contract decides the rest. A padding that meets or exceeds a **constant** box SHALL be
refused at load, where nothing a request can supply could change the outcome. There is no
render-time `container_padding_no_room`.

A `container` that gives neither `size` nor `to` SHALL default to `size: [fill, fill]`, the
resolution its previous `[auto, auto]` default produced on every format.

This requirement supersedes the frozen `docs/SPEC.md` §4.2 in full and carries the complete
post-change rotation contract. Everything in that section SHALL continue to hold except its final
bullet:

- **Container-only, orthogonal.** `rotate` is valid only on a `container` and must be a multiple of
  90 degrees (`{0, 90, 180, 270}`, normalised via `rem_euclid(360)` within a small tolerance). Any
  `rotate` on another item type, or a non-orthogonal value, is a validation error.
- **Counter-clockwise.** Author canvas corners map to physical box corners as R90 BL→BR, BR→TR,
  TR→TL, TL→BL; R180 BL→TR and so on; R270 BL→TL and so on.
- **`at` and its extent stay parent-frame.** A rotated container is placed and bounds-checked exactly
  like an unrotated one. Rotation is an inner transform, so nested rotated containers compose without
  compounding coordinate flips.
- **The inner author canvas swaps for 90 and 270.** Children are authored in the container's natural
  reading orientation; the inner authoring box and child bounds swap to `[inner_h, inner_w]`. Padding
  is author-space and rotates with the design. The container's own paint, its `stroke` and its
  `background` (`shape-paint`), is not rotated: it is painted on the container's box in its parent's
  frame at every rotation.

The final bullet, "No `auto` under rotation", is **replaced**. Sizes SHALL compose through the swap:
a rotated container SHALL compute its intrinsic size in **author space** by the ordinary container
rule, padding plus its children combined on each author axis by whichever arrangement it carries, and
SHALL then swap the resulting pair to obtain its intrinsic size in its parent's physical frame. It is the completed
author-space aggregate that swaps, not the children's raw intrinsic sizes: author-space offsets,
`line` requirements, a flow container's author-space packing direction and author-space padding are
inside the aggregate and survive the swap with it.
`content` and `fill` SHALL be permitted anywhere beneath a rotated container and on the rotated
container itself, and the intrinsic pass SHALL recurse into it in author space.

ADR-0036 is amended accordingly: its §5 is retired and the rest stands.

#### Scenario: A zero inner box renders an empty container

- **WHEN** a container's resolved width equals its horizontal padding and its active children are a
  `line`, an `image` and a `text` bound to an empty value
- **THEN** it renders as an empty box and the render succeeds
- **AND** a template declaring `size: [10, 10]` with `padding: 6.0` fails validation instead

#### Scenario: An authored child in a zero box fails its own check

- **WHEN** that same container holds an active `image` with `size: [1, 1]`
- **THEN** the render fails the authored-extent check, because 1 does not fit a zero-wide inner box
- **AND** an active `line` whose endpoints need room the inner box lacks fails its own bounds check

#### Scenario: A text with content in a zero box still raises

- **WHEN** that same container holds an active `text` whose value is non-empty
- **THEN** the render fails with reason `text_does_not_fit`, under either `overflow` value
- **AND** the container's own rule is unchanged: it is the child's policy that raises

#### Scenario: A rotated container hugs its rotated content

- **WHEN** a `container` with `rotate: 90` and `size: [content, content]` holds a `text` with
  `size: [content, content]` whose laid-out size in author space is 40 by 6
- **THEN** its physical footprint in the parent is 6 wide by 40 tall

#### Scenario: The swap carries author-space padding and offsets

- **WHEN** a `container` with `rotate: 90`, `size: [content, content]` and `padding: 2.0` holds a
  `text` at author-space `at: [3, 1]` with `size: [10, 4]`
- **THEN** its author-space intrinsic size is 17 by 9, so its physical footprint is 9 wide by 17 tall
- **AND** an implementation swapping only the child's 10 by 4 would give 4 by 10 and fail

#### Scenario: A line inside a rotated container contributes through the swap

- **WHEN** a `container` with `rotate: 270` and `size: [content, content]` holds only a `line` from
  author-space `at: [0, 0]` to `to: [12, 0]`
- **THEN** its author-space intrinsic width is 12 and its physical intrinsic height is 12

#### Scenario: A stretching child beneath rotation takes the author canvas

- **WHEN** a `container` with `rotate: 90` and `size: [10, 40]` holds a `text` with
  `size: [fill, 6], max_w: 25`
- **THEN** the author canvas is 40 wide, and the text's author-space width is 25, its cap binding
- **AND** `max_w` names the author axis the item is authored in, not the physical one

#### Scenario: 180 degrees does not swap the canvas

- **WHEN** a `container` with `rotate: 180` and `size: [content, content]` holds a `text` whose
  laid-out size is 40 by 6
- **THEN** its footprint is 40 wide by 6 tall

### Requirement: Text is laid out against the box it will get, and what does not fit is authored

Every **active** `text` item SHALL be laid out by the steps below and SHALL have its `overflow`
policy enforced, whether or not either of its axes asks for an intrinsic size. Laying out and
demanding an intrinsic size are different things: a text in a fully authored box (`size: [40, 10]`,
fixed `font_size`) reports no intrinsic size on either axis and is still broken, fitted and checked
against its policy. Its layout is the rendered output, not a measurement.

The box a text is laid out against is known before its content is: it is the item's own extent when
that extent is authored, and the available extent, capped, when it is content or frame. On a
dynamic-width `single` the frame used for that is `format.width.max`. Nothing about line breaking
consults the page format.

1. **Break.** A value's lines are its segments at `\n`, so a value with N newlines has N+1 lines, and
   **every one of them is laid out** whatever the item's `wrap` flag says. `\r\n` is normalised to
   `\n` before this step, because `\r` is unmapped in the bundled font and would otherwise be charged
   the `.notdef` advance while rendering nothing; a lone `\r` is not a terminator (see #259).
   `wrap: true` then wraps each line to the box width, breaking at spaces and never inside a word: a
   word wider than the box stays whole on its own line, wider than the box. `wrap: false` breaks
   nothing further. A line therefore remains wider than the box when one **word** is and `wrap: true`
   was authored, which step 2 shrinks and step 3 resolves; when one **glyph** is, which no breaking
   rule can help; or when `wrap: false` was authored, which step 3 resolves.

2. **Shrink.** A `font_size` range picks the largest size in `[min, max]` at which the broken block
   fits the box height **and every line step 1 broke it into fits the box width**, in 0.5 pt steps, including the
   reservation the *Vertical fitting reserves the ink each alignment can expose* requirement
   computes, for the item's `alignment.vertical`, from the outer ink of the lines broken at that
   candidate. The text SHALL be re-broken at each candidate's glyph advances, as today's
   `largest_fitting_font` does; the emitted breaks are the ones from the selected size, not breaks
   frozen at `font_size.max`. A word too wide for the box at one candidate is therefore a reason to
   try a smaller size, not a break to place inside the word. A fixed `font_size` skips this step.
3. **Overflow.** What still does not fit is resolved by the item's `overflow` policy.
4. **Emit.** Every line produced by step 1, blank or not, gets its own line box. A blank first or last
   line is a line the caller wrote and is laid out like any other.

Blank edge lines no longer make the measured and emitted blocks differ: nothing is trimmed, so the
block step 2 chooses the size against is the block step 1 produced. Step 3 may still shorten it — the
`ellipsis` policy drops the lines that do not fit — so the contract names which block counts: the
item's **intrinsic height** SHALL be the block height of the lines emitted **after** the overflow
policy has been applied, at the size step 2 chose. What changed is that the difference is now caused
only by overflow, which is visible on the label as a marker, and never by a line silently removed for
carrying no glyphs.

This supersedes ADR-0045's blank-edge rule and the previous normative ordering of steps 2 and 4, under
which a blank edge line was counted while choosing the size and dropped when emitting. That rule
predates hard line breaks surviving at all: it was written when a non-wrapping item emitted one line,
and it is the same silent discard this capability now refuses everywhere else. A value with a blank
edge therefore occupies one more line box than it did, and may select a smaller font or gain an
overflow marker as a result.

Neither the breaks nor the size SHALL be re-decided when the item's box turns out to be larger than
the box it was laid out against. A `fill` text on a label that clamps up to `width.min` keeps the
lines and size it was laid out with, and the extra width becomes slack for `alignment.horizontal`.

An item's box SHALL be its box regardless of `alignment.horizontal`, which positions content inside
it. This supersedes `openspec/changes/archive/2026-08-21-issue-180-auto-length-text-alignment`, under
which a centred auto-width text on a dynamic-width label was given the alignment slot as its box
while a left-aligned one was given the laid-out width.

**`overflow`.** A `text` SHALL carry an `overflow` field with the values `ellipsis` (the default) and
`fail`. Both shorten nothing the other would not; they differ in when they give up:

| | `ellipsis` | `fail` |
| --- | --- | --- |
| fits as authored | render it | render it |
| fits once shortened | render the shortened form | `text_does_not_fit` |
| cannot fit however short | `text_does_not_fit` | `text_does_not_fit` |

Shortening has two independent paths. Lines are dropped from the end of the block, the block kept
being the longest leading run of lines that fits the box height in the form it will be emitted, and
the marker is appended to the last retained line; a line that is wider than the box is shortened where it sits,
whether or not anything was dropped. Either path trims characters until the line and the marker fit.
The marker reports the **field**, not the line it sits on: it is appended whenever any line was
dropped, whether that line carried glyphs or was blank, and for a dropped line it lands at the end
of the last retained line whatever that line holds. A value whose every line is shown, unshortened, carries no marker.
The shortest form it can produce across the box is the marker alone. Down the box there is no single
shortest form, because a run's height depends on the ink at its edges and dropping a line changes
which ink that is: a longer run can fit where a shorter one does not, as when the first line's deep
descender lies inside a two-line block and below a one-line block. Shortening therefore succeeds
whenever `...` fits the box width and at least one leading run of lines, each judged in the form it
would be emitted with its own reservation, fits the box height, and fails otherwise. Every run SHALL
be judged before the item is refused; no run, the first line alone included, is refused on its own
account while a longer one fits. Height is shortened by whole lines only: characters are trimmed to
meet the box width, never to shed an accent or a descender. Two cases therefore reach the third row,
and neither is a separate rule:

- the box is narrower than `...` itself, so there is nothing shorter to produce;
- no leading run of lines, from the whole block down to its first line, fits the box height at the
  chosen size in its emitted form, reservation included.

An over-wide **line** is shortened in place, wherever it sits in the block: a line wider than the
box at the chosen size is trimmed until it and the marker fit, independently of the dropped-lines
path, so the marker may sit on a middle line while a later fitting line is emitted untouched. An
over-wide **word** reaches the policy exactly like an over-wide glyph: under `wrap: true` it is
kept whole, so at the chosen size it is a line wider than the box, and step 3 shortens or refuses
it. A box can be too narrow for a word or a glyph and still wide enough for the marker, and under
`ellipsis` that case renders the shortened form with `...`; it fails only when the marker does not
fit either. Under `fail` it fails as soon as the content overflows, marker or no marker.

Clipping SHALL NOT be an outcome of the policy: a box that cannot hold the shortest representable
form of its content is an error, not a label with half a glyph on it.

The policy SHALL be evaluated against the **measured block** the *Vertical fitting reserves the ink
each alignment can expose* requirement defines: the cap-height-to-baseline metric block plus the
reservation computed from the outer ink of the lines being judged, for the item's
`alignment.vertical`, including `center`. Size selection, the line budget and the `fail` verdict SHALL each judge
that one quantity on the lines it is deciding about, so none of them can accept a block another
refuses. A `center`-aligned item whose metric block fits its box but whose
block plus reservation does not SHALL be shortened under `ellipsis` and SHALL raise
`text_does_not_fit` under `fail`, and a box that no leading run of lines fits, each run in its emitted form
with its own reservation, SHALL raise `text_does_not_fit` under either. Because the reservation
follows the emitted ink, the verdict for one box can differ between two values of the same width and
line count: a box one cap height tall holds `HELIX` and refuses `Égypt`.

A glyph the rendering font supplies is measured wherever it inks, so the ADR-0050 consequence that a
glyph outside the font's ascender/descender band may clip is superseded: `Å` and `Ǻ`, which ink above
Inter's ascender, are reserved for like any other accent. One gap remains. A character the rendering
font does not map is drawn by Typst from a fallback font the measurement does not read, so its
position is measured from the rendering font's own missing-glyph outline rather than from what is
drawn, and its ink may clip. Handling it is #254's scope, not this requirement's.

This requirement supersedes the frozen `docs/SPEC.md` §3.1 bullet "Blank first/last lines are dropped
before rendering, so a leading or trailing newline does not push the visible text off centre; interior
blank lines are kept as spacing.", which the blank-line rule above replaces in full. It also supersedes
the frozen `docs/SPEC.md` §3.1 sentence "If the content still overflows
at `font_size.min`, the fitting lines are kept and the last is ellipsized" and its multiline wrap
paragraph, and the §4.1 clause "A range auto-shrinks the text to fit the box (0.5pt steps) and
truncates with an ellipsis if it still overflows", generalising both to every format and every
`font_size` spelling.

#### Scenario: A fully authored text is still laid out and still enforces its policy

- **WHEN** a `text` declares `size: [40, 10]` with a fixed `font_size` and `overflow: fail`, so
  neither axis asks for an intrinsic size, and its value does not fit
- **THEN** the render fails with reason `text_does_not_fit`
- **AND** with `overflow: ellipsis` it is broken and ellipsized to the 40 by 10 box, rather than
  emitted unfitted for the renderer to clip, which is what happens today

#### Scenario: A long word is split, not overflowed

- **WHEN** a `wrap: true` `text` carries a single word far wider than its box, and the box is
  tall enough for the resulting lines
- **THEN** the word is not split: it stays whole on one line, step 2 spends the `font_size` range
  on it, and whatever still does not fit at the floor is resolved by the item's `overflow` policy
- **AND** this scenario keeps its name from the superseded version, where the word was split
  character by character and neither policy was consulted

#### Scenario: An over-wide word shrinks whole instead of breaking

- **WHEN** a `wrap: true` `text` with `font_size: { min, max }` carries a value containing a word
  too wide for its box at `max` but fitting whole at some size in the range, in a box tall enough
  for the resulting lines
- **THEN** it renders that word whole, on one line, at the largest such size
- **AND** an implementation retaining the character-chunking loop accepts the first size whose
  height works and emits the word split across lines, and fails this scenario

#### Scenario: An over-wide word at the floor is shortened when the marker still fits

- **WHEN** a `wrap: true` `text` with `font_size: { min, max }` and `overflow: ellipsis` carries a
  word still wider than its box at `min`, in a box still wider than `...` at that size
- **THEN** it renders the shortened form with the `...` marker, because a shortened form exists
- **AND** the same item with `overflow: fail` fails with reason `text_does_not_fit`

#### Scenario: An over-wide word at a fixed size takes the overflow outcome

- **WHEN** a `wrap: true` `text` with a fixed `font_size` and `overflow: ellipsis` carries a word
  wider than its box, in a box still wider than `...` at that size
- **THEN** it renders the shortened form with the `...` marker rather than splitting the word
- **AND** the same item with `overflow: fail` fails with reason `text_does_not_fit`
- **AND** the same item with `overflow: ellipsis` in a box narrower than `...` fails with reason
  `text_does_not_fit`, because no shortened form exists

#### Scenario: No emitted line is ever a mid-word fragment without a marker

- **WHEN** any `wrap: true` `text` renders any value under either `overflow` policy
- **THEN** every emitted line carrying glyphs is either a whole word, words joined by single
  spaces, or a line carrying the `...` marker; a blank line carries no glyphs and no marker
- **AND** a mid-word fragment with no hyphen and no marker never appears

#### Scenario: An over-wide line is shortened where it sits, not only at the end

- **WHEN** a `wrap: true` `text` with `overflow: ellipsis` and a fixed `font_size` carries a
  value whose first line is wider than its box while its last line fits as authored, with no
  line dropped off the end of the block
- **THEN** the over-wide line is trimmed until it and the marker fit, and the fitting last line
  is emitted untouched
- **AND** the marker therefore sits mid-block rather than at the end of the last retained line

#### Scenario: An over-wide glyph is shortened when the marker still fits

- **WHEN** a `wrap: true` `text` with `overflow: ellipsis` and a fixed `font_size` carries a
  glyph wider than its box, in a box still wider than `...` at that size
- **THEN** it renders as `...`, because a shortened form exists
- **AND** the same item with `overflow: fail` fails with reason `text_does_not_fit`

#### Scenario: A box narrower than the marker cannot be shortened

- **WHEN** the same `ellipsis` item sits in a box narrower than `...` at its chosen size
- **THEN** the render fails with reason `text_does_not_fit`, because no shortened form exists
- **AND** no clipped marker is emitted

#### Scenario: A box too short for one line cannot be shortened

- **WHEN** a `text` with a fixed `font_size` of 20 sits in a box 40 wide and 2 units tall
- **THEN** the render fails with reason `text_does_not_fit` under either policy

#### Scenario: A hugging text never shortens on its own account

- **WHEN** a `content`-width `text` with no `max_w` renders in a frame wide enough for its available
  extent
- **THEN** it is laid out at its natural width with no truncation, because its box is its content
- **AND** shortening applies only when a cap or the available extent binds

#### Scenario: An empty value in a zero-width box is not an overflow

- **WHEN** a `content`-width `text` bound to an empty value resolves to a zero-wide box
- **THEN** it renders empty and no error is raised, because there is no content to shorten

#### Scenario: Shrinking happens before the policy

- **WHEN** a `text` with `font_size: { min: 8, max: 20 }` and `overflow: fail` carries a value that
  does not fit at 20 but fits at 12
- **THEN** it renders at 12 and no error is raised

#### Scenario: A leading blank line shrinks the chosen font

- **WHEN** a `wrap: true` `text` with `font_size: { min: 8, max: 20 }` receives a value of one blank
  line followed by **two** non-blank lines that need no wrapping, in a box tall enough for exactly two
  lines at 20 pt and for three at 14 pt
- **THEN** the block is three line boxes while the size is chosen, so 20 pt does not fit and 14 pt is
  selected
- **AND** all three are emitted at that size, the blank one included, so the visible text sits one line
  lower than the same value without the leading newline
- **AND** no overflow marker is added, because no line was dropped

#### Scenario: A hugging parent hugs the emitted lines, not the trimmed ones

- **WHEN** a `container` with `size: [20, content]` holds a `wrap: true` `text` with a fixed
  `font_size` whose value is a blank line followed by two non-blank lines
- **THEN** the container's intrinsic height is the block height of **three** lines
- **AND** it is the same count the font size was chosen against, because no trimmed lines remain for it
  to differ from: this scenario keeps its name from the superseded version, where those counts were two
  and three

#### Scenario: A hard line break survives when wrapping is off

- **WHEN** a `wrap: false` `text` receives a value of two non-blank lines, in a box tall enough for
  both and wide enough for each, so neither shrinking nor the overflow policy has anything to resolve
- **THEN** both lines are emitted, in order, and neither is broken at the box width
- **AND** an implementation keeping only the first input line would emit one

#### Scenario: An empty value is one empty line

- **WHEN** a `text` receives an empty value in a box that holds at least one line
- **THEN** it is laid out as one line carrying no glyphs, and its intrinsic height is one line box:
  one cap height, with no reservation, because a line carrying no glyphs carries no ink
- **AND** its intrinsic width is zero, so a hugging parent reserves height without reserving width

#### Scenario: A whitespace-only line keeps its line box

- **WHEN** a `wrap: true` `text` receives a value whose second of three lines contains only spaces
- **THEN** three line boxes are laid out, the middle carrying no glyphs, and wrapping does not collapse
  it away

#### Scenario: A dropped trailing blank earns the marker

- **WHEN** a `wrap: false` `text` with `overflow: ellipsis` receives `"message\n"` in a box tall enough
  for one line and wide enough for `message` but not for `message...`
- **THEN** the blank line does not fit and is dropped
- **AND** the emitted form carries the marker and fits the box width, so at least one character of
  `message` is removed to make room, rather than the label claiming the value was shown in full

#### Scenario: A dropped blank leaves the marker on a line with no glyphs

- **WHEN** a `wrap: false` `text` with `overflow: ellipsis` receives `"\nmessage"` in a box that holds
  one line and is at least as wide as `...`
- **THEN** the retained line is the blank one and `message` is dropped
- **AND** that line carries the marker, so the label reads `...`
- **AND** in a box narrower than `...` the same value is `text_does_not_fit` instead, under the
  unchanged rule that a box which cannot hold the shortest representable form is an error

#### Scenario: CRLF costs no width

- **WHEN** a `text` receives `"abc\r\nabc"`
- **THEN** it is laid out identically to `"abc\nabc"`, in line count, chosen size and intrinsic width
- **AND** no `\r` reaches measurement, where it would be charged the `.notdef` advance

#### Scenario: Alignment does not change the box

- **WHEN** the same `content`-width `text` renders once with `alignment.horizontal: left` and once
  with `center`
- **THEN** both are drawn into a box of the laid-out text width, and neither gets the frame remainder

#### Scenario: Centring is authored

- **WHEN** a `text` declares `size: [fill, 16.1]` with `alignment.horizontal: center` on a
  dynamic-width label clamped to `width.min`
- **THEN** its box spans the frame remaining from its anchor, and the text is centred within it

#### Scenario: The policy is independent of the format

- **WHEN** the same item and value are placed on a fixed-width `single`, a `sheet` slot, and an
  auto-length `single` clamped to `width.max`, each with the same resolved box
- **THEN** all three produce the same lines and the same overflow outcome

#### Scenario: A centred item that overflows only by its ink is an overflow

- **WHEN** a `center`-aligned `text` with `overflow: fail` and a fixed `font_size` carries a value
  whose metric block fits its box height but whose block plus reservation does not
- **THEN** the render fails with reason `text_does_not_fit`
- **AND** the same item with `overflow: ellipsis` drops or shortens lines until the block plus its
  reservation fits, or fails if no such form exists

#### Scenario: A longer run fits where the first line alone does not

- **WHEN** a `center`-aligned `wrap: false` `text` with a fixed `font_size` of 20, `line_spacing: 0.5`, `overflow: ellipsis` and a box 28 pt tall and wide enough for every line with the marker renders `g\u0323\nHELIX\nHELIX` (a `g` carrying a combining dot below)
- **THEN** it keeps two lines, the second ending in `...`: the three-line block needs about 34.55 pt, the two-line emitted form about 24.83 pt, because the first line's dot lies inside it, and the one-line form about 31.45 pt, because centring it must reserve twice the dot's depth
- **AND** the render is not refused, although the first line alone does not fit
- **AND** its ink is contained, as the *Vertical fitting reserves the ink each alignment can expose* requirement defines containment

#### Scenario: A box's verdict follows the ink the value carries

- **WHEN** a `top`-aligned `text` with a fixed `font_size` of 20, `overflow: fail` and a box as wide
  as either value needs and exactly `cap_height(20 pt)` tall renders `HELIX`, and then `Égypt`
- **THEN** `HELIX` renders, because none of its glyphs inks above cap height or below the baseline
- **AND** `Égypt` fails with reason `text_does_not_fit`, and fails the same way under
  `overflow: ellipsis`, because its one line is the only run there is, and it carries an accent and a
  descender the box cannot hold, and shortening never trims characters to shed them

### Requirement: The size vocabulary is a number, `content`, or `fill`

A `size` component SHALL be a number, a parameter reference `"{param}"` resolving to one, `content`,
or `fill`. The two axes are independent: `size: [content, fill]` hugs horizontally and stretches
vertically.

`auto` SHALL NOT be a valid `size` component. A template using it SHALL fail validation with a
message naming both replacements, and SHALL be quarantined per
`openspec/changes/archive/2026-08-21-issue-181-duplicate-id-not-fatal` rather than making the server
fail to start. The value carried no single meaning to preserve: it meant "fill the frame remainder"
for `text` and `container` on a fixed-width format, "shrink to content" for `text` on a dynamic-width
`single`, "fill the frame remainder" for `container` on that same format, and "exactly `max_w`" for
`qr` and `image`. Rejecting it is the migration mechanism, because a template silently re-read under
any one of those meanings would relayout with no error anywhere.

Migration therefore requires choosing the intended source for each item and axis; it is not a token
rename. In particular, replacing an old `qr` or `image` pairing of `auto` with `max_w` by `content`
does not preserve the old exactly-`max_w` extent, because `content` now starts from the natural size
and merely treats `max_w` as a cap.

#### Scenario: A template using `auto` is refused and quarantined

- **WHEN** a template declares `size: [auto, 16.1]` on any item, on any format
- **THEN** it fails validation, the message names `content` and `fill`, and it is quarantined
- **AND** the server starts and every other template still loads

#### Scenario: The two axes are independent

- **WHEN** an item declares `size: [content, fill]`
- **THEN** its width is its intrinsic width and its height is the frame's remaining height

#### Scenario: A container with no extent fills its parent

- **WHEN** a `container` declares neither `size` nor `to`
- **THEN** it resolves exactly as `size: [fill, fill]` does

### Requirement: Load-time validation and render-time resolution are one algorithm

Size resolution SHALL have exactly one implementation. Load-time validation SHALL run it, rather than
a second copy of the same rules, against a frame built from the template alone: parameter defaults
instantiated per the frozen `docs/SPEC.md` §3.1 rule "At load time, parameter defaults are
instantiated to validate default geometry bounds", which this requirement does **not** supersede, and
`format.width.max` on the horizontal axis of a dynamic-width `single`.

A geometry parameter reference is permitted without an explicit `default`, so instantiation SHALL
retain the existing fallback chain unchanged: the declared `default` when present and parsing as a
number, otherwise the parameter's `min`, otherwise `0`.

A refusal at load SHALL therefore depend only on the template's structure and its declared parameter
defaults, never on the data of any request. It is not a claim that no request could render the
template.

Load-time validation SHALL NOT measure text, encode a QR, or decode an image, and SHALL NOT run a
container's arrangement. At that stage a content source SHALL be taken to yield its available extent.
That is a true upper bound on every claim, because a content or frame extent is clamped by the
available extent, so **no single item's own extent** accepted at load can overflow its frame at render
for want of a measurement.

The guarantee is exactly that wide, and this requirement says so rather than leaving the wider reading
available. It covers one item against its own frame and cannot cover an **accumulation** of siblings,
because load has nothing measured to accumulate: inside a flow container every content-source child
stands in at the whole padded inner extent, so their sum says nothing about the room they will take.
Packed children can therefore accumulate past the padded inner box at render, and the first child the
arrangement positions past that box fails the ordinary bounds check with `UnsupportedLayoutItem` and
`item_out_of_frame`, which is the refusal an author-placed item out of its frame already gets. No
reason is added for it, and load refuses nothing on its account. Load instead checks each packed child
against the padded inner box as if it were the only child, which is a true necessary condition and
catches an oversized authored extent where it is written.

Structural validation SHALL traverse every branch, active or not: a written zero, an impossible
padding, a malformed placement, a `qr` asking for a content or frame extent without `module_size`, or
a shrinking `to` on an unresolved axis is refused wherever it is written, including behind a gate no
default parameter satisfies. Only intrinsic evaluation and frame requirements are skipped for an
inactive branch, and an inactive item's value is not resolved.

This requirement supersedes the frozen `docs/SPEC.md` §7 note "Sizing/bounds logic is intentionally
duplicated between validation (compile time) and rendering (request time); the two must stay in
sync."

#### Scenario: An invalid inactive branch is still refused at load

- **WHEN** a template declares an item behind `when: { debug: true }` whose `size` is `[0, 10]`, and
  `debug` declares `default: false`
- **THEN** the template fails validation and is quarantined

#### Scenario: An inactive item imposes no requirement

- **WHEN** an item's `when` gate does not match the resolved parameters
- **THEN** it imposes no frame requirement on any ancestor and is never asked for an intrinsic size

#### Scenario: An inactive branch's data is still lazy

- **WHEN** a template declares an otherwise valid `text` behind an inactive `when` gate whose value
  references a data field no request supplies
- **THEN** the template loads and renders without `MissingField`

#### Scenario: A geometry parameter with no default falls back to its minimum

- **WHEN** a template declares `size: ["{box_w}", 10]` and `box_w` declares `min: 12` and no `default`
- **THEN** load-time validation resolves that axis as 12
- **AND** a parameter declaring neither resolves as 0, which the written-zero rule then judges

#### Scenario: An intrinsic size is never consulted at load

- **WHEN** a template declares a `content`-width `text` whose placeholder content would overflow
- **THEN** it loads, because no text is measured at load, and whether it overflows is per request

#### Scenario: An accumulation is a render failure, not a load refusal

- **WHEN** a flow container with an authored inner width of 20 and `gap: 2` holds two `content`-width
  text children whose values are supplied per request
- **THEN** the template loads, because at load each child stands in at the whole 20-wide inner box and
  no arrangement is run
- **AND** a request whose values measure 5 and 6 renders both on one line
- **AND** a request whose values measure 14 and 6 fails at render with `UnsupportedLayoutItem` and
  `details.reason` of `item_out_of_frame`, because the second child is positioned at 16 and its far
  edge is 22

#### Scenario: A data-dependent zero is not a load-time refusal

- **WHEN** a template's box collapses to zero width only for requests supplying an empty value
- **THEN** it loads, and renders an empty box for those requests and a normal box for others

### Requirement: The size-resolution reason set

`details.reason` SHALL carry these slugs for size resolution, and `docs/SPEC.md` §10.1's table SHALL
be superseded for them:

| Code | Reason | When |
| --- | --- | --- |
| `UnsupportedLayoutItem` | `size_invalid` | At render: a parameter-resolved extent is not greater than 0, or a `size` extent resolves negative. A **written** non-positive extent is a load-time refusal instead, surfacing as `TemplateInvalid` / `template_validation_failed`. |
| `UnsupportedLayoutItem` | `max_size_invalid` | A `max_w` or `max_h` is not greater than 0. |
| `UnsupportedLayoutItem` | `edge_rect_inverted` | **At render**, a `to` resolves below or to the left of its `at`. This takes priority over `size_invalid` for every `to` spelling, including a stretching `to` that is valid against `width.max` and inverts against a smaller resolved frame. A `to` that already inverts against the load-time frame is refused at load instead, surfacing as `TemplateInvalid` / `template_validation_failed` like every other load-time refusal. |
| `UnsupportedLayoutItem` | `intrinsic_size_undefined` | An intrinsic size was demanded at render but could not be produced because the content declares no extent on that axis or its image-dimension metadata cannot be parsed. |
| `UnsupportedLayoutItem` | `text_does_not_fit` | A `text` does not fit its box: under `overflow: fail` as soon as it overflows, under `overflow: ellipsis` when it still overflows at the shortest form it can be reduced to. |

These slugs SHALL be withdrawn, and SHALL NOT be raised by any code path:

| Reason | Why it can no longer occur |
| --- | --- |
| `size_auto_without_max` | An extent's source supplies it; none needs a `max_*` to resolve. |
| `size_auto_no_room` | A clamped zero renders empty and a negative extent is `size_invalid`. |
| `container_padding_no_room` | A zero inner box renders empty; a constant one is refused at load. |
| `auto_length_cursor_mismatch` | Sizes are exchanged per node, so there is no cursor to desynchronize. |

Adding `intrinsic_size_undefined` and `text_does_not_fit` and withdrawing four slugs is a change to
the reason set that `docs/SPEC.md` §10.1 makes part of the contract, and is recorded as a decision
against ADR-0052.

The `message` beside a size-resolution reason SHALL locate the offending item as an index path
through the layout tree (`layout[0].items[2]`). That is **prose**, on the same footing as the
coordinates `item_out_of_frame` already carries: `details.reason` stays the only machine-readable
discriminator, and this requirement does not extend the `details` schema.

The test reconciling `docs/SPEC.md` §10.1's table with the `Reason` enum SHALL remain deliberately
asymmetric. Its undocumented-addition half SHALL accept reason additions found in canonical
`openspec/specs/**/spec.md` and in active change deltas, because additions must pass the required
pre-archive gates. Its phantom half SHALL apply explicit withdrawals only after this requirement is
synced into canonical `openspec/specs/**/spec.md`; an active delta SHALL NOT suppress a §10.1-table
phantom. Archived changes SHALL count for neither half. Before archive, the two additions are
therefore documented by this active delta, while the four withdrawn slugs remain an expected
registry-test failure. Archive sync removes that failure without a code edit. Once canonical, a
withdrawn slug fails the test if it is reintroduced, just as an undocumented addition does.

#### Scenario: A withdrawn slug is unreachable

- **WHEN** the reason set is enumerated
- **THEN** none of `size_auto_without_max`, `size_auto_no_room`, `container_padding_no_room` or
  `auto_length_cursor_mismatch` is present
- **AND** the registry test fails if any of them is reintroduced without a spec change

#### Scenario: An error locates its item

- **WHEN** a render fails with `text_does_not_fit` for an item nested two containers deep
- **THEN** the message names its index path through the layout tree

### Requirement: Vertical fitting reserves the ink each alignment can expose

A text line's metric box runs cap height to baseline, so accented capitals ink above it and descenders ink below it, and every layout item is drawn into a clipped box of its resolved size. What a block needs beyond its metric box is the ink its own lines carry past its outer edges, and the renderer SHALL measure that rather than assume it. For a block of `n` lines at a candidate font size `s`, drawn with the font instance the renderer will render them with, the renderer SHALL define:

- `pitch(s)` = `line_spacing × s`, where `line_spacing` is the item's authored value or 1.2 when absent (`text-line-spacing`)
- `leading(s)` = `pitch(s) − cap_height(s)`, the paragraph leading the renderer emits for the item; between lines only
- `metric_block(n, s)` = `cap_height(s) + (n − 1) × pitch(s)`, leading between lines only
- `baseline_i(s)` = `cap_height(s) + (i − 1) × pitch(s)`, the depth of line `i`'s baseline below the block's metric top, for `i` from 1 to `n`
- `rise_i` and `fall_i`: how far line `i`'s highest ink lies above its baseline and its lowest ink below it. They SHALL be taken from the outlines of the glyphs line `i` is drawn with, meaning the line as the renderer shapes it: divided, as the renderer divides it, into segments of one writing direction and one script, where a character of no specific script (a combining mark, a digit, punctuation, a space) joins the segment around it, and each segment shaped on its own, at that font instance and size, with every glyph at its shaped position and vertical offsets included. A line that draws no ink, because it is blank or holds only spaces, contributes to neither.
- `a` = outer ink above = `max(0, maxᵢ(rise_i − baseline_i(s)))`: how far the block's highest ink rises above its metric top
- `d` = outer ink below = `max(0, maxᵢ(baseline_i(s) + fall_i) − metric_block(n, s))`: how far its lowest ink falls below its last baseline
- `reserve(vertical)` = `a + d` for `top` and `bottom`, and `2 × max(a, d)` for `center`

The font instance is the one the item renders with: the item's weight on the `wght` axis and `s` on the `opsz` axis, re-read at every candidate size, because the outlines move with both axes. The reservation SHALL come from that instance's outlines and the lines being judged, and from nothing else: not from a constant, a tolerance margin, a font-wide band or a sample of rendered pixels.

Only **outer** ink is reserved. Ink between the block's metric top and its last baseline is inside the box the fit already charges, whichever line draws it, so a descender on a first line or an accent on a last line reserves nothing while the pitch keeps it inside. The maxima run over every line, not only the first and the last, so a pitch small enough to push an interior line's ink past the block's edge is still measured.

A one-line block is one cap-height box whatever the pitch, so `metric_block(1, s)` is pitch-independent and the `text-line-spacing` single-line no-op holds at the fitting level with no special case.

A block of `n` lines at size `s` SHALL be treated as fitting a box of height `H` when `metric_block(n, s) + reserve(vertical) ≤ H`, both terms computed on those `n` lines at size `s`. Every judgement of height SHALL use this one comparison, each on the lines it is deciding about:

- **size selection** judges the lines broken at each candidate size;
- **the `ellipsis` line budget** keeps the largest `k`, from `n` down to 1, whose emitted form fits, where the emitted form of `k` lines is the first `k` lines with every over-wide line shortened in place and, when `k < n`, the marker appended to the `k`-th. The reservation is computed on that emitted form, the marker's glyphs included, because dropping or shortening a line changes the ink the block carries. Fitting is not monotonic in `k`, so the item SHALL be refused only when no `k` fits; there is no separate one-line check that could refuse it first. The previous closed-form budget `max(1, floor((H − reserve × s + leading(s)) / pitch(s)))` and its one-line floor are retired: the reserve is no longer known until the surviving lines are;
- **the `overflow: fail` verdict** judges the lines broken at the chosen size;
- **the intrinsic height** is `metric_block(n, s) + reserve(vertical)` on the lines finally emitted.

The fit comparison carries the renderer's existing tolerance of 0.01 pt, and since every judgement above is that comparison, each carries it. Every containment guarantee in this requirement is therefore bounded by that tolerance: ink may sit up to 0.01 pt outside the box. At 180 dpi that is one fortieth of a pixel, enough to change an antialiased edge pixel's coverage and not enough to cut a stroke.

**Containment** SHALL be judged against an unclipped reference and never against a second clipped render, because a clipped render in a taller box clips an under-reserved accent exactly as the exact box does. An item's ink is contained when the same label, rendered with nothing changed but that item's clip removed and with the item's box placed clear of every label edge so no ink it draws can leave the page, puts no ink in any raster row lying wholly outside the item's box. The rows each box edge falls in are the item's own rows, and are where the tolerance above lands. Removing only the clip keeps the text at the same raster phase as the clipped render, so the reference shows exactly the ink the clip would have cut.

`center` SHALL reserve twice the larger outer ink rather than the sum, because the block is centred on its metric box: the slack `(H − metric_block) / 2` left on each side must absorb the outer ink on that side alone. A block whose outer ink is on one side only, such as `É` alone, therefore reserves twice that ink when centred and once when `top`-aligned.

The reservation SHALL be applied identically whether the size was chosen from a `font_size` range or written as a fixed number. A fixed size cannot shrink, so on a fixed-size item the reservation is visible in the line count and in the verdict of the item's `overflow` policy, never in the size.

Because the reservation follows the ink, two values of the same width and line count can fit at different sizes, keep different numbers of lines, and resolve different `content` heights. That is the intended outcome, and it replaces the rule this requirement previously stated, that per-string glyph bounds SHALL NOT decide a size or a placement. What stays independent of the glyphs is where a `center`-aligned baseline sits, which the *Vertical alignment places a fixed metric box, inset only by the ink the block carries past its aligned edge* requirement owns, together with every other placement rule. That requirement reads the same `a` and `d` from the same emitted lines as the fit.

The reservation is part of the item's **intrinsic height** for every alignment, because that height is the room the block needs and clipped ink is not room it has. An item asking for a `content` height therefore resolves a box of `metric_block(n, s) + reserve(vertical)`, and its content sits within that box as its alignment says. An item whose height is authored, or comes from `fill` or `to`, is unaffected: its box is decided without reference to the intrinsic.

The guarantee covers every glyph the rendering font supplies, wherever it inks, including those that rise above the font's ascender or fall below its descender. A character the rendering font does not map is outside it, as the text-layout requirement states: Typst draws that character from a fallback font the measurement does not read, and it is #254's scope.

This requirement supersedes, in the frozen `docs/SPEC.md` §3.1, the "**`top` and `bottom` inset the block so its ink stays inside the slot**" bullet, the "**Two limits worth knowing**" bullet, and the definition of the `overflow` term in the wrapped line-count formula `floor((H − overflow + leading) / (cap_height + leading))`. The paragraph they sit under, "**What `alignment.vertical` aligns.**", and its lowercase bullet are superseded by the alignment requirement named above, and the blank-edge bullet by the text-layout requirement. The Typst-default leading (`0.65em`) an earlier revision of this requirement inherited is retired: `leading(s)` above is derived from the authored pitch, and the pitch itself comes from the `text-line-spacing` capability, which owns its default.

#### Scenario: Aligned edges are unchanged

- **WHEN** a `top`-aligned `text` with a fixed `font_size` of 20 and `overflow: fail` renders `HELIX` in a box exactly `cap_height(20 pt)` tall
- **THEN** it renders, because its reservation is zero
- **AND** before this requirement the same template was refused with `text_does_not_fit`, having reserved 0.4824em it did not use
- **AND** this scenario keeps its name from the superseded version, which held `top` and `bottom` to the font's bands; they now reserve the value's own ink, so the aligned edges do change

#### Scenario: Auto-shrink sees the emitted ink

- **WHEN** a `top`-aligned `text` with a `font_size` range renders `HELIX` and then `HÉLIX`, in a box wide enough for either at the range's maximum and too short for either there
- **THEN** `HÉLIX` settles at a smaller size than `HELIX`, because the two values have the same advances and only `HÉLIX` carries ink above cap height
- **AND** `HÉLIX`'s ink, its accent included, is contained

#### Scenario: A centred block auto-shrunk into a tight box keeps its descenders

- **WHEN** a 24 mm tape template whose `center`-aligned, `wrap: true` `text` item is 120 mm wide and fills the full 18.1 mm printable height, with `font_size: { min: 10, max: 32 }`, renders a value carrying a descender that breaks to two lines, such as "Kitchen Utensils and a much longer second line here"
- **THEN** the chosen size is the largest 0.5 pt step at which `metric_block(2, s) + 2 × max(a, d)` fits 51.31 pt, with `a` and `d` measured on the two lines broken at that step
- **AND** the block's ink is contained, the descender of `g` included, where the same template and value before #245 cut that descender mid-stroke on the box's final raster row

#### Scenario: A centred multiline block's line budget counts the reserve

- **WHEN** a `center`-aligned `wrap: false` `text` with a fixed `font_size` and `overflow: ellipsis` sits in a box exactly `metric_block(3, s)` tall and wide enough for each line with the marker, and renders `HELIX\nHELIX\nHELIX`, and then `HELIX\nHELIX\nHELgX`
- **THEN** the first value keeps all three lines and carries no marker, because its reservation is zero
- **AND** the second keeps two lines, the second ending in `...`, because its three-line form must reserve twice the depth of `g` and its two-line emitted form, marker included, fits
- **AND** with `overflow: fail` the first renders and the second fails with reason `text_does_not_fit`
- **AND** in each render that succeeds the kept block's ink is contained

#### Scenario: Ink between the lines reserves nothing

- **WHEN** a `top`-aligned two-line `text` with `size: [content, content]` at the default pitch is laid out at size `s` with `Hg\nÉH`, and then with `ÉH\nHg`
- **THEN** `Hg\nÉH` resolves a box exactly `metric_block(2, s)` tall
- **AND** `ÉH\nHg` resolves a box `metric_block(2, s) + a + d` tall, `a` being `É`'s accent height above cap height and `d` the depth of `g`

#### Scenario: A centred item asking for a content height grows by the reservation

- **WHEN** a `text` with `size: [content, content]` is laid out at size `s` with `HELIX`, and then with `Égypt`, first `top`-aligned and then `center`-aligned
- **THEN** `HELIX` resolves a box exactly `cap_height(s)` tall under either alignment
- **AND** `top`-aligned `Égypt` resolves `cap_height(s) + a + d`, its ink reaching the box's top and bottom edges and contained
- **AND** `center`-aligned `Égypt` resolves `cap_height(s) + 2 × max(a, d)` with its metric block centred, so the gaps between its ink and the box's edges are `max(a, d) − a` above and `max(a, d) − d` below
- **AND** this scenario keeps its name from the superseded version, where the growth was the font's bands whatever the value

#### Scenario: An asymmetric font reserves twice its larger overflow

- **WHEN** a `text` with `size: [content, content]` is laid out at size `s` with `É`, which inks above cap height and not below the baseline
- **THEN** `center`-aligned it resolves `cap_height(s) + 2a`, and `top`-aligned `cap_height(s) + a`
- **AND** this scenario keeps its name from the superseded version, where the asymmetry had to come from a font supplied through `LABELER_FONTS_DIR`; it now comes from the value, so the bundled font shows it

#### Scenario: The reservation is read from the instance rendered

- **WHEN** a `top`-aligned `text` with `size: [content, content]` and a fixed `font_size` renders `É` at `font_weight: 400`, and then at `font_weight: 700`
- **THEN** each resolved height is `cap_height(s)` plus the accent's height above cap height in the outlines of that weight's instance at that size
- **AND** the two heights differ, as the two instances' accents do

#### Scenario: A glyph outside the declared band still clips

- **WHEN** a `top`-aligned `text` with a fixed `font_size` renders `Ǻ`, whose ink rises above Inter's typographic ascender, in a box exactly as tall as its intrinsic height
- **THEN** the render succeeds and the glyph's ink is contained
- **AND** before this requirement the reservation stopped at the ascender and the top of the glyph was clipped
- **AND** this scenario keeps its name from the superseded version, under which such a glyph could clip; a glyph the font supplies no longer does, and the one remaining clip is the unmapped character below

#### Scenario: A value of ten thousand lines is shortened like a value of three

- **WHEN** a `top`-aligned `wrap: false` `text` with a fixed `font_size`, `overflow: ellipsis` and a box holding two lines at that size renders a value of 10,000 short lines
- **THEN** it keeps the first two lines, the second ending in `...`, and the render completes
- **AND** no line is measured for ink more than twice in reaching that answer, and a line whose baseline would lie more than 0.01 pt below the box's bottom edge is not measured for ink at all, since no run containing it can fit within the fit comparison's tolerance, because every line's hard break is laid out and the budget's work must grow with the line count rather than with its square

#### Scenario: The metric cutoff honours the fit tolerance

- **WHEN** a `top`-aligned `text` with a fixed `font_size` of 20 and `overflow: fail` renders `HELIX`, whose metric block is 14.55078125 pt and whose reservation is zero, in a box 14.54578125 pt tall, and then in a box 14.53578125 pt tall
- **THEN** it renders in the first box, 0.005 pt short of its metric block and within the 0.01 pt tolerance, and fails with reason `text_does_not_fit` in the second, 0.015 pt short
- **AND** the same two boxes give the same two outcomes under `overflow: ellipsis`, so the line budget's cutoff before measuring ink accepts and refuses exactly where the fit comparison does

#### Scenario: A mark after a change of script is measured in its own segment

- **WHEN** a `top`-aligned `text` with a fixed `font_size` of 20 renders `αH\u0301` (a Greek alpha, then `H` carrying a combining acute) in a box exactly as tall as its intrinsic height
- **THEN** it is inset at the top by exactly what `H\u0301` alone is inset by, because the renderer shapes the Latin segment `H\u0301` on its own and places the acute over the `H`
- **AND** its ink is contained
- **AND** a measurement shaping the line as a single segment places the acute 393 font units lower in the bundled font, under-reserving by 3.84 pt at this size, which is the clip this scenario exists to refuse

#### Scenario: A character the font does not map is outside the guarantee

- **WHEN** a `text` renders a value containing a character the rendering font has no glyph for
- **THEN** the item is fitted and rendered, and is not refused on that character's account
- **AND** its reservation at that position is the one the rendering font's own missing-glyph outline calls for, whatever the fallback glyph Typst draws there inks, which is #254's scope

#### Scenario: A centred item with headroom is unaffected

- **WHEN** a `center`-aligned text item rendering a single line, a value with no line breaks that fits its box width at `font_size.max`, sits in a box whose height is authored, `fill` or `to`, and that box is tall enough at `font_size.max` for both the measured reservation and the font-band reservation before this requirement: `metric_block(1, max) + 2 × max(a, d) ≤ H` and `metric_block(1, max) + 2 × max(u, d_font) × max ≤ H`, with `u` and `d_font` the font's ascent and descent overflows that revision reserved
- **THEN** the size the fitter chooses is decided by width alone, exactly as before, and the item resolves the same box, size and lines it did before
- **AND** the rendered output is byte-identical to what the same template and data produced before this requirement, because a centred block's placement never read the reservation

#### Scenario: A height-bound centred item picks a larger size

- **WHEN** a `center`-aligned `text` item with ample width and `font_size: { min: 10, max: 20 }` renders `HELIX` in a box 16 pt tall
- **THEN** it renders at 20 pt, because its metric block of 14.55 pt fits with nothing reserved
- **AND** before this requirement the same template and value rendered at 13.0 pt, the largest step at which `1.2099 × s ≤ 16`, so its output changes, and the unchanged-output scenario above does not cover it

#### Scenario: The default pitch tightens existing multi-line items

- **WHEN** a two-line `wrap: true` item declaring no `line_spacing` renders with the bundled font at its fitted size `s`
- **THEN** its metric block is `cap_height(s) + 1.2 × s`, not the `2 × cap_height(s) + 0.65 × s` an earlier revision reserved

### Requirement: A dynamic width's resolved bounds are ordered

On a dynamic-width `single` (`format.width: { min, max }`), each bound is a literal or a `"{param}"`
reference, and both are resolved per render before the label is measured. The resolved `min` SHALL be
less than or equal to the resolved `max`. A render whose resolved `min` is strictly greater than its
resolved `max` SHALL be refused with `400 InvalidRequest` and `details.reason` of
`width_bounds_inverted`, and the message SHALL name both resolved values with the template `unit` and,
for each bound that is a parameter reference, the parameter it resolved from. The render SHALL NOT
clamp the content requirement into the inverted pair, SHALL NOT swap the bounds, SHALL NOT proceed at
either bound alone, and SHALL NOT panic. Equal bounds are ordered: a render whose `min` and `max`
resolve to the same value renders at that width.

**The order of refusals is observable.** Each resolved bound is first judged as a dimension, exactly as
it is today: a bound that is not a finite number greater than zero, or that exceeds the
`max_label_dimension_mm` setting, is refused with `422 UnsupportedLayoutItem` and
`dimension_exceeds_limit`, and the pair is never compared. Only two bounds that are each a legal
dimension reach the ordering check, so `width_bounds_inverted` means exactly that: two usable
widths, in the wrong order. A `max` of `0` or `-5` against a `min` of `10` is therefore
`dimension_exceeds_limit`, not `width_bounds_inverted`.

The ordering check runs **before the items are measured**, so it precedes every refusal measurement
can raise. A request whose bounds are inverted and whose items would also fail measurement, such as an
authored item extent that resolves to zero, is refused with `400 width_bounds_inverted`, and the item
failure is not reached. The same request with ordered bounds is refused by measurement as it is today,
`422 UnsupportedLayoutItem` with `size_invalid` for that example. **This changes the error contract
for one class of request:** today measurement runs first and such a request answers the item's `422`,
never reaching the clamp; after this change it answers `400 width_bounds_inverted`. The label's own
bounds are judged before anything inside them, because an item cannot be measured against a frame
that does not exist.

**The refusal reads values, not provenance.** The check applies to the values resolved **for that
render**: a supplied value for a referenced bound is used, and a parameter's declared `default` is
used only when the request omits it. A bound that resolved from a `default`, or that is a literal in
the template, meets the check exactly as a caller-supplied value does, and is reported the same way.
`param-resolution` still decides a default that cannot be resolved at all (`422 TemplateInvalid`,
`param_default_unresolvable`) before any bound exists to compare. Two consequences follow. A template
whose two bounds are literals in the wrong order is refused with `400 width_bounds_inverted` on every
render, because nothing a request carries can change either value. A template whose declared default
puts a referenced bound out of order is refused on exactly the renders that leave that parameter to
its default, and a render supplying a value that orders the pair succeeds. Whether a template of
either kind ought to be refused when it is loaded is a separate question this requirement does not
answer; what it guarantees is that no render reaches the clamp with the pair inverted.

**Every path that renders a label is covered.** The single-label render, the thumbnail, and each
label of a batch, print or CSV import request resolve the bounds through the one pre-pass, and the
refusal above applies on each. On the batch path a refused label is a failing label under
`batch-validation`: the request SHALL return `422` with `error.code` `BatchInvalid` and one entry in
`error.details.failures` at that label's `index` carrying `code` `InvalidRequest` and `reason`
`width_bounds_inverted`, and SHALL produce nothing.

This requirement supersedes the frozen `docs/SPEC.md` §3.1 auto-length paragraph so far as it
concerns the bounds the label width is clamped into (the words "clamping to `[min, max]`"). It leaves
the rest of that paragraph, the §3.1 load-time sentence about instantiated defaults, and every other
§3.1 rule authoritative, and it does not alter which width an item contributes, which "An item
requires of its frame the smallest extent that contains it" already states.

#### Scenario: A supplied `max` below `min` is refused

- **WHEN** a `single` template declares `width: { min: 10.0, max: "{max_width}" }` on a `length`
  parameter `max_width` with `default: 120.0`, and `POST /api/render/label?format=png` is sent with
  `data.max_width` of `5`
- **THEN** the response is `400` with `error.code` `InvalidRequest`, `error.details.reason`
  `width_bounds_inverted`, and a message naming `max_width`, `5`, and `10`

#### Scenario: A supplied `min` above a literal `max` is refused, naming the parameter that moved

- **WHEN** a `single` template declares `width: { min: "{min_width}", max: 60.0 }` and a render
  supplies `min_width` of `80`
- **THEN** the response is `400` with `error.details.reason` `width_bounds_inverted` and a message
  naming `min_width`, `80`, and `60`

#### Scenario: Both bounds are references and both are named

- **WHEN** a `single` template declares `width: { min: "{lo}", max: "{hi}" }` and a render supplies
  `lo` of `30` and `hi` of `20`
- **THEN** the response is `400` with `error.details.reason` `width_bounds_inverted` and a message
  naming `lo`, `hi`, `30`, and `20`

#### Scenario: Equal bounds render

- **WHEN** a `single` template declares `width: { min: 10.0, max: "{max_width}" }` and a render
  supplies `max_width` of `10`
- **THEN** the render succeeds and the label is `10` units wide

#### Scenario: A non-positive `max` is a dimension failure, not an ordering failure

- **WHEN** a `single` template declares `width: { min: 10.0, max: "{max_width}" }` and a render
  supplies `max_width` of `0`
- **THEN** the response is `422` with `error.code` `UnsupportedLayoutItem` and `error.details.reason`
  `dimension_exceeds_limit`, and the ordering check is not reached

#### Scenario: An inverted default is refused on the render that resolves it

- **WHEN** a `single` template declares `width: { min: 10.0, max: "{max_width}" }` on a parameter
  `max_width` with `default: 5.0`, and a render omits `max_width`
- **THEN** the response is `400` with `error.details.reason` `width_bounds_inverted`, naming
  `max_width`, `5`, and `10`

#### Scenario: A supplied value overrides an inverted default

- **WHEN** a `single` template declares `width: { min: 10.0, max: "{max_width}" }` on a parameter
  `max_width` with `default: 5.0` and otherwise valid geometry, and a render supplies `max_width` of
  `20`
- **THEN** the render succeeds

#### Scenario: Inverted bounds are refused before an item is measured

- **WHEN** a `single` template declares `width: { min: 10.0, max: "{max_width}" }` and a `text` item
  whose authored width is `"{w}"` on a parameter `w` with a positive default, and a render supplies
  `max_width` of `5` and `w` of `0`
- **THEN** the response is `400` with `error.details.reason` `width_bounds_inverted`, and the item's
  zero width is not reported

#### Scenario: With ordered bounds the item's own refusal stands

- **WHEN** the template of the previous scenario is rendered with `max_width` of `40` and `w` of `0`
- **THEN** the response is `422` with `error.code` `UnsupportedLayoutItem` and `error.details.reason`
  `size_invalid`, as it is today

#### Scenario: The worker does not panic and the service keeps serving

- **WHEN** the request of the first scenario is sent
- **THEN** the response is the `400` above and not the `500 Internal` a covered panic answers with,
  and a following `GET /api/health` on the same service answers `200`

#### Scenario: A batch carrying an inverted label is atomic

- **WHEN** `POST /api/batch` carries two labels of the template in the first scenario, the first
  supplying `max_width` of `40` and the second supplying `max_width` of `5`
- **THEN** the response is `422` with `error.code` `BatchInvalid`, `error.details.failures` holds
  exactly one entry, at `index` `1`, with `code` `InvalidRequest` and `reason`
  `width_bounds_inverted`, and no PDF or ZIP is produced

### Requirement: Vertical alignment places a fixed metric box, inset only by the ink the block carries past its aligned edge

`alignment.vertical` SHALL position a text block's **metric box** inside the item's resolved box. The metric box runs from the first line's cap-height top to the last line's baseline, with baselines `pitch(s)` apart, exactly as the *Vertical fitting reserves the ink each alignment can expose* requirement defines `metric_block(n, s)`. A blank line has a line box like any other, so a blank first line's cap-height top is still the block's metric top. The box is Typst's default line box, cap height to baseline: the one CSS `text-box-trim` and Figma's vertical trim use, and the convention Pango, Pillow, TeX (`\strut`) and Flutter (`StrutStyle`) follow.

With `a` and `d` the emitted block's outer ink above and below, as the fitting requirement defines them:

- **`center`** SHALL centre the metric box in the item's box and inset nothing. A centred baseline is therefore a function of the box, the font size, the pitch and the line count, and never of which glyphs the lines carry: two values rendered at the same size and line count in the same box share every baseline.
- **`top`** SHALL place the metric top `a` below the box's top edge. A block with no ink above its metric top sits flush, its metric top on the box's top edge.
- **`bottom`** SHALL place the last baseline `d` above the box's bottom edge. A block with no ink below its last baseline sits flush, its last baseline on the box's bottom edge.

The inset SHALL be computed from the same emitted lines, at the same size and on the same font instance, as that item's fit and intrinsic height, so the block placed is the block the fit judged and no placement insets by more than the fit reserved. The edge a block is not aligned to needs no inset of its own: the fit reserved `a + d` for `top` and `bottom`, so the ink at the far edge lands inside the box as well.

The inset SHALL never be negative. A block whose ink falls short of its aligned metric edge is not moved toward that edge: `ace`, whose ink tops out near x-height, sits top-aligned exactly where `HELIX` does, its ink below the top edge by the gap between cap height and the top of that ink. What is aligned is the metric box, not the ink box. Centring each string's own ink box, which #127 shipped and #133 reverted, SHALL NOT be restored at any alignment.

What this costs is stated rather than hidden. A `top`- or `bottom`-aligned baseline moves between two values when one of them inks past the aligned metric edge, and by exactly that ink: `HELIX` and `Émile` top-aligned at one size differ by the height of `É`'s accent above cap height. In the bundled Inter that covers accented capitals, the dots of `i` and `j`, and the overshoot of round capitals and figures such as `O` and `0`, which rise about 0.01em above cap height. A value whose ink stays inside the metric box at the aligned edge never moves.

This requirement supersedes, in the frozen `docs/SPEC.md` §3.1, the paragraph "**What `alignment.vertical` aligns.**" in full and the "**Lowercase-only text sits lower in its slot than all-caps.**" bullet in full. The other bullets under that paragraph were already superseded: the inset and limits bullets by the fitting requirement, and the blank-edge bullet by *Text is laid out against the box it will get, and what does not fit is authored*. The "**`alignment.vertical` on auto-length items.**" paragraph, which makes `top` the default, is unaffected and remains authoritative.

#### Scenario: Unaccented capitals sit flush with the top edge

- **WHEN** a `top`-aligned `text` with a fixed `font_size` of 20 renders `HELIX` in a box taller than its cap height
- **THEN** its metric top sits on the box's top edge, with no inset, where before this requirement it was inset by 0.2412em (4.82 pt)
- **AND** in the rendered PNG the item's first inked raster row is within one row of the box's top edge

#### Scenario: An accented capital is inset by its own accent and stays whole

- **WHEN** a `top`-aligned `text` with a fixed `font_size` of 20 renders `Émile` in a box exactly as tall as its intrinsic height
- **THEN** its metric top is inset by the height `É`'s accent rises above cap height in the font instance rendered, and not by 0.2412em
- **AND** its ink, the accent included, is contained, as the *Vertical fitting reserves the ink each alignment can expose* requirement defines containment

#### Scenario: A descender is inset by its own depth and stays whole

- **WHEN** a `bottom`-aligned `text` with a fixed `font_size` of 20 renders `gyp` in a box exactly as tall as its intrinsic height
- **THEN** its baseline is inset by the depth of its lowest descender in the font instance rendered, and not by 0.2412em
- **AND** its ink, the descenders included, is contained, as the *Vertical fitting reserves the ink each alignment can expose* requirement defines containment

#### Scenario: A centred baseline does not follow the glyphs

- **WHEN** one `center`-aligned `text` with a fixed `font_size` renders `HELIX`, `Émile`, `testj` and `gyp` in turn, in a box tall enough for each at that size
- **THEN** no inset is emitted for any of them, and all four renders put the baseline on the same raster row
- **AND** their ink sits at different distances from the box's top and bottom edges, because the metric box is centred and not the ink box

#### Scenario: A block is never pulled toward its aligned edge

- **WHEN** a `top`-aligned `text` with a fixed `font_size` renders `ace` and then `HELIX` in the same box
- **THEN** both put the baseline on the same raster row
- **AND** `ace`'s first inked row lies below the box's top edge, by the gap between cap height and the top of its own ink, rather than on it

#### Scenario: Ink between the lines moves nothing

- **WHEN** a `top`-aligned two-line `text` at the default pitch renders `Hg\nÉH`, and then `ÉH\nHg`
- **THEN** `Hg\nÉH` is emitted with no inset, because its accent lies below the block's metric top and its descender above the block's last baseline
- **AND** `ÉH\nHg` is inset at the top by `É`'s accent height
- **AND** rendered `bottom`-aligned, `ÉH\nHg` is inset at the bottom by the depth of `g` and `Hg\nÉH` is not inset
