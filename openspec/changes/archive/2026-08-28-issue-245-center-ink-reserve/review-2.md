## Review Metadata

- **Round**: 2
- **Prior round**: round 1 verdict REVISE (author claude, reviewer codex): the delta contradicted the existing layout-sizing requirement instead of modifying it, and the containment contract could not be satisfied as written

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/layout-sizing/spec.md, design.md, plus AGENTS.md/CLAUDE.md, docs/SPEC.md §3.1, docs/adr/0045, 0049, 0050, 0082 and README.md, src/render/helpers.rs, src/render/mod.rs, src/lib.rs, catalog/tape/brother/*.yaml, tests/fixtures/templates/*.yaml, fonts/InterVariable.ttf tables, openspec/specs/layout-sizing/spec.md, and requirement inventories under openspec/specs/
- **Issue**: #245

SPECS_SHA256: <VALUE>

## Findings

### Critical (blocking)

1. **The principal acceptance scenario is arithmetically impossible.** The scenario specifies an 18.1 mm-high box and `font_size: { min: 10, max: 32 }`, then requires the selected size to become smaller after adding the reserve (`specs/layout-sizing/spec.md:234-242`). But 18.1 mm is 51.31 pt, while Inter at 32 pt needs only `32 × (1490 + 2×494) / 2048 = 38.72 pt`. The maximum therefore fits both before and after the change. This also contradicts the proposal’s correct observation that 32 pt fits even the smaller 16.1 mm catalog box (`proposal.md:34-36`). The scenario cannot reproduce #245, cannot go red against the old implementation, and cannot prove the required behavior.

2. **Changing `overflow_em(Center)` changes intrinsic geometry and can move text, contradicting the stated scope.** `block_height` adds `overflow_em × size` (`src/render/helpers.rs:1010-1014`) and is used not only by the fit predicate but to produce `TextFit.height_units` (`src/render/helpers.rs:794-805`). That value becomes the measured intrinsic height (`src/render/mod.rs:1207-1224`) and resolves a `content`-height item’s actual box (`src/render/mod.rs:1401-1407`). A centred `size: [content, content]` text therefore grows vertically by the reserve; because `at` fixes its bottom-left position, centring the metric block in the taller box also moves its baseline upward by half the reserve. This contradicts “this change moves no text” (`proposal.md:28-29`), “reservation governs fitting alone” (`specs/layout-sizing/spec.md:216-219`), and the unqualified byte-identical headroom guarantee (`specs/layout-sizing/spec.md:244-250`). The complete contract must decide and specify intrinsic-height behavior, placement consequences, and a content-height scenario. The same overlooked call also makes `block_height_for_test` include the new centre reserve (`src/render/helpers.rs:1017-1021`), so the existing raw-Typst block calibration at `src/render/mod.rs:5249-5263` will fail even though the impact section names only two measurement tests (`proposal.md:82-84`).

3. **The 0.01 pt tolerance still makes the absolute containment contract unsatisfiable.** `text_fits` accepts a block up to 0.01 pt taller than its box (`src/render/helpers.rs:565-582`), while the delta repeatedly guarantees that accepted font-band ink is inside the box (`specs/layout-sizing/spec.md:202-206,252-265`). A glyph reaching the declared ascender or descender can therefore remain outside the clip by nearly 0.01 pt. The assertion that 0.01 pt “cannot move a raster row” (`specs/layout-sizing/spec.md:205-206`) does not repair this: at 180 dpi it is 0.025 pixel, and a subpixel displacement can change antialiased coverage or cross a pixel boundary. The raster requirement of no ink on the final row (`specs/layout-sizing/spec.md:241-242`) is consequently not implied by the predicate. Either make containment explicitly tolerance-bounded and give the raster test deliberate slack, or change the predicate; an exact inside-the-box guarantee and the current tolerance cannot both stand.

### Moderate

1. **The specified line-budget tolerance is absent from the scoped implementation.** The delta says both the fit comparison and line-count comparison carry 0.01 pt tolerance (`specs/layout-sizing/spec.md:202-206`), but `max_lines` uses the exact height with no epsilon (`src/render/helpers.rs:742-746`). The design says changing the single `Center` arm updates every judgement (`design.md:81-86`) and proposes no line-budget edit. Near a boundary, a width overflow can therefore enter the ellipsis path and drop a line that `text_fits` would accept vertically. Decide whether the line budget receives the tolerance; if it does, account for the resulting top/bottom behavior change too.

2. **The fixture impact inventory still omits the repository’s explicit multiline case.** `brother_24mm_multiline.yaml` has a centred multiline item in a 16.1 mm box (`tests/fixtures/templates/brother_24mm_multiline.yaml:21-31`), and the HTTP suite deliberately renders text described as wrapping onto two lines (`src/lib.rs:1320-1342`). For two Inter lines, the old vertical coefficient is `2×1490/2048 + 0.65 = 2.10508`, while the new coefficient is `2.58750`; in a 45.64 pt box their 0.5 pt ceilings are approximately 21.5 pt and 17.5 pt. That is materially larger than the proposal’s general “one or two 0.5 pt steps” claim and is a third affected Brother fixture beyond those listed at `proposal.md:30-44`.

3. **The ADR plan names only the old row’s annotation, not the required new ADR-0084 index row.** The project requires every behavior-changing ADR to add its own `docs/adr/README.md` row (`AGENTS.md:29-30`). The proposal and design promise ADR-0084 and an annotation of ADR-0050’s existing row (`proposal.md:56-57`, `design.md:123-134`), but never state that the new ADR-0084 row will be added. The current index ends at ADR-0082 (`docs/adr/README.md:88-90`).

### Suggestions

- Retain `2 × max(u, d)` if centre placement remains symmetric and unpadded. Equal slack is placed above and below the metric block, so containment requires each half to cover its respective overflow; `u + d` works only when `u = d` or placement shifts asymmetrically. Inter is symmetric at `494/2048` on both sides, so the two formulas coincide for the bundled font.
- The MODIFIED requirement’s header exactly matches `openspec/specs/layout-sizing/spec.md:615`, and the block carries the existing requirement’s complete content plus the intended edits and new scenario.
- The catalog arithmetic is correct: the four tapes still fit at maximum size. The listed `brother_24mm_printed_on`, `brother_24mm_lines_divider`, and horizontal Avery calculations also check out; the blocking arithmetic error is the separate 18.1 mm acceptance scenario.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: REVISE

## Required Changes (APPROVE_WITH_CHANGES only)

CHANGES_APPLIED: n/a

## Rebuttals