## Context

See proposal.md for motivation and specs/layout-sizing/spec.md for the contract.

What the code looks like today, in the terms this design has to change:

- `RenderContext::measure` (`src/render/mod.rs:1010`) walks the layout on dynamic-width singles only,
  appending `MeasuredText` values to a flat `Vec` and returning a content extent. The render walk
  (`render_text_item`, `:1445`) consumes that `Vec` positionally through a `Cell<usize>` cursor. The
  two walks must visit the same items in the same order or the render fails with
  `auto_length_cursor_mismatch`; keeping them aligned costs a nineteen-line comment at `:1035` and a
  dedicated clause for right-anchored containers.
- `resolve_size` / `resolve_size_value` (`:1967`, `:1996`) take an `allow_auto_fill` flag that
  encodes "does this item type have a frame to fall back on", plus blame logic choosing between
  `max_size_invalid` and `size_auto_no_room`.
- `templates.rs` carries its own `resolve_size` (`:1548`), `resolve_size_value` (`:1607`),
  `resolve_to_extent` (`:1534`) and `subtree_uses_auto` (`:1499`): a second implementation of the
  same rules against the `width.max` frame.
- `render_container_item` (`:1743`) has an unrotated path and a rotated path that repeat the same
  placement and framing logic with a swapped canvas.
- `fit_text_auto_length` (`src/render/helpers.rs:736`) is the only content-measuring function and has
  exactly one call site.

Constraints that shape the approach: `raw.rs` / `models.rs` / `convert.rs` move together for any
layout field; `docs/SPEC.md` is frozen; every exposed model is registered in `src/openapi.rs`;
templates that fail validation are quarantined rather than fatal (ADR-0058); and output changes are
only acceptable where the specs say behavior changes.

## Goals / Non-Goals

**Goals:**

- One resolver, called from both `templates.rs` and `render/mod.rs`, so a sizing rule is written once.
- Structural pairing of the two passes, so cursor desynchronization is unrepresentable rather than
  merely tested for.
- Byte-identical Typst source for every template whose behavior the specs do not change, as a
  regression guard on a diff this large.
- One container path, with rotation as a parameter rather than a second branch.

**Non-Goals:**

