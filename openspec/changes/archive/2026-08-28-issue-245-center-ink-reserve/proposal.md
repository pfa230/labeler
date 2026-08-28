## Why

Issue [#245](https://github.com/pfa230/labeler/issues/245). `overflow_em`
(`src/render/helpers.rs:993`) returns `0.0` for `VerticalAlign::Center`, so the auto-shrink fitter
accepts the largest font size whose **cap-height boxes** fit the text item's box. Every layout item is
emitted into a `clip: true` box (`render/mod.rs:1555`), and a centred block fitted to a tight box has
no slack left, so its descenders and accented capitals are cut. `Top` and `Bottom` already reserve
both overflows and do not have this defect (ADR-0050).

Auto-shrink is exactly the case ADR-0050's "`center` splits the slack already" assumption fails on:
the fitter's job is to consume the slack. The only workaround an author has is a smaller text box
inside the frame, and that is a magic number — ink depth scales with the size the fitter picks, so a
fixed inset is too much for small text and too little for large.

## What Changes

- **`center` reserves ink room in the fitter.** `overflow_em` returns
  `2 × max(ascender − cap_height, |descender|)` for `Center`, in em, instead of `0.0`. Twice the
  *larger* overflow, not the sum: a centred block is centred on its cap-height metric box, so the
  slack `(H − block) / 2` on each side must absorb the overflow on *that* side. The sum is correct
  only for a symmetric font. Bundled Inter is exactly symmetric (both overflows are 494/2048 =
  0.241211em), so for every render this service ships today the two formulas are the same number,
  0.482422em; they diverge only for a font supplied through `LABELER_FONTS_DIR`.
- **The reserve applies everywhere the reservation is consulted**: the fit predicate
  (`block_height`, `helpers.rs:1014`, which decides the size and answers `text_fits`) and the
  multiline line-count cap (`max_lines`, `helpers.rs:742`, which decides how many lines survive). One
  rule, one meaning of "fits", matching how `top`/`bottom` already behave.
- **Placement inside a box does not change**, but a `content` height does. `pad_em` stays `0.0` for
  `center`, so nothing is inset. The reservation is nevertheless part of the intrinsic height
  (`block_height` produces `TextFit.height_units`, `helpers.rs:794-806`, which resolves a `content`
  box at `render/mod.rs:1401`), so a `center`-aligned item asking for a `content` height gets a box
  taller by the reserve and its ink sits `max(u, d) × size` higher above its anchor. That is what
  `top`/`bottom` already do, and no catalog or fixture template asks for a content height on a text
  item, so nothing this repo ships moves.
- **BREAKING (visible output).** A centred, height-bound text item near its ceiling renders smaller.
  How much smaller is set by how large the reserve is against the block it is added to, so the drop
  is largest on a one-line block in a tight box: 5.5 pt on the worst bundled case below, 4.0 pt on
  the worst multiline one. A centred multiline item near its ceiling keeps one fewer line and
  ellipsizes — including at a **fixed** `font_size`, where there is no shrink path to absorb the
  reserve. Measured against what this repo ships:
  - The four catalog tapes (`catalog/tape/brother/brother_{9,12,18,24}mm.yaml`) are **unchanged**:
    each one's `font_size.max` still fits with the reserve (24 mm: 38.72 pt of 45.64 pt; the tightest,
    12 mm, is 21.78 pt of 22.39 pt), so height never binds and only width drives their size.
  - Two fixtures height-bind on a single line: `brother_24mm_printed_on.yaml`'s first line (8.0 mm
    box = 22.68 pt, max 24 pt) goes 24 pt to 18.5 pt, the largest drop anywhere in the repo at
    5.5 pt; and `brother_24mm_lines_divider.yaml`'s first line (7.5 mm box = 21.26 pt from
    `at: [0, 8.6] to: [-0, 16.1]`, max 20 pt) goes 20 pt to 17.5 pt, the smallest at 2.5 pt, which
    the render test at `src/render/mod.rs:5545` exercises. No fixture and no catalog template lands
    in the new 422 band.
  - `tests/fixtures/templates/brother_24mm_multiline.yaml` drops 21.5 pt to 17.5 pt on a centred
    two-line block in a 16.1 mm box: 4.0 pt, between the two above. A second line dilutes the reserve
    — it is 23% of a two-line metric block against 66% of a one-line one — but this box is tighter
    relative to its ceiling than `lines_divider`'s. The HTTP test at `src/lib.rs:1321` renders it.
  - `tests/fixtures/templates/avery5163_asset_tag.yaml` does change: `{id}` (0.35 in box, max 22 pt)
    drops to ~20.5 pt, `{name}` drops half a step, and the fixed-12 pt `{tags}`/`{description}`
    (0.65 in box) hold two lines instead of three once the text wraps that far — three lines of ink
    is 47.6 pt in a 46.8 pt box, which is why the third line's descenders clip today.
