## Context

See `proposal.md` — Why. The fitter's reservation lives in one function, `overflow_em`
(`src/render/helpers.rs:993`), consulted from two places since #226 unified the layout pass:
`block_height` (`:1014`), which decides whether a candidate size fits and so answers `text_fits`, and
the `max_lines` inverse of that formula (`:742`). `Center` returns `0.0` from both. Placement is a separate constant, `pad_em`/`pad_pt`, which ADR-0050 kept deliberately distinct:
the pad appears in the generated Typst source, the reservation never does.

Two constraints shape the approach. ADR-0045 forbids letting position or spacing depend on which
glyphs the data carries, which rules out measuring per-string ink bounds. ADR-0050 rejected changing
Typst's line-box edges to `ascender`/`descender` on measured evidence: it stretches every multi-line
block by 35% and clips fixed-size templates that fit today.

Since #226 the same predicate also gates ADR-0082's overflow policy: `text_fits` decides whether
`overflow: fail` raises (`:716`), and a separate check refuses any item whose box cannot hold one line
at the chosen size (`:733`). Widening the reservation therefore widens what those two refuse, which is
the sharpest edge of this change and is why the spec states it as a requirement rather than leaving it
as a side effect.

The contract also already exists in part. #226 synced `openspec/specs/layout-sizing/spec.md`, whose
`Text is laid out against the box it will get, and what does not fit is authored` requirement owns
the fitting pipeline and the `overflow` policy and states today's centred behavior as normative
("centred text is not inset and can still clip in a slot shorter than `1.21 × font_size`",
`openspec/specs/layout-sizing/spec.md:686-691`). This change therefore MODIFIES that requirement
rather than adding a parallel capability beside it: two requirements prescribing incompatible
outcomes for the same input is not a contract. The reservation's own definition is still frozen-spec
territory, so it arrives as an ADDED requirement in the same capability under the first-touch rule.

## Goals / Non-Goals

**Goals:**

- A `center`-aligned block the fitter accepts has its font-declared ink inside the clipped box, at
  whatever size the fitter picked, with no authored inset.
- One definition of "fits" across size selection and line budget.
- No placement change, so a template with height to spare renders byte-identically.

**Non-Goals:**

- Containing glyphs that ink outside the font's own ascender/descender band (211 and 197 of Inter's
  glyphs do). The spec's second requirement says so.
- Any template-authored control over the reservation. `bottom-edge: "descender"` was #124's idea and
  #124 was closed without it; a reserve that costs nothing unless height binds needs no opt-out.
- Reclaiming the 0.7–1.0 mm container padding the catalog tapes carry. Those templates are unaffected
  by this change and re-tuning them is separate work.

## Decisions

### 1. `center` reserves `2 × max(u, d)`, not `u + d`

The block is centred on its **metric** box, so a symmetric reservation is split evenly and each side
gets `reserve / 2`. Containment therefore needs `reserve / 2 ≥ u` and `reserve / 2 ≥ d`, i.e.
`reserve ≥ 2 × max(u, d)`. The sum is sufficient only when `u = d`, or when the reservation is
distributed asymmetrically — which centring, by definition, does not do. This is the same arithmetic
CSS's half-leading model performs: CSS 2.2 §10.8.1 splits leading `L = line-height − (A + D)` into
`L/2` above and `L/2` below, and css-inline-3 keeps that rule for the trimmed `cap alphabetic` box.

For bundled Inter the two formulas are the *same number*: its OS/2 table gives
`sTypoAscender 1984`, `sCapHeight 1490`, `sTypoDescender −494` over `unitsPerEm 2048`, so
`u = d = 494/2048 = 0.241211em` exactly and both formulas give `0.482422em` [verified: parsed
`fonts/InterVariable.ttf` directly]. The choice is therefore free for every render this service ships
and only bites a `LABELER_FONTS_DIR` font with lopsided metrics — where `u + d` would leave the clip
in place and call the bug fixed.

