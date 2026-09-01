## Context

See `proposal.md` for motivation. What shapes the approach:

- **The paint has one call site today, and it is stroke-only.** `render_container_item` emits
  `#rect(width, height, stroke: {thickness}, radius: {thickness * 2 or 0})` (`src/render/mod.rs:2085`)
  when `frame` is present, and `render_line_item` emits `#line(..., stroke: {thickness})`
  (`src/render/mod.rs:2020`). Nothing else draws a boundary.
- **Draw order is already correct.** The frame `#rect` is written to the Typst source *before* the
  container's child box (`src/render/mod.rs:2089` then `:2101`), and Typst paints `#place` calls in
  source order. A `fill:` on that same rect lands behind the children with no reordering.
- **The painted rectangle is already the outer box.** The rect uses `pbox.w`/`pbox.h` at
  `pbox.x`/`dy`, and padding is applied inside, to the child source. So "the paint covers the padding
  band" is the existing geometry, not a new one.
- **The rotation split already exists.** The rect is emitted outside `wrap_rotation`, so a rotated
  container's outline is already unrotated (frozen `docs/SPEC.md` §4.2). The background inherits that
  for free.
- **`Frame` is not a two-stage type.** `raw.rs:253` declares `pub frame: Option<Frame>` importing the
  *domain* struct from `models.rs`, so the wire format and the validated model are the same type, and
  `Frame` (`models.rs:942`) carries no `deny_unknown_fields`. `frame: { thickness: 0.02, bogus: 1 }`
  parses today. This change does not preserve that.
- **Typst 0.15 needs nothing added.** `#rect` takes `fill: color|none`, `stroke: length + color|none`
  and `radius: relative length`; `#line` takes the same stroke; `rgb("#abc")` accepts 3, 4, 6 or 8
  hex digits; a stroke given only a thickness paints black.
- **Blast radius, counted rather than assumed.** `frame:` as YAML appears in
  `tests/fixtures/templates/avery5163_asset_tag.yaml:48` and in a template embedded in
  `tests/acceptance_issue_263.rs:566`. `Frame` is constructed directly in five unit tests
  (`src/render/mod.rs:3799`, `:3839`, `:4015`, `:4739`, `:7197`), all inside `mod tests` (`:2148`),
  so no production code outside `render`/`convert`/`templates` builds one. No `ui/src/` code reads
  any of these fields. Operator templates outside the repository are the real exposure.

ADR: this change adds **ADR-0092, "A shape carries a stroke and a background, in any colour"**
(following ADR-0091 on `main`). It records the paint vocabulary, and it records
the reversal of the monochrome constraint that #280 and #282 both proposed, with the printability
cost that reversal accepts. It supersedes no earlier ADR. Its row goes in `docs/adr/README.md` in the
same commit.

## Goals / Non-Goals

**Goals:**

- One paint vocabulary that a shape added later adopts unchanged, with no per-item spelling.
- Exactly one spelling per concept where a concept is a switch: one way to say "no outline", one way
  to say "square corners". A colour is a value rather than a switch, so it has one grammar accepting
  several equivalent spellings (a name, short hex, long hex, alpha-bearing) and exactly one canonical
  spelling on read-back.
- A corner radius that means the same thing whether or not the shape is stroked.

**Non-Goals:**

- Making `text` legible on a filled ground. That needs #282 and is not attempted here; a container
  filled black today hides black text inside it, and this change does not soften that.
- A shape's paint influencing its layout. Neither `stroke` nor `background` participates in size
  resolution: `layout-sizing` is untouched, and a stroke does not grow, inset or reserve anything.
- Gradients, tilings, dashes, caps and joins. Typst offers all of them on the same parameters; none
  is needed to paint a block, and each widens the grammar the spec has to hold.

## Decisions

### One vocabulary on the item, not a `frame` block per shape

`stroke` and `background` sit directly on the shape. A container *is* a rect; a `frame` block treats
the container's own boundary as a sub-object it owns, which is why `line` could not share it and
spelled its thickness bare instead.

*Alternative: add `color` and `fill` inside the existing `frame`, and `color` beside `line.thickness`.*
Smallest diff, and rejected: it produces no shared type, so the third shape invents a third spelling,
which is precisely the trajectory this change exists to stop.

*Alternative: a nested `paint: { stroke, fill }` block.* Keeps the word `fill` usable by nesting it
away from the sizing keyword. Rejected for the indentation it adds to every painted shape and for the
second container future shapes would each have to carry.

### `background`, not `fill`