- Flow / packed layout (#212). This design hands #212 a protocol that already exchanges child sizes;
  packing is one arrangement inside it.
- Auto-height labels. `format.height` stays a fixed `Dimension`, so the page node participates on
  the width axis only.
- Re-wrapping a child to a width chosen after it reported. Wrapping stays authored, so one intrinsic
  number per axis per constraint is sufficient and min-content/max-content is not needed.
- Any change to the alignment metrics of ADR-0045 / 0049 / 0050, the `when` gate, or interpolation.

## Decisions

### 0. Three sources for an extent, which is what removes the exceptions

An earlier draft of these artifacts stated eleven requirements, four of which were the same rule seen
from different angles. The rule is that an extent comes from one of three places, and the source
decides everything else:

| Source | Extent | Checked or clamped | Caps | Needs an intrinsic | Reports upward |
| --- | --- | --- | --- | --- | --- |
| **author** | what was written | checked, refused if it does not fit | inert | no | its extent |
| **content** | the intrinsic size | clamped by cap and available | bind | yes | its extent |
| **frame** | the available extent | clamped by cap and available | bind | yes | `min(intrinsic, cap, available)` |

Four rules the earlier draft stated separately collapse into the first four columns: which extents
`max_*` binds, whether a zero is an error or an empty box, why a right-anchored authored extent needs
a `claim ≤ inset` check, and why a numeric image is never decoded. That last one was the weakest part
of the earlier design, justified there as a laziness optimisation. It is not an optimisation: an
authored extent needs no intrinsic size because the author supplied the number, and that is true of
every item type.

The classification must happen before deciding whether a cap is legal. Today
`PlacementRaw::into_placement` rejects every `max_w`/`max_h` beside `to` (`src/convert.rs:31-37`), so
that conversion guard is deleted and the cap reaches `source_of`: it binds a frame-source `to` and is
inert on an authored one, exactly as the table says.

The single asymmetry left is the last column, where a frame source reports its **content** upward
while taking the available extent downward. That is not an exception; it is the definition of `fill`,
and it is what lets an item that stretches to a label still be the item that sizes it.

The report is bounded by the same cap and available extent as the extent itself, not by the raw
intrinsic. Round 8 caught an earlier draft dropping that bound: a `qr` with `size: [fill, 10]`,
intrinsic 50 and `max_w: 20` would have reported 50 and then occupied 20, sizing its parent to a width
it never uses. Current measurement already binds the budget before contributing
(`src/render/mod.rs:1084-1089`, `:1189-1216`).

The `to` classification collapses the same way. Resolving a corner is `F·s + v` with `s` set for a
sign-negative component, so every `to` extent is `F·(s(to) − s(at)) + (to − at)`: an affine function
of the frame whose slope is `−1`, `0` or `+1`. Slope `0` is an authored extent, slope `+1` is a frame
extent and is exactly what `fill` produces, and slope `−1` is the one case that needs a resolved frame
because it shrinks as the frame grows. Three slopes, not a four-row table of cases.

### 1. A measured tree, not a side list

`measure` returns a tree that mirrors the layout tree, not a flat `Vec`:

```
struct Measured {
    intrinsic: [Option<f32>; 2],  // per AXIS: None = not demanded on this axis
    text:      Option<TextFit>,   // the laid-out lines and chosen size, for every active text
    children:  Vec<Measured>,     // active children only, in layout order
}
```

Three fields, and each earns its place. `intrinsic` is per axis and optional because demand is per
axis: `size: [40, content]` demands the height and not the width, and the spec forbids reading an
image's dimensions for an axis that did not ask. A single `Option<Size>` cannot express that; it
would force a numeric image to be decoded, or to carry a fabricated size that later arithmetic could
not distinguish from a measured one. `line` is the degenerate `[None, None]` rather than a variant.

`text` sits beside `intrinsic` rather than inside it because layout and measurement are demanded
independently: every active text is laid out, since its lines are the rendered output, while its
intrinsic size is recorded only where an axis asked. Conflating the two is the one way an
implementation could satisfy this decision and still skip `overflow` enforcement on a fully authored
box.

Nothing else is stored. A QR's module count and an image's pixel dimensions feed the intrinsic and
are then discarded; the placement walk re-encodes the QR and forwards the image bytes as it does
today, rather than carrying a payload enum whose only real member is the text case. An error's index
path comes from the walk's own position, not from a field on the node.

The placement walk takes `(&LayoutItem, &Measured)` together and recurses into
`(children[i], measured.children[i])`. The pairing is structural, so there is no cursor, no
positional replay, and no way for the two walks to disagree about which item they are on.
`auto_length_cursor_mismatch` is deleted because the state it guarded no longer exists.

`children` holds only `when`-active items, and the placement walk filters by the same predicate, so
the two agree by construction. The alternative, keying a side table by a path vector (`[0, 2, 1]`),
keeps the two walks independent and was rejected: it reintroduces the same class of bug behind a
nicer key, and the tree is what #212 will need anyway.

### 2. The intrinsic pass runs on every format

`content` can appear on a fixed-width `single` or a `sheet`, so the pass cannot be conditional on
`format.width` being dynamic. `LengthMode::{Fixed, Dynamic}` and `AutoLength` are deleted; what was
`Dynamic` becomes "the top-level horizontal constraint is `format.width.max`, and the page width is
the clamped root contribution", computed in `compile_label_doc` rather than carried in the context.

A consequence worth stating rather than discovering: text layout also stops consulting the page
format. Today the engine pre-breaks a text's lines when `font_size` is a range, or when the template
is auto-length and the item's width is frame-dependent (`src/render/mod.rs:1445-1450`); otherwise the
`FontSize::Fixed` arm emits the authored string and lets Typst break and clip it (`:1539-1588`). The
second test is `LengthMode` again, a page-level flag deciding a per-item question.

An earlier draft tried to keep it as a conditional, phrased as "the engine decides lines when a size
depends on them", and the round-3 review showed that still changed a fixed-font `to: [-0.0]` text on
a fixed page. The conditional was the error, not its phrasing: a text's constraint box is known
top-down in every case, including on an auto-length label where the budget is `width.max - at.x`,
which is exactly what the current measure pass uses. So the rule has no condition. Lines are laid out
against the constraint, always, and what happens to what still does not fit is a policy the author
writes (decision 11) rather than a consequence of which branch ran.

Byte-identity for a `font_size` range therefore rests on one thing: the unified helper must produce
what `fit_text_to_box` produces for the same box, including counting blank edge lines while choosing
the size and dropping them only at emission. That is an implementation obligation with a test, not a
design question. Fixed-`font_size` text does change, and the specs say so: the four fixed-font items
in `tests/fixtures/templates/avery5163_asset_tag.yaml` (`:91`, `:98`, `:141`, `:148`) are the only
ones in the repository, and they are this change's visual acceptance.

“Break, then shrink” does not mean preserving the breaks chosen at `font_size.max`. As today,
`largest_fitting_font` tests each 0.5 pt candidate by wrapping at that candidate's glyph advances
(`src/render/helpers.rs:395-407`, `:444-451`); the emitted breaks are those chosen at the selected
size. Keeping one set of breaks while shrinking would not be byte-identical.

### 3. One resolver, and load substitutes availability for an intrinsic

An earlier draft parameterised the resolver over a three-method `Intrinsics` trait with two
implementations, one of which returned a constant. That is indirection for a problem that does not
exist. Work the arithmetic instead:

```
authored  ->  the number
content   ->  min(intrinsic, cap, available)
frame     ->  min(cap, available)
```

At load nothing may be measured, so an intrinsic is taken to be the available extent. Substitute it:
`min(available, cap, available)` is `min(cap, available)`, which is the frame row. **At load, a
content extent and a frame extent are the same expression.** Load does not need a different provider,
a stub, or a trait threaded through every signature; its tree builder supplies availability as the
intrinsic data and the ordinary content arm reduces to the frame expression.

An earlier version of this decision replaced the trait with a `Measure::{Real, UpperBound}` enum
passed into the resolver. Round 9 rejected that, correctly: a stage discriminator handed in by the
caller is structurally `allow_auto_fill` with a better name, and the measurable target below forbids
exactly that. Renaming a flag is not removing it.

The protocol is value-level instead. Each stage first resolves geometry parameters into the same
concrete values map. `source_of` consumes that map and returns an `AxisSpec`: the source plus the
anchor, inset and affine `to` terms needed downstream. The resolver then takes both that classified
axis and the intrinsic **as data**:

```rust
fn resolve(
    axis: &AxisSpec,
    frame: f32,
    available: f32,
    cap: Option<f32>,
    intrinsic: Option<f32>,
) -> f32
```

It has no stage, no mode and no flag; it reads `intrinsic` only in the `content` arm and cannot tell
which stage produced it. `frame` is required to evaluate the affine authored `to` whose slope is
`-1`; its presence is geometry data, not stage awareness. The measured tree is what differs: at
render it is built by measuring, and at load it is built with every intrinsic set to the available
extent, which by the arithmetic above makes `content` and `frame` resolve identically.
Stage-awareness therefore lives only at the tree-building boundary—render measures there, while load
substitutes availability—and never travels into the rules.

What the single resolver buys is the point of the change: caps, offsets, insets, the `to` slope, zero
and negative extents, padding, rotation composition and bounds are computed by the same code at both
stages, instead of by two hand-maintained copies that #150 and #155 have already caught drifting.

Two consequences to state rather than discover. The frame validation resolves against is built from
the template's **declared parameter defaults**, per the frozen `docs/SPEC.md` §3.1 rule this change
does not supersede, so a refusal means "unsatisfiable at the declared defaults", not "unsatisfiable
for every request"; that is today's behaviour (`src/templates.rs:561-570`, `:995-1027`). And an
`image` whose demanded natural size cannot be determined from its dimension metadata loads and fails
per request with `intrinsic_size_undefined`, because a `name`-bound image has no bytes until a request
arrives and refusing a `src`-bound one at load would make the two sources diverge.

### 4. `auto` is a tombstone in `raw.rs`, absent from `models.rs`

`raw::SizeValue` keeps an `Auto` variant that `convert.rs` rejects with a written message ("`auto`
was renamed: use `content` to hug the item's own size, or `fill` to stretch to the frame"), pathed by
`serde_path_to_error` to the exact item and axis. `models::SizeValue` has only `Fixed`, `Content` and
`Fill`, so no code downstream of conversion can encounter it.

The alternative, deleting the variant and taking serde's `unknown variant` error, was rejected
because the quarantine message is the entire user interface for this migration: the author sees one
line in the template-registry error and nothing else. Serde's message names the valid variants but
cannot say which one replaces `auto` for this item's case, and the two cases differ
(`left`-aligned → `content`, `center`/`right` → `fill`). The cost is one dead variant, removable in a
later change once the catalog and the docs have moved on.

### 5. An intrinsic size is content extent times scale, and the engine invents neither

The organising idea that removes the `qr`/`image` special cases is that an intrinsic size is a
content extent multiplied by a scale, and a node has one exactly when both terms are determinable.
Item type supplies those two terms in the render-only `intrinsic` dispatch; once supplied, the sizing
rule does not branch on item type.

`font_size` and `module_size` play the same role: how big one unit of this content is, one em or one
module. The scale used by the intrinsic arithmetic is always template-units-per-content-unit.
`font_size` is authored in points, so its text metrics are converted through the existing
`pt_to_units` path (`font_size / 72` for `in`, `font_size × 25.4 / 72` for `mm`); `module_size` is
already in the template unit. A raster image's scale is one device pixel expressed in the template
`unit` (`1/dpi` for `in`, `25.4/dpi` for `mm`), not the `dpi` number itself. An earlier draft wrote the formula as "extent times the template `dpi`", which is
dimensionally backwards and disagreed with its own scenarios. Text can never lack an intrinsic size because `font_size` is required (`src/raw.rs:174`);
a QR could, because `module_size` is optional. That is a schema inconsistency, not a model one, and
an earlier draft papered over it by inventing a default pitch of four device dots at the template
`dpi`. That number appeared nowhere in the codebase, no author chose it, and it was normative and
pinned by a scenario: the same class of thing as the ten `auto` meanings this change deletes.

It is gone. A `qr` asking for a content or frame extent without `module_size` has not said how big it
is, and that is refused at load. Numeric and constant-`to` QRs are unaffected, since an authored
extent needs no scale.

`module_size` still changes meaning, from a minimum generated-SVG pixel pitch per module to a length
in the template `unit`, and `quiet_zone` from a boolean selecting the encoder's four-module margin to
a count of modules. An author who sizes a QR numerically and sets no positive `quiet_zone` sees no
change; one who sets a positive `quiet_zone` sees the margin become their number of modules, even
inside a numeric box.

That last part is real implementation work, not a relabelling. `build_qr_svg` currently passes
`quiet_zone > 0.0` to `renderer.quiet_zone(bool)` (`src/render/helpers.rs:279-290`), which asks the
encoder for its own four-module margin and discards the magnitude. The generator must instead emit
the symbol with no encoder quiet zone and expand the SVG canvas by `quiet_zone` **modules of the
symbol's own grid** on each side.

Grid units, not template units: the SVG is unitless and is scaled `fit: contain` into whatever box
the item resolves to, so the margin needs no length. That is also what keeps the field meaningful on
a QR that never asks for an intrinsic size, since a numeric or constant-`to` QR may set `quiet_zone`
without setting `module_size`. `module_size` enters only the intrinsic-size arithmetic, which such an
item does not reach; an earlier draft wrote the expansion as `quiet_zone × module_size`, which cannot
be evaluated for exactly that accepted case.

The other way to lack an intrinsic size is to declare no extent at all: an SVG with neither absolute
`width` and `height` nor a `viewBox`. That is a property of the input format rather than of the
layout model, and it fails at render with `intrinsic_size_undefined`, an item-agnostic reason rather
than the image-scoped reason an earlier draft used.

### 6. Image dimensions come from a promoted `image` dependency

`image` is already pinned at `=0.25.10` as a dev-dependency. It moves to `[dependencies]` with only
`png` and `jpeg` enabled, and dimensions are read with a header-only decode.

SVG dimensions are read **per axis**, using the existing `regex` dependency rather than adding
`usvg`: for the axis being asked, its own absolute `width` or `height` if present, else that axis's
`viewBox` extent, else nothing. Not "both dimensions or the viewBox" — that gate would refuse an SVG
carrying `width="20mm"` and no height to an item spelling `size: [content, 10]`, which the spec
accepts and which per-axis demand exists to allow.

An absolute physical unit is converted to the template unit. A unitless dimension or one in `px`
uses the same one-device-pixel scale as a `viewBox` extent. Percentages and font-relative lengths are
not absolute dimensions and fall through to the `viewBox`; without one, that axis has no extent.

A dimensions-header read that fails, on bytes that passed the MIME and base64 checks but whose
dimension metadata cannot be parsed as the format they claim, is the third way an intrinsic can be
unavailable and raises `intrinsic_size_undefined` like the other two. This is not a full-image
validation: bytes with readable dimensions but corrupt later content get through sizing and retain
today's `typst_compile_failed` outcome. An authored-extent image still reaches the renderer without
even the dimensions read, exactly as today.

Alternatives: hand-parsing PNG `IHDR` and JPEG `SOF` markers, rejected as a decoder we would own
without wanting to; adding `imagesize`, rejected because `image` is already in the lockfile.

### 7. One container path

`render_container_item`'s rotated and unrotated branches collapse into one that computes the author
canvas as `swaps_axes() ? (h, w) : (w, h)` and is otherwise identical. The rotated branch exists
today only because `auto` was banned beneath rotation, so it never needed the measurement plumbing;
once intrinsic sizes compose through the swap (spec: "Sizes compose through container rotation") the
distinction disappears. `subtree_uses_auto` is deleted with it.

### 8. There is no circularity check, because there is no circularity

The `fill`-inside-`content` resolution was its own spec requirement in an earlier draft. It is a
proof, not a contract, so it lives here; the spec keeps only the scenario that pins the outcome.


The issue asks for "`auto` whose intrinsic size depends on a stretch descendant is refused at load",
and an early draft of these artifacts specified a `size_circular` refusal. Working the equation
through shows it is unnecessary, and the adversarial review of round 1 said so independently.

Take a `content` container at offset 0 with padding `p` in a frame `F`, holding a `fill` child. The
container's constraint is `F`; the child's constraint is `F − p`. The child reports its intrinsic `I`
upward, because that is what `fill` does. The container's intrinsic is `p + I`, its resolved extent
is `min(p + I, max_w)`, its inner box is `I`, and the child then resolves to `I`. Every step is a
function of quantities already known when it is evaluated. No fixed-point iteration, no ordering
constraint, and nothing to refuse.

The visible semantic is that `fill` and `content` are indistinguishable under a hugging parent, which
is the same shrink-to-fit behaviour CSS has. It is worth a scenario precisely because it surprises
people.

This also disposes of the harder question the round-1 review raised, whether such a check would have
to compose through a rotated container's axis swap. A check that does not exist has no axis mapping
to get wrong.

### 9. ADRs

Three, all new:

- **ADR-0080, "One size-resolution protocol: intrinsic and resolved."** Supersedes ADR-0026
  (auto-length dynamic width), ADR-0053 (`max_*` caps), ADR-0054 (auto fallback position) and
  ADR-0059 (auto-length text box is the alignment slot). Amends ADR-0036 by lifting its §5 `auto`
  ban and ADR-0051 by retiring its §4 clause 1 and §10 `LengthMode`, and by closing its §11 deferral
  (#149, closed as superseded by #226 on 2026-08-25).
- **ADR-0082, "Text overflow is an authored policy, not a consequence of the format."** Records the
  `overflow` field, its `ellipsis` default and `fail` value, the rule that both raise
  `text_does_not_fit` when no shortening can make the content fit, why clipping is neither a value
  nor a fallback, and how that coexists with ADR-0050's ink-outside-the-band clipping. It also records the unconditional line-breaking rule of decision 2,
  since that is what makes the policy the only remaining variable.
- **ADR-0081, "The size vocabulary is `content` and `fill`."** Records the rename, why `auto` is
  refused rather than re-meant, and the `to`-as-stretch-with-inset equivalence with its orientation
  split.

ADR-0080 also carries the reason-set decision `docs/SPEC.md` §10.1 requires: withdrawing
`size_auto_without_max`, `size_auto_no_room`, `container_padding_no_room` and
`auto_length_cursor_mismatch`, and adding `intrinsic_size_undefined` and `text_does_not_fit`, is a
change to the contract that §10.1 says must be recorded against ADR-0052. `text_does_not_fit` is recorded against
ADR-0082 with the policy that raises it, and the remaining five movements against ADR-0080.

All three add rows to `docs/adr/README.md`. **Numbering hazard:** `0070` is claimed by three unmerged
worktrees (`issue-197`, `issue-200`, `issue-212`) and `0071` by `issue-210`; `0067` is an unused gap
in `main` that this change deliberately leaves alone rather than backfilling an append-only series.
Re-check the highest number on `main` immediately before writing the files, and take three
consecutive free numbers.

### 10. Why the `to` orientation split exists, since two drafts got it wrong

Decision 0 states the slope framing; this records why it is not obvious, because the same mistake was
made twice and a reader will otherwise make it a third time.

ADR-0051 §5 established that a `to` axis is frame-dependent when exactly one corner is edge-relative.
That predicate is correct for "does the frame survive into the extent" and insufficient for "is this a
stretch node", because the two XOR orientations have opposite slopes:

```
at plain, to edge-relative:   extent = F + to - at     slope +1, grows with the frame
at edge-relative, to plain:   extent = to - at - F     slope -1, shrinks with the frame
```

The first draft mapped both to `fill`, which is wrong for the second: at `at: [-20]`, `to: [90]`,
`F = 100`, the true extent is 10 while `fill` gives 80, and past `F = 110` the true rectangle inverts
while the `fill` node stays positive. Both current resolvers preserve the actual subtraction
(`src/templates.rs:1534-1545`, `src/render/mod.rs:1975-1979`).

The second draft fixed the algebra and then stated the slope `-1` refusal in physical-axis terms,
which a quarter turn defeats: a rotated container's author height is its physical width, so an
unresolved width becomes an unresolved author height. The refusal therefore needs the propagated
resolved-axis state and its rotation swap, not a sentence about the page format.
`models::Placement::width_is_frame_dependent` is a purely syntactic x-axis predicate
(`src/models.rs:496-504`) and cannot express either correction.

### 11. `overflow` is a field, not a consequence

Splitting "what does this text do when it does not fit" out of the sizing rules came from the round-3
discussion, and it is what makes decision 2's unconditional rule affordable. Three concerns were
tangled in one code path: whether to wrap (`multiline`, already a field), what size to use
(`font_size`, already a field), and what to do with the remainder (nothing, inferred from the page
format). The third becomes `overflow: ellipsis | fail` on `text`.

`ellipsis` is the default because it is what every template in this repository using a `font_size`
range already gets, which is all of them but one. The exception matters: `avery5163_asset_tag.yaml`
carries four fixed-`font_size` multiline items (`:91`, `:98`, `:141`, `:148`) that take the unfitted
`FontSize::Fixed` arm today, so their emitted Typst and possibly their visible output change here.
That fixture is one part of this change's visual acceptance. `fail` is the
value that did not previously exist in any spelling, and it is the one an author printing from a
spreadsheet actually wants, because a silently shortened part number is worse than a rejected label.

`clip` is deliberately **not** a value, and clipping is not a fallback either. Mid-glyph clipping is
not a policy anyone chose; it is what the renderer does when nobody decided, and reifying it, or
quietly falling back to it, would preserve the accident under a nicer name.

An earlier draft kept it as a backstop for the two cases trimming cannot resolve: a box narrower than
`...`, and a box shorter than one line at a fixed size. That was wrong for the same reason the rest
of the accident was wrong. If `ellipsis` means "make it fit", then a box where nothing fits is a
failure of the policy, and reporting it is the honest outcome. Both cases now raise
`text_does_not_fit`, which is also what the two policies converge on: they differ in *when* they give
up, not in what happens then.

The box stays `clip: true` in the emitted Typst, but only as a containment guarantee for ink the
metric model cannot see: ADR-0050 already says centred text can clip below `1.21 × font_size` and
that outlier glyphs ink outside the ascender/descender band at any alignment. Those are unchanged and
unreachable by any policy evaluated on metrics.

This adds a field to `raw.rs`, `models.rs`, `convert.rs` and `src/openapi.rs`, per the
three-files-together rule.

## The measurable target

Prose about elegance is not evidence, so here is what the implementation must actually look like when
it is done. These are counts taken from the current tree and they are checkable at review time.

**Today.** Nine functions compute an extent, across two files:
`templates::{resolve_to_extent, resolve_size, resolve_size_value}` and
`render::{resolve_to_extent, measure, measure_box_height, measure_container_footprint, resolve_size,
resolve_size_value}`. Roughly eighteen non-test sites decide what a size spelling means, through
`is_auto()`, `width_is_frame_dependent()` or the `allow_auto_fill` flag, spread across
`models.rs`, `templates.rs` and `render/mod.rs`. Two of those functions are near-copies of two
others, which is the duplication `docs/SPEC.md` §7 admits to.

**After.** Five functions, in one module:

**Four shared functions**, called identically from both stages and unable to observe which one they
are in:

| Function | Job |
| --- | --- |
| `source_of(placement, axis, geometry_values)` | return an `AxisSpec` classifying `Authored` / `Content` / `Frame` and retaining the anchor, inset and affine `to` terms; this is the only semantic inspection of `SizeValue` / `Extent` |
| `available(frame, axis_spec)` | the space an item has from its anchor |
| `resolve(axis_spec, frame, available, cap, intrinsic)` | the three-arm table in decision 0, including evaluation of an authored affine `to`; `intrinsic` is data, not a mode |
| `requirement(axis_spec, claim)` | the frame-requirement rule from the already-classified axis |

**One render-only function**, `intrinsic(item, axis, box)`, which dispatches by item type and is the
single place item type is visible in sizing at all. Load never calls it: the load walk supplies the
available extent in its place, which by the arithmetic in decision 3 makes `content` resolve exactly
as `frame` does. An earlier draft listed all five as shared, which is impossible — a stage-blind
`intrinsic` cannot both measure at render and synthesise availability at load.

Decision sites for "what does this spelling mean" collapse to one: `source_of`. Everything downstream
branches on `AxisSpec`, never on `SizeValue` or `Extent`. The concrete geometry-values map is data:
load builds it from `default → min → 0`, render builds it from resolved request parameters, and
neither changes a sizing rule. `allow_auto_fill` disappears rather than being renamed, and
`templates.rs` gains no sizing logic of its own.

**Stage-awareness, stated honestly.** Load genuinely cannot measure, so the load and render tree
builders necessarily differ in what intrinsic data they supply. The target is that this distinction
lives only at that boundary and that both builders call the same rules in the same order.
`resolve`, `source_of`, `available` and `requirement` take no mode argument and cannot observe the
stage. An earlier draft of this section demanded "no caller-supplied flag" anywhere, which was not
achievable and which the same draft then contradicted by passing one into `resolve`.

**How to judge it.** After apply:

The obvious check is a `grep` for `is_auto`, `allow_auto_fill` and `width_is_frame_dependent`
outside tests, expecting hits only in `source_of`. That check is **necessary but gameable**: the same
decisions re-spelled under new predicate names pass it, as does splitting load and render into two
similarly-shaped tree builders with no literal stage parameter. It catches a lazy port, not a
determined one. So it is the first of four, and the structural ones carry the weight:

1. `grep` for the three present names outside tests: hits only in `source_of`.
2. **Variant and call-site inventory.** Enumerate every inspection of `models::SizeValue` and
   `models::Extent` in non-test code, including helper methods and destructuring rather than only
   literal `match`/`if` syntax. `source_of` must be the only site that assigns sizing semantics or
   performs geometry from them. Mechanical construction, serialization, reference validation and
   parameter instantiation may inspect the variants, but each such site must be listed and must not
   classify a source, resolve an extent, apply a cap, compute availability or compute a requirement.
   Separately inventory every caller of the four shared functions: both tree builders must use the
   same functions, in the same order, with no parallel arithmetic before or after the calls.
3. **Signature inventory.** No function in the resolver module takes a stage, mode, or
   `allow_*`-shaped parameter, and none is generic over a provider. `resolve` takes the intrinsic as
   `Option<f32>` data.
4. **Cross-file inventory.** No sizing rule is expressed in both `templates.rs` and `render/mod.rs`.
   `templates.rs` should call the resolver and hold no arithmetic of its own.

Check 2 is the one that actually proves the claim, because it inventories semantics and the shared
call graph rather than assuming every legitimate schema-plumbing inspection can be deleted or relying
on what a predicate is called.

**What this does not claim.** Net line count in `src/` is not expected to fall much. The change
deletes `LengthMode`, `MeasuredText` and its cursor, `item_anchor`, `subtree_uses_auto`,
`measure_box_height`, `measure_container_footprint`, the old model sizing predicates, the duplicated
rotated container path, both parallel resolvers and four reason variants; it adds a measured tree, a
per-axis resolved bit, an `overflow` field and an image decoder dependency. The win claimed here is
branch count and the elimination of two implementations of one rule, not size. `render/mod.rs` being
7092 lines is not the problem this change fixes.

## Risks / Trade-offs

- **The diff touches every layout path at once.** → The specs make behavior changes explicit and
  narrow; everything else is guarded by a byte-identical-source assertion over the catalog and the
  test fixtures. Any diff in emitted Typst for a template the specs do not name is a bug, not a
  judgement call.
- **Load-time validation cannot see intrinsic sizes** (decision 3), so a class of "this will never
  fit" template still loads and fails per request. → Unchanged from today, and the spec states the
  boundary as a requirement so it is a contract rather than an accident.
- **`module_size` changes meaning** (decision 5). → Zero uses in the catalog and none in fixtures;
  the ADR records it and the migration table in `docs/AUTHORING.md` names it.
- **A new runtime dependency** (decision 6). → Already pinned and already built for tests; only two
  decoders are enabled.
- **Template migration is visual and belongs to this change.** Each of the four catalog tapes is
  rendered to PNG before and after migration, both images are opened, and alignment, auto-shrink, QR
  squareness and clipping are inspected. `avery5163_asset_tag.yaml` is not migrated or edited, but
  its four fixed-font items render differently because of the unconditional text rule, so its
  existing render-and-look acceptance remains as a separate check.
- **The twelve template migrations are judgement calls.** `auto` encoded four behaviours, so each
  file is read against its layout or regression intent rather than mechanically renamed. The QR
  fixtures paired `auto` with `max_w` to obtain exactly that width, a behaviour `content` no longer
  provides. The `brother_24mm_weights` fixture must preserve the #152 assertion that `max_w: 117` at
  `at.x: 1.5` does not perturb a `width.max: 120` tape render.
- **The `src/` test conversion is wide (~45 sites) and each is a judgement.** → `auto` meant
  different things in different places, so a blanket `content` or a blanket `fill` would silently
  change what a test asserts. Each site is read against what its test is for.
- **`quiet_zone` changes magnitude, not only units** (decision 5): a positive value meant four
  modules and now means that many. → One fixture uses `0.0`, which is unchanged; the ADR records it
  and `docs/AUTHORING.md` names it.
- **Two cases that render something today will start returning 422**: a text box narrower than `...`,
  and one shorter than a single line at the chosen size. → Both produce garbage output today (a lone
  clipped marker, a serial number cut through the middle) and neither is reachable by any template in
  this repository at its current geometry. A range shrinks first but can still reach the policy at
  `font_size.min`; it is not exempt. The four fixed-font items in
  `tests/fixtures/templates/avery5163_asset_tag.yaml` need visual acceptance because the entire layout
  pass is new for them, not because only fixed fonts can overflow. `/batch` names the failing row, so
  a bad CSV row is identified rather than printed wrong.
- **The `to` orientation split is subtle** (decision 10) and two drafts got it wrong: first by
  mapping both XOR orientations to `fill`, then by stating the refusal in physical-axis terms that a
  quarter turn defeats. → Scenarios pin all four corner combinations, the fixed-frame case that must
  keep resolving, the hugging-container case, and the rotated case in both directions.
- **A right-anchored item's geometry is easy to get backwards.** `at` is the box's *low* edge, so
  `at: [-a]` leaves exactly `a` of room and an item there can never be wider than its own inset. →
  The available-extent definition is written once and every contribution row is derived from it;
  scenarios assert the inset, not a sum.
- **A user template using `auto` stops working on upgrade.** → Deliberate, per proposal.md. The
  failure is a quarantine with a written migration message, visible in the registry and scoped to
  the one template.
- **ADR number collision with four unmerged worktrees.** → Re-check before writing; the README index,
  not the filename, is the source of truth.

## Migration Plan

1. Land the engine and the vocabulary together; there is no intermediate state where `auto` and
   `content`/`fill` both work, by design.
2. Convert the roughly 45 `SizeValue::Auto(...)` construction sites in `src/` unit tests, choosing
   `content` or `fill` per test intent. A test asserting the old fill-on-fixed-frame behaviour
   becomes a `fill` test; one asserting content sizing becomes a `content` test. These are Rust code
   and cannot be deferred: the variant is gone, so they do not compile.
3. Before any template or engine edit, render and open the four catalog tapes to capture the visual
   baseline. Then migrate `catalog/tape/brother/brother_{9,12,18,24}mm.yaml` and the eight fixtures
   `brother_24mm_lines_divider`, `brother_24mm_multiline`, `brother_24mm_qr`, `brother_18mm_qr`,
   `brother_24mm_printed_on`, `brother_24mm_weights`, `brother_24mm_max_w_cap`, and `homebox-qr` one
   at a time according to each file's intent. This reverses the earlier scope split with
   [#228](https://github.com/pfa230/labeler/issues/228); this design does not assign #228 a remaining
   purpose or status.
4. Update `docs/AUTHORING.md` §4-§8, deleting §5 ("`auto` means two different things") outright, and
   document `overflow`.
5. No data migration and no stored state: templates are files, and the registry re-reads them.

**Sequencing.** This change deliberately moves the twelve repository template migrations back into
#226 so the branch is self-contained and landable. The before images for the four catalog tapes are
captured while the old engine and spellings still work; the engine and error contract are then
implemented, the templates are migrated, and the after images are rendered and inspected during verification.

There is one apply-stage-only registry failure before archive:
`spec_documents_every_reason_and_invents_none` (`src/errors.rs:610`) accepts reason additions from
canonical `openspec/specs/**` and active change deltas, but its phantom half runs from the frozen
§10.1 table alone (`:691-698`). Task 7.3 preserves that asymmetry while adding
canonical withdrawals: the active delta documents the two additions, but cannot suppress the four
withdrawn §10.1 slugs as phantoms. Task 10.2 records that exact temporary failure; archive makes the
withdrawals canonical and removes it without a code edit. Archive precedes the single commit and
push, so branch CI sees the post-archive tree and the full suite must be green. Any other failure,
before or after archive, is a defect.

Rollback is `git revert` of the merge; nothing outside the repository changes.

## Open Questions

- Whether `fill` should eventually gain an explicit inset spelling so `to` is not the only way to say
  "stretch, leaving 2 units". Deferred: `to: [-2.0, h]` says it today, and adding a second spelling
  before #212 shows whether it is wanted would be guessing.
- Whether the blank-edge ordering should survive at all. Blank leading and trailing lines are counted
  while a `font_size` range picks its size and dropped only at emission, so a value with a stray
  newline renders smaller than the same value without one. That is arbitrary, and it is retained here
  only to keep ranged-font output byte-identical. It is the last piece of magic in the text path and
  deserves its own issue rather than a silent change inside this one.
