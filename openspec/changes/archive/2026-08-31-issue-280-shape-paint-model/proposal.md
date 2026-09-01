## Why

Closes [#280](https://github.com/pfa230/labeler/issues/280). A container can be outlined but never
filled, so no layout can paint a solid block. `Frame { thickness, rounded }` (`src/models.rs:943`) is
stroke-only, and the renderer emits it with no fill (`src/render/mod.rs:2085`). Every element on a
label therefore carries the same visual weight, and hierarchy has to come from type size alone.

The narrow fix would be one `fill` key on `frame`. That is not what this change does. The engine has
two shapes today (a container's frame rect and `line`), each with its own private spelling for a
stroke thickness and no way to say what colour anything is drawn in. Adding a fill to one of them
leaves the next shape (a standalone `rect`, an ellipse, a path) to invent a third spelling. This
change instead defines one paint vocabulary, applied to the two shapes that exist: every shape is
stroked, and a shape enclosing an area is also filled.

## What Changes

- **New paint vocabulary, scoped by what a shape is.** `stroke: { thickness, color }` describes the
  outline and is accepted on every shape. `background: <colour>` describes what fills the interior
  and is accepted only on a shape that has one, which is `container`; a `line` encloses nothing and
  refuses it. Where both are accepted they are optional and independent, so a container may be
  stroked, filled, both, or neither.
- **A colour is a value, not a keyword pair.** A colour is a hex string (`#rgb`, `#rgba`, `#rrggbb`,
  `#rrggbbaa`) or one of the named colours. `stroke.color` defaults to black, preserving what a
  thickness-only stroke draws today. This overrules the monochrome constraint #280 proposed; see
  Impact.
- **`container` carries the paint directly.** A container is a rect, so `stroke`, `background` and
  `rounded` sit on the container itself rather than inside a nested `frame` block.
- **`rounded` becomes a radius.** `rounded: <number>` is the corner radius in template units, applied
  identically to the stroke and the background. Omitting it gives square corners. Like `background`
  it belongs to a shape with an interior, so a `line` refuses it too.
- **BREAKING: `container.frame` is removed.** `frame: { thickness, rounded }` no longer parses. Its
  replacement is `stroke: { thickness }` plus `rounded: <number>`.
- **BREAKING: `line.thickness` is removed.** `line` takes `stroke: { thickness, color }` like every
  other shape. A bare `thickness` on a `line` no longer parses.
- **BREAKING: `rounded: <bool>` is removed.** `rounded: true` and `rounded: false` no longer parse.
  `false` becomes an omitted key; `true` becomes an explicit radius.
- A template using any removed spelling is quarantined at load with a validation error naming the
  field, as any other unknown or ill-typed field already is.

Out of scope, deliberately:

- **Text colour.** `text` gets no ink here. That is [#282](https://github.com/pfa230/labeler/issues/282),
  which stays independent of this change in both directions: a filled container renders whatever colour
  its text already renders, reversing text out of a dark ground needs #282 as well, and nothing decided
  here constrains what #282 chooses. In particular, this change's rejection of a monochrome vocabulary
  is a decision about shape paint and is not binding on text ink.
- **New shape item types.** `rect`, `ellipse`, `path` and `svg` shapes are what this vocabulary
  exists to serve, but each is its own decision and its own issue. This change proves the vocabulary
  against the two shapes already in the engine before more are built on it.
- **Gradients, tilings and dashed strokes.** Typst supports all three on the same parameters. Each
  widens the grammar and none is needed to paint a solid block.
- **Which `details.reason` a refusal carries.** Extracted to
  [#289](https://github.com/pfa230/labeler/issues/289) after three diff-review rounds failed to close
  it. A refusal's reason is decided by the stage that caught it (`TemplateError::Yaml` →
  `template_parse_failed`, `Validation` → `template_validation_failed`, `src/errors.rs:561`,
  `src/reason.rs:33`), so a semantically bad but well-formed value refused inside a `Deserialize`
  reports that the YAML did not parse. Fixing it means either moving the two-stage parsing boundary or
  remapping the reason for **every** template refusal, neither of which is a paint decision. This
  change therefore pins no reason code: its scenarios say a bad template "fails validation and is
  quarantined", which is true under both the current mapping and whatever #289 settles.

## Capabilities

### New Capabilities

- `shape-paint`: how a shape declares its outline and its fill. The `stroke` and `background` blocks,
  the colour grammar and its named set, the corner radius, which items accept paint, the geometry the
  paint covers, the draw order against the shape's own children, and what load-time validation
  refuses. Supersedes the `container` and `line` bullets of frozen `docs/SPEC.md` §4.1 under the
  first-touch rule, and carries the complete post-change contract for both.

### Modified Capabilities

Removing `frame` leaves three accepted requirements naming a key that no longer exists, so each gets a
`MODIFIED` delta carrying its complete updated contract:

- `layout-sizing`: "A container establishes a padded frame, and rotation swaps it" states that the
  physical `frame` outline is not rotated (`openspec/specs/layout-sizing/spec.md:622`). The clause is
  respelled to name the container's `stroke` and `background`; the rotation behaviour it describes is
  unchanged.
- `flow-layout`: "A packed child carries no position and is sized by `layout-sizing` alone" lists
  `frame` among the keys a packed container child may carry
  (`openspec/specs/flow-layout/spec.md:197-200`). The list is respelled.
- `flow-layout`: "Packing places the children that take up room along the primary axis" requires that
  a zero-extent child's `frame` stroke still be drawn
  (`openspec/specs/flow-layout/spec.md:361-363`, and its scenario at `:380`). The clause is respelled;
  the guarantee is unchanged.

None of the three changes what the system does. Each replaces a spelling this change removes, which is
exactly what the archive gate checks for: a requirement whose name drifts rewrites the wrong
requirement silently.

A word on one that is **not** modified. `layout-sizing` already defines `fill` as a **sizing** keyword
(`size: [fill, content]`, ADR-0081). That is why the paint key is named `background` and not `fill`:
one word, one meaning, inside one item. No sizing requirement changes.

## Impact

- **Schema, breaking.** Three spellings are removed. In-repo, `frame:` appears in one template file
  (`tests/fixtures/templates/avery5163_asset_tag.yaml:48`, with `rounded: false`) and in a template
  embedded in an acceptance test (`tests/acceptance_issue_263.rs:566`); `Frame` is constructed
  directly in five unit tests (`src/render/mod.rs:3799`, `:3839`, `:4015`, `:4739`, `:7197`, all
  inside `mod tests` at `:2148`); `line.thickness` appears across fixtures and unit tests. Operator templates in a live
  `LABELER_CONFIG_DIR` are the real exposure: each one using a removed spelling is quarantined at
  startup until it is edited. Quarantine is per template and non-fatal (#175), so a server with such
  a template still starts and still serves every other template. `docs/DEPLOY.md` gains the upgrade
  note.
- **The monochrome question, reversed.** #280 and #282 both argued for a monochrome-only vocabulary,
  on the grounds that the renderer targets mono laser and thermal media and a palette invites
  templates that cannot print. This change rejects that: the model must extend to arbitrary shapes
  and SVGs, and a two-keyword vocabulary is a grammar that has to break to widen. The print path
  is where device conversion belongs: ADR-0033 (Accepted) has labeler read the printer's capabilities
  and drive render quality from them, so constraining the *template* vocabulary would make the same
  decision a layer up with less information. What ships today is narrower than that ADR: a bi-level
  driver printing a single label gets a PNG binarized at a fixed 0.5 luminance threshold with no
  dithering (`src/driver.rs:19-27`, `src/render/helpers.rs:15-26`), and every sheet or PDF path
  applies no binarization at all. The exposure this change opens is therefore concrete: mid-tones
  collapse to black or white on bi-level single labels, and colour passes through untouched on PDF.
  Both belong to the print path to improve. ADR-0092 records this decision **for shape paint**: it
  rejects the monochrome constraint as a rule about `stroke` and `background`, on the reasoning above,
  and it decides nothing about text. #282 remains free to reach its own conclusion about ink, monochrome
  or otherwise, and ADR-0092 neither grants nor forecloses it.
- **Code.** `raw.rs` (new `StrokeRaw`, colour parsing, presence-preserving optionals,
  `ContainerRaw`/`LineRaw` field changes), `render/helpers.rs` (the length emitter's precision bound),
  `models.rs` (`Stroke`, `Color`, `Frame` deleted), `convert.rs` (`TryFrom`), `templates.rs`
  (validation), `render/mod.rs` (`#rect` gains `fill`, `stroke: none`, decoupled `radius`;
  `#line` stroke gains a colour), `openapi.rs` (register `Stroke`, and `Color` as a **string** schema carrying the canonical
  `#rrggbbaa` form the API contract promises, not its internal RGBA storage; `Frame` is not in the
  registration list today, so nothing is dropped there).
- **Docs.** `docs/AUTHORING.md` §9 documents frames by worked example and is rewritten;
  `docs/adr/0092-*.md` plus its row in `docs/adr/README.md`.
- **No new dependency.** Typst 0.15 already accepts `fill`, `stroke: none` and `radius` on `rect`,
  a `length + color` stroke on `line`, and `rgb("#abc")` for 3, 4, 6 or 8 hex digits.