**Alternatives.** `u + d`, the issue's own wording: rejected above, and rejected only on the
asymmetric case, which is why the visible consequence the issue accepted is unchanged. Measuring the
string's ink bounds (Pango exposes exactly this split — logical extents for layout, ink extents for
what was drawn): rejected by ADR-0045, since it makes the same template render at different heights
for different data. Padding `center` asymmetrically so the *ink* band is centred and then reserving
`u + d`: tighter by `|u − d|`, but it moves text for any asymmetric font, adds a signed pad to a
placement path that has only ever grown boxes, and forces `pad_pt` to load a font instance for
`center` — which it deliberately short-circuits today so that a centred fixed-size render does not
acquire a new way to fail.

No layout engine surveyed guarantees ink containment: CSS says trimming to `cap alphabetic` still
leaves overflow as overflow, Flutter's `StrutStyle` documents that tall glyphs escape the strut, and
Pango warns that logical extents may sit inside the ink. The reservation is this service's own
guarantee, made possible because it also owns the clip; the bounded scope in the spec's second
requirement is what every one of those engines states instead.

### 2. One reservation, every place the fit is judged

`overflow_em` keeps its single-source-of-truth role: changing the `Center` arm changes the size search
and both line budgets at once. Splitting it — reserving in `block_height` but not in `max_lines` —
would let the two disagree about what fits, and would leave a fixed-`font_size` centred block, which
has no shrink path, clipping exactly as it does today.

The cost is named in the proposal: a fixed-size centred multiline item can drop a line to ellipsis.
That trade is not new; `top`/`bottom` have made it since ADR-0050, and ADR-0082's `overflow: fail`
already gives an author who would rather see an error than a shortened line an explicit way to say
so.

### 3. Refusing, rather than clipping, where the reservation is what fails

The reservation is the room the overflow policy exists to defend, so the two must not disagree. Once
`center` reserves ink, an item that fits its caps but not its ink is an item whose content does not
fit its box, and ADR-0082 already says what happens then: ellipsize, or raise `text_does_not_fit`
under `overflow: fail`. Carving `center` out — reserving for the size search but letting the policy
judge on the old model — would reintroduce exactly the split Decision 2 rejects, and would mean a
`fail` template silently printing clipped ink, which is the one outcome an `overflow` policy is
supposed to make impossible.

The visible cost is a template authored tight around the cap-height box: in Inter the one-line floor
for a centred item moves from `0.7275 × size` to `1.2099 × size`, and between those two heights a
render that used to succeed now returns 422. That is a break, it is in the proposal and the ADR, and
it is the loud failure the repo's no-silent-fallbacks rule asks for. None of the four catalog tapes
and none of the fixture templates sit in that band.

### 4. The reservation is part of the intrinsic height, and `block_height` must stop meaning two things

`block_height` is read twice: by `text_fits`, where the answer wanted is "does this block plus the
room its ink needs fit the box", and by `layout_text`, where it becomes `TextFit.height_units`
(`src/render/helpers.rs:794-806`) and so the intrinsic that resolves a `content` box
(`src/render/mod.rs:1401`). Both readings want the reserved height — an intrinsic that excluded the
reservation would hand a `content` box exactly enough room to clip — and `top`/`bottom` have worked
this way since ADR-0050. Extending it to `center` is consistent, and the spec states the consequence:
a centred `content`-height item grows and its ink rises above its anchor.

There is a third reader that wants the opposite. `block_height_matches_typst_layout`
(`src/render/mod.rs:5253`) compiles a real Typst frame and compares it against
`block_height_for_test`, which passes `Center`. That test is calibration: it asserts our model of
Typst's line stacking, which has no reservation in it, and it passes today only because the `Center`
arm returns zero. After this change it drifts ~19% at 20 pt and fails — correctly, because the
quantity it compares is no longer the one Typst lays out.

So the fix is not to exempt the test but to separate the two meanings: a metric block height, which
is what Typst lays out and what calibration compares, and the reserved demand, which is metric plus
`reserve × size` and is what fitting and the intrinsic use. One expression stays the definition of
the other, so they cannot drift.

### 5. What the containment claim is checked against

The fit predicate compares with a 0.01 pt tolerance (`text_fits`, `src/render/helpers.rs:565-582`),
and the line budget carries none (`:742-746`). The reservation does not remove the tolerance and
should not: tightening it would be an unrelated change to every alignment. So the guarantee is
stated as tolerance-bounded — ink at the very edge of the declared band may fall up to 0.01 pt
outside the box — rather than as exact containment the predicate does not deliver. At 180 dpi that
is 1/40 of a pixel: it can change one antialiased edge pixel's coverage, which is why the acceptance
scenario asserts a closed stroke rather than a blank raster row.