`fill` is already a sizing keyword in this schema: `size: [fill, content]` (ADR-0081, `resolver.rs`).
Naming the paint key `fill` would put two unrelated meanings of one word inside one item, where the
reader disambiguates by which key it sits under. `background` is unambiguous and needs no nesting to
stay that way.

### The superseded spellings are removed, not aliased

`frame`, bare `line.thickness` and boolean `rounded` stop parsing. The alternative, desugaring them
into the new keys the way `options:` and `container.option` are desugared (`convert.rs:284`), keeps
every existing template working and was rejected: a desugaring is a second parse path, a second thing
to hold in mind when the next shape is added, and a second contract for the spec to state. This
project's stated line is that an exception needs a demonstrated failure of the uniform rule, and
"templates keep working" is the cost of the uniform rule rather than a proof against it.

The cost is stated rather than hidden. In-repo it is the full inventory in Context above: one YAML
fixture, one template embedded in an acceptance test, five direct `Frame` constructions in unit tests,
and the `line.thickness` sites. In a live `LABELER_CONFIG_DIR` it is every operator template using a
removed spelling, each of which is quarantined at startup with an error naming the field (#175). Quarantine is per template, so the server starts and every other
template still renders. `docs/DEPLOY.md` carries the upgrade note.

### Arbitrary colour, over the monochrome vocabulary both issues proposed

#280 and #282 each scoped fill and ink to monochrome, arguing a palette invites templates that cannot
print on mono laser and thermal media. This change rejects that argument on the ground that a
two-keyword vocabulary is a grammar, and widening a grammar later breaks every template written
against it, whereas a colour that a given printer renders as grey costs the author one reprint.

The stronger reason is that the constraint would sit in the wrong layer. ADR-0033 (Accepted) places
ownership of device conversion in the print path: labeler reads the printer's capabilities and drives
render-quality parameters from them rather than handing a device an artifact its own filter must
threshold. A monochrome *template* vocabulary would make the same decision one layer up, with strictly
less information about the device.

**What is implemented today is narrower than that ADR, and the difference matters here.** The print
path selects PNG only for a bi-level driver printing a single label, and PDF otherwise
(`src/driver.rs:19-27`); the PNG path then binarizes with a fixed Rec.601 luminance threshold at 0.5,
explicitly with no dithering (`src/render/mod.rs:719-720`, `src/render/helpers.rs:15-26`). ADR-0033's
`dither_policy` and PWG `black_1` emission are architecture, not shipped code.

So the printability exposure this change opens is specific, and worth stating rather than waving at:
a mid-tone colour on a **bi-level single label** is thresholded at 0.5 luminance, so it lands as pure
black or pure white with no halftone, and two distinct colours either side of the threshold become
indistinguishable. On every **sheet or PDF** path there is no binarization at all, so the colour
reaches the device and whatever the device's own filter does with it is outside labeler. Neither is a
reason to constrain the schema: both are the print path's to improve, and ADR-0033 already says so.

ADR-0092 records this as the decision it is, so a later reader finds the reversal argued rather than
drifted. Its scope is **shape paint**, and it says so: it reverses the monochrome constraint #280 was
filed with, and it deliberately decides nothing for #282. Text ink is a separate contract with separate
reasoning available to it (a light ink on an unfilled ground is invisible, which is a validation
question shape paint never faces), and an ADR for this change that bound #282 in advance would be
deciding an issue nobody reviewed under it.

### Every numeric paint value is checked finite, not merely positive

`thickness` and `rounded` are `f32`, and YAML admits `.nan` and `.inf`. `format_length`
(`src/render/helpers.rs:245`) formats whatever it is given, so a NaN thickness would reach the Typst
source as the literal `NaNmm` and fail at compile time on some later request rather than at load. The
existing analogous checks test only `<= 0` (`src/templates.rs:1867`, `:1967`), which NaN passes, but
the project already checks finiteness where it matters (`src/convert.rs:136`, `:147` on flow gaps).
Both paint values follow that precedent, and the spec carries the refusal scenarios.

There is a second bound in the same place, and it is about precision rather than validity.
`format_length` emits at four decimal places (`src/render/helpers.rs:253`), so a thickness of
`0.00001` formats as `0` and the stroke disappears: the template validates and then draws nothing,
which is exactly the outcome the "one spelling for no outline" rule exists to prevent.

The bound is the emitter's **quantum**, not the cliff. Four decimal places means the only emittable
lengths are multiples of `0.0001`, and the value that first survives is around `0.00005`, where
round-half-even carries it up to `0.0001` (`0.00006` emits `0.0001`; `0.00004` emits `0.0000`).
Setting the floor at the cliff would accept `0.00006` and render it as `0.0001`, which is a value the
author did not write, so the floor is one whole quantum instead: every accepted value then renders at
the thickness it declares, to the precision the emitter has. Rather than widen the formatter, which
would change how every length in the engine is emitted for the sake of magnitudes no device can print,
the contract states that floor on both `thickness` and `rounded` and refuses anything smaller at load.

### A colour parses at load into one canonical form

A colour is validated and normalized to RGBA at load, not carried as a string to the renderer. The
renderer then emits exactly one form, `rgb("#rrggbbaa")`, for every colour including named ones, so
the Typst layer never learns the name set and there is one code path from any spelling to any output.

The consequence is that a template read back through the API (`TemplateDetail`, `src/models.rs:72`)
reports `color: "#ff0000ff"` where the author wrote `red`. That is a normalization, not a loss, and it
is what makes the name set a load-time concern only. Because it is externally visible, the spec states
it as a requirement rather than leaving it to fall out of the implementation.

*Alternative: pass the author's string through to Typst.* Rejected: it makes Typst the validator, so
an unknown name surfaces as a render-time compile failure on some later request rather than as a
quarantine at load.

### The named set is ours, with its values written into the contract

The sixteen names are the CSS Level 1 names carrying their CSS values, and the spec states each value
rather than deferring to anything.

The first draft of this design proposed mirroring the renderer's own constants instead, on the
argument that a mirror cannot drift. Two facts kill it. First, Typst's constants are not the CSS ones:
its `red` is `#ff4136` and its `yellow` is `#ffdc00`
(`typst-library-0.15.1/src/visualize/color.rs:118-130`), so an author writing `red` would get a colour
they did not ask for. Second, the drift the mirror was protecting against cannot happen: because a
colour normalizes to RGBA at load and the renderer is handed `rgb("#rrggbbaa")`, Typst never sees a
name, so there is no second table to disagree with ours. The mirror bought nothing and cost the
principle of least surprise.

*Alternative: hex only, no names.* One grammar and no table at all. Rejected because
`background: black` is what an operator hand-editing a YAML template will write, and refusing it buys
only the deletion of a sixteen-row table that never changes.

### The radius is authored, and the clamp is stated

`rounded` is a number, so the radius no longer derives from `thickness * 2.0` (`render/mod.rs:2079`),
a derivation that yields a square corner exactly when there is no stroke, which is the fill-only block
this change exists to make.

A radius larger than half the shorter side is clamped rather than refused, and the spec says so. It
cannot be refused at load in general: a shape's extent may resolve from its content or its frame
(`layout-sizing`), so the side length is not known when the template is validated. Refusing only when
the extent is authored would be a second code path keyed on the extent's source, which is the kind of
carve-out this project refuses; clamping always, and stating it in the contract, is the uniform rule.

**The renderer performs the clamp itself, before emitting.** Typst also clamps, but to
`min(w, h) / 2 + min(half stroke width)` (`typst-layout-0.15.1/src/shapes.rs:764-772`), which is
stroke-dependent and therefore reintroduces exactly the coupling this decision removes. Clamping to
half the shorter side ourselves, where the resolved box is known, keeps the contract a single
stroke-independent sentence and means Typst's own clamp is never reached.

### One scenario name in the `flow-layout` delta keeps a word the change removes

`flow-layout`'s scenario "A zero-extent child still draws its frame and still raises its errors" keeps
that title verbatim, though its body now says `stroke`. This is a tooling constraint, not an oversight:
`openspec validate` resolves a MODIFIED requirement's scenarios by name and reports a renamed one as a
scenario the delta drops, refusing the change. Renaming it is therefore impossible in the same delta
that respells its body. The title is prose and carries no contract; the normative WHEN/THEN lines are
correct. Renaming it belongs to a later change against the archived requirement, and is not parked here
as a task.

### A stroke is centred on the boundary, and that is stated rather than left implicit

Typst draws a rect's stroke centred on the boundary: it halves the thickness when computing corner
geometry (`typst-layout-0.15.1/src/shapes.rs:763`, `:765-768`). Half the stroke therefore lies
outside the shape's resolved box. This is already true of today's `frame` and nothing about it
changes here, but the spec now says it, because the alternatives are both worse: insetting the stroke
would make a shape's ink depend on its stroke thickness, and outsetting it would make a stroke grow
the shape, which would drag paint into size resolution.

The consequence the contract has to carry is that the outer half can be clipped. A container's frame
rect is emitted into its parent's coordinate space, and each container's own content is placed inside
a `clip: true` box (`src/render/mod.rs:2101`), so a nested shape's overhanging ink is clipped by its
parent's box and a top-level shape's by the label. A 1.0-unit stroke on a container flush with the
label edge shows 0.5 units of ink, not 1.0. That is stated in the spec with a scenario rather than
discovered by an author.

### `Stroke` and `Color` are proper two-stage types

Every optional paint key is **presence-preserving**: `Option<Option<T>>` behind
`deserialize_present_typed` (`src/raw.rs:65`), the pattern this repo already uses for `flow`
(`src/raw.rs:257`). A plain `Option<T>` maps an explicit `null` onto `None`, which would make
`stroke: null` a silent second spelling of "no outline" and `color: null` a silent second spelling of
"black", contradicting the single-spelling rule the whole design rests on. `Some(None)` makes the
null visible, and `convert.rs` refuses it with the field's path attached.

`StrokeRaw` and the colour's `Deserialize` live in `raw.rs` with `deny_unknown_fields`, and convert to
`models::Stroke` and `models::Color` through `TryFrom` in `convert.rs`, with `serde_path_to_error`
attaching the JSON path as it does for every other field. This restores the architecture's stated
two-stage rule where `Frame` quietly broke it, and it is what makes `stroke: { thickness: 1, bogus: 2 }`
an error rather than a silently ignored key.

### The reason code a refusal carries is not decided here

The contract says a bad template "fails validation and is quarantined" and deliberately names no
`details.reason`. An earlier draft required every paint refusal to report
`template_validation_failed`; three diff-review rounds could not close it, because the reason follows
the stage that caught the error rather than what went wrong (`src/errors.rs:561-564`,
`src/reason.rs:33-34`), and a colour or an unknown `stroke` key is refused inside a `Deserialize`.
Satisfying it meant moving the parse/validate boundary or changing the wire contract for unrelated
refusals. Extracted to #289, which owns the error contract across every template field. Nothing in
this change depends on which reason lands, because no requirement here names one.

## Risks / Trade-offs

- **An operator's templates stop loading on upgrade.** → Quarantine is per template and non-fatal:
  the server starts, unaffected templates render, and the error names the offending field and file.
  `docs/DEPLOY.md` gets the upgrade note with the three replacements spelled out.
- **A filled container hides the text inside it until #282 lands.** → Real, and not mitigated. This
  change delivers the ground, not the reversed block; the proposal says so and #282 is filed.
- **Alpha reaches print.** → Accepted under the same reasoning as colour: a 50% black composites over
  white and prints as grey, which is a legitimate thing to want and a foreseeable thing to regret.
  Stated in the spec rather than refused.
- **The schema owns a name table the renderer disagrees with.** → The sixteen CSS names and values are
  ours, and Typst's constants of the same names differ (`red` is `#ff4136` there). Because a colour is
  normalized to RGBA at load and emitted as `rgb("#rrggbbaa")`, Typst is never asked to resolve a name,
  so the disagreement cannot surface in output. The exposure is a maintainer reading the two tables and
  assuming they match; ADR-0092 records that they deliberately do not.
- **The rendered result cannot be gated.** → A fill, a radius and a colour are visual, and this
  project holds that no checked box may claim a render-and-look loop (#220). Unit and HTTP tests
  assert the generated Typst source and that both PNG and PDF compile; that a rounded black block
  *looks* right rests on whoever implements it, verified by rendering and opening the image, and
  claimed nowhere.

## Migration Plan

1. `tests/fixtures/templates/avery5163_asset_tag.yaml:48` moves from `frame: { thickness: 0.02, rounded: false }`
   to `stroke: { thickness: 0.02 }`, dropping `rounded` entirely.
2. `tests/acceptance_issue_263.rs:566` embeds a template using `frame: { thickness: 0.5, rounded: false }`;
   it moves to `stroke: { thickness: 0.5 }`. That test asserts the `flow-layout` guarantee this change
   respells, so it is the executable half of that MODIFIED delta and must keep asserting the same
   thing.
3. The five `Frame { … }` constructions in `src/render/mod.rs` unit tests (`:3799`, `:3839`, `:4015`,
   `:4739`, `:7197`) become `Stroke { … }` plus an explicit `rounded` where the test meant a radius.
   `:4739` is the only one passing `rounded: true`, so it is the only one that must choose a number.
4. Fixtures and unit tests using bare `line.thickness` move to `stroke: { thickness: … }`.
5. `docs/AUTHORING.md` §9 is rewritten: it documents `frame` by worked example twice.
6. `docs/DEPLOY.md` gains an upgrade note listing the three removed spellings and their replacements.

No data migration exists to perform: the service is stateless and templates are files an operator
owns. Rollback is reverting the commit; a template edited to the new spelling stops parsing on the
old build, which is the mirror of the same breaking change and is why the upgrade note names both
directions.
