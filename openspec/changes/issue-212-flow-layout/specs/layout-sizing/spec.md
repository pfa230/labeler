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
every respect once permitted, and the condition on it is one of the two places the resolved-axis state
is consulted; the other is `flow-layout`'s `wrap`, which chooses lines against a container's own main
extent and so requires that axis to be resolved.

The **available extent** on an axis SHALL be `frame extent − resolve(at) − inset`, where
`resolve(at)` is `at`'s component when non-negative and `frame extent + at` when sign-negative, and
`inset` is the far-edge margin a `to` reserves (`−to`'s component) or zero. For a sign-negative `at`
of inset `a` the frame terms cancel and the available extent is `a − inset`, independent of the
frame: the anchor is the box's low edge, so a right- or top-anchored item has only the space between
its anchor and the far edge, less any margin it reserves there.

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
| `container` | its children aggregated by its **arrangement**, plus padding | 1 |
| `line` | none: two endpoints, no box | — |

A container's arrangement decides how the row above aggregates. An **absolute** container takes the
largest child frame requirement on each axis; a **flow** container takes the assembled extent the
`flow-layout` capability defines. The term is the children either way, and only the combination
differs, so this rule is stated once here and the combination once per arrangement.

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

#### Scenario: A line has no intrinsic size

- **WHEN** any item asks a `line` for an intrinsic size
- **THEN** there is none to give: a line contributes through its endpoints, by the next requirement
- **AND** an implementation modelling measured nodes uniformly must keep "no intrinsic size"
  distinguishable from "an intrinsic size of zero"

#### Scenario: A container's arrangement decides the combination

- **WHEN** two children whose extents are 10 and 6 wide sit in a `content`-sized container
- **THEN** an absolute container at `at: [0, 0]` and `at: [0, 5]` has an intrinsic width of 10
- **AND** a flow container with `flow: { direction: row, gap: 2 }` has an intrinsic width of 18


### Requirement: A frame's axis is resolved unless something inside it decides its size

A frame's axis SHALL be **resolved** when its extent is known before the items inside it are sized.
It SHALL be resolved when the item establishing it has an authored extent on that axis; when the
extent is a frame source under a **sign-negative anchor**, whose available extent is `a − inset` and
therefore frame-independent; and otherwise when the enclosing frame's axis is resolved and the
extent's source is not `content`. A `content` axis is never resolved, because its extent is derived
from the very children being sized.

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

**The two rules that consult it.** The first is `flow-layout`'s `wrap`, which chooses lines against the
container's own main extent and so requires that axis to be resolved; that capability states the rule
and this one supplies the state it reads. The second is here. A `to` with a sign-negative `at` and a non-negative `to` resolves
to `to − at − F`: it grows *narrower* as the frame grows, has no claim that could size a frame, and
inverts once `F` exceeds `to − at`. It SHALL be permitted only on a resolved axis, where the frame is
a constant before the item is sized, and SHALL be refused with `TemplateInvalid` otherwise. Where
permitted it is an authored extent in every respect: its extent is the corner subtraction, `max_*` is
inert on it, and it demands no intrinsic size.

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

#### Scenario: A wrapping flow container reads the same state

- **WHEN** a flow container declares `wrap: true` on an axis this requirement calls unresolved
- **THEN** the template is refused at load by `flow-layout`'s rule, reading this state unchanged


### Requirement: A container establishes a padded frame, and rotation swaps it

A `container`'s children SHALL be sized against its **padded inner box**, its resolved extent less
its padding, clamped at zero on each axis. A zero inner box SHALL render an empty container, whether
or not it has active children.

A container positions its children by its **arrangement**, and the arrangement also decides how their
requirements aggregate into the container's own intrinsic size:

- **absolute**, the arrangement of a container with no `flow` block, and the only one before this
  change: each child is positioned by its own `at`, and the container's intrinsic size on an axis is
  padding plus the **largest** child requirement on that axis.
- **flow**, the arrangement of a container carrying a `flow` block: children are packed in order, and
  the container's intrinsic size is padding plus the **assembled extent** the `flow-layout` capability
  defines.

An arrangement decides **position**, never extent. Every child of either kind is sized by this
capability against the padded inner box, and every other sentence of this requirement applies to a
flow container unchanged, including the zero-inner-box outcome and the rotation swap below.

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
a rotated container SHALL compute its intrinsic size in **author space** by the rule for its
arrangement above, padding plus that arrangement's aggregate on each author axis, and SHALL then swap the
resulting pair to obtain its intrinsic size in its parent's physical frame. It is the completed
author-space aggregate that swaps, not the children's raw intrinsic sizes: author-space offsets,
`line` requirements and author-space padding are inside the aggregate and survive the swap with it.
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
