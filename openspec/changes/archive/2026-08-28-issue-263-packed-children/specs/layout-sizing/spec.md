## MODIFIED Requirements

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
  is author-space and rotates with the design. The physical `frame` outline is not rotated.

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
  `debug` defaults to `false`
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