That makes the metric guarantee arithmetic, and arithmetic is not what the issue reported — a cut
`g` is. So the acceptance evidence is a raster: render the #245 repro, find the item box's rows, and
assert no ink on its final row where the pre-change render put 16 pixels of a sliced `g`. A test
asserting `overflow_em(Center)` equals a constant would pass against a renderer that still clips,
which is the failure mode this repo has shipped before. The eye pass the project requires of a
rendering change is on top of that, not instead of it, and it is not a checkable task box (#220).

### 6. ADR-0084, superseding one clause of ADR-0050

ADR-0050's "**`center` is left alone**" paragraph is what this change reverses, and its stated reason
— reserving both sides "would cost a full em on top of a 0.7275em line" — is where it went wrong:
the reservation is `0.4824em`, not `1.0em`, because the cap-height line already accounts for the rest.
Its second reason, protecting the bundled tape templates, holds and is now checked rather than
assumed: all four catalog tapes still fit at `font_size.max` with the reserve.

Per repo convention the ADR bodies are not edited. ADR-0084 states what it supersedes, gets its own
row in `docs/adr/README.md`, and the ADR-0050 row there is annotated the way ADR-0066's row records
its partial supersession by ADR-0071. Number 0084 because #226 took 0080-0082, the token grammar took 0079 and
`issue-263` holds 0083; the number is re-checked against `main` before the commit.

## Risks / Trade-offs

- **A centred template near its ceiling silently renders smaller.** → It is a behavior change, it is
  marked BREAKING in the proposal, and the ADR carries it. There is no compatibility flag: a second
  reservation policy would double every fitting path's test matrix to preserve a clip.
- **A fixed-size centred multiline item loses a whole line to protect a fraction of a descender.**
  → Real, and the sharpest edge of the change: the bundled `avery5163_asset_tag` case gives up its
  third line to recover 0.4 pt of clipped ink. Accepted for consistency with `top`/`bottom` (Decision
  2); an author who wants the line back raises the box or lowers the size, both of which are visible
  in the template.
- **A render that succeeded now returns 422.** → Only for a centred item whose box sits between
  `0.7275 × size` and `1.2099 × size`, or one declaring `overflow: fail` that overflows by its ink.
  Both are Decision 3, both are specified, and neither is reachable by anything this repo ships. An
  author's fix is the same one the error message already names: a taller box or a smaller floor.
- **A centred `content`-height item moves.** → Specified rather than hidden, and consistent with
  `top`/`bottom`. Nothing this repo ships asks for a content height on a text item, so the exposure
  is templates in the wild, which the ADR carries as a breaking consequence.
- **The raster assertion is itself a test that can be written so it cannot fail.** → It must be
  proved red first: run it against the unmodified `Center` arm and see it fail on the clipped row,
  then against the fix. A pass on both is a broken test, not a green one.
- **#226 landed mid-planning (808dc7f).** → The worktree is rebased onto it and every line reference
  here is re-read from the rebased tree. `overflow_em`, `block_height` and `pad_em` came through
  unchanged in shape; the two `max_lines` sites became one; the fit predicate gained two error paths,
  which is Decision 3. `issue-263` is in flight over the same layout walk but not over the fitter.
- **The reservation is derived from the font instance at the candidate size.** → Inter's `opsz` axis
  can move the metrics per candidate, which is why `block_height` reads them off the already-instanced
  face rather than caching a ratio. The new arm must do the same; it does, since it reads the same
  face.
- **Verification cannot be a metric assertion alone.** → A test asserting `overflow_em(Center)` equals
  a constant passes against a broken renderer. The fit test must be the ink-conservation kind
  ADR-0050's Consequences describe — and its warning applies here in reverse: a centred control is no
  longer a clip-free control, so the comparison must be raster-vs-box-edge, not subject-vs-control.

## Migration Plan

None. The service is stateless, templates are read at startup and on reload, and nothing persisted
changes shape. Rollback is a revert of the one arm.