- **The overflow policy ADR-0082 added turns two of these cases into errors, not smaller text.**
  Since #226 the fit predicate also gates `overflow: fail` and a hard floor check, both of which now
  measure against the larger centred reservation:
  - a `center`-aligned item with `overflow: fail` whose ink (not its caps) exceeds the box returns
    **422 `text_does_not_fit`** where the same request rendered before;
  - a `center`-aligned item whose box cannot hold one line **plus the reservation** at its chosen size
    returns 422 with "box height … is shorter than one line" (`helpers.rs:733`), where before it
    rendered and clipped. The threshold moves from `0.7275 × size` to `1.2099 × size` in Inter.

  Both are the loud failure the policy asks for rather than silent clipping, and both are visible
  breaks for a template authored tight around the cap-height box.
- **ADR-0084** records the decision and supersedes ADR-0050's "`center` is left alone" clause; the
  ADR-0050 row in `docs/adr/README.md` is annotated, and neither ADR body is edited.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `layout-sizing`: two deltas in one file.
  - **MODIFIED** `Text is laid out against the box it will get, and what does not fit is authored`.
    That requirement already owns the fitting pipeline and the `overflow` policy, and it currently
    states the opposite of this change: its shrink step reserves ink only "at the aligned edge", and
    its policy paragraph declares that centred text "can still clip in a slot shorter than
    `1.21 × font_size`". Both are rewritten, and a scenario is added for a centred item that
    overflows only by its ink. The complete post-change requirement is carried, scenarios included.
  - **ADDED** `Vertical fitting reserves the ink each alignment can expose`. The reservation itself —
    `u`, `d`, the block height, the per-alignment reserve, the placement pads and the bounded
    guarantee — lives only in the frozen `docs/SPEC.md` §3.1 today, so first-touch requires an ADDED
    requirement holding the complete contract for all three alignments and naming the two §3.1
    bullets and the line-count formula term it supersedes.

## Impact

- `src/render/helpers.rs`: the `Center` arm of `overflow_em`, its doc comment, and the two
  measurement tests that assert `overflow_em(Center) == 0.0` and that a bottom-aligned fit lands
  smaller than a centred one in the same slot.
- `src/render/mod.rs:5253`'s `block_height_matches_typst_layout`, which compares
  `block_height_for_test` against a compiled Typst frame. It passes `Center` today precisely because
  that arm reserves nothing, so it needs a metric-only block height to keep comparing like with like.
- `docs/adr/README.md` gains the ADR-0084 row and an annotation on ADR-0050's.
- `src/render/mod.rs:273`'s comment, which restates the old rule.
- Tests whose expectations encode the old centred ceiling (avery5163 renders).
- Verification is a rendered raster, not a metric assertion: a PNG of the #245 repro whose descender
  is closed, plus the eye pass the project requires of any rendering change.
- No schema, API, OpenAPI, or UI change. No new template field: this is the metric model the fitter
  already applies to `top`/`bottom`, extended to the third alignment.
- `src/render/helpers.rs:716`'s `Overflow::Fail` arm and `:733`'s one-line floor check, which do not
  change but start refusing more, being downstream of the reservation.
- **Sequencing.** #226 landed on `main` as 808dc7f while this was being planned, and this worktree is
  rebased onto it. Its ADR-0082 left ADR-0050's ink model untouched deliberately, so this change is
  still needed; `overflow_em`, `block_height` and `pad_em` came through it unchanged in shape, and the
  two duplicated `max_lines` sites became one. ADR numbers 0080-0082 went to #226, 0079 to the token
  grammar and 0083 is claimed by `issue-263`, so this change takes **0084**, re-checked against `main`
  at commit time.
