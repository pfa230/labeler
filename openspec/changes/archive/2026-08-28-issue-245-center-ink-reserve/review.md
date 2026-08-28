## Review Metadata

- **Round**: 3
- **Prior round**: round 2 verdict REVISE (author claude, reviewer codex): an impossible acceptance scenario, an unspecified intrinsic-height consequence, and a containment claim the predicate's tolerance cannot support

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/layout-sizing/spec.md, design.md, plus AGENTS.md/CLAUDE.md, docs/SPEC.md §3.1, docs/adr/0045, 0049, 0050, 0082 and README.md, src/render/helpers.rs, src/render/mod.rs, src/lib.rs, catalog/tape/brother/*.yaml, tests/fixtures/templates/*.yaml, and openspec/specs/layout-sizing/spec.md
- **Issue**: #245


## Findings

### Critical (blocking)

None.

### Moderate

1. **The principal exact-size scenario remains under-specified because it does not fix the width or the line count at each candidate.** The requirement says fitting re-breaks text independently at every candidate size (`specs/layout-sizing/spec.md:20-24`), matching `largest_fitting_font` (`src/render/helpers.rs:611-617`). But the acceptance scenario merely says the value “wraps to two lines” and supplies no width (`specs/layout-sizing/spec.md:248-253`). The vertical arithmetic is correct only if the value remains exactly two lines at the decisive candidates: with Inter, the old two-line coefficient is `2×1490/2048 + 0.65 = 2.105078`, giving 24.0 pt in 51.31 pt, and the new coefficient is `2.105078 + 988/2048 = 2.5875`, giving 19.5 pt. If the value occupies three lines at the larger candidates, 24.0 pt does not follow. Specify a box width, or explicitly require two lines at the old and new decisive candidates, so the exact THEN is mechanically implied.

2. **The line-budget boundary contract is internally inconsistent and its scenario still makes an absolute containment claim that the permitted tolerance cannot support.** The requirement says the kept count is “at most `floor(...)`, never fewer than one” (`specs/layout-sizing/spec.md:202-205`), but when that floor is zero no count satisfies both; the implementation actually uses `max(1, floor(...))` after its one-line check (`src/render/helpers.rs:733-746`). The same one-line check and `text_fits` permit demand up to 0.01 pt above the box (`src/render/helpers.rs:574-582`, `:733-735`), while the multiline scenario still says the kept ink “is inside the box” without the bounded qualification (`specs/layout-sizing/spec.md:265-271`). For a box less than one-line demand by 0.005 pt, the code may keep one line whose declared band protrudes by that amount. State the actual capped formula and carry the requirement’s 0.01 pt qualification into this scenario.

3. **The content-height scenario incorrectly says asymmetric-font ink is centred.** A centre-aligned block receives no pad (`src/render/helpers.rs:969-974`) and its metric body is centred by `#align` inside the clipped box (`src/render/mod.rs:1539-1555`). With reserve `2×max(u,d)`, the metric block is centred and the declared ink band is contained, but the ink band itself is centred only when `u = d`; the asymmetric-font scenario explicitly covers `u ≠ d` (`specs/layout-sizing/spec.md:282-287`). Thus `specs/layout-sizing/spec.md:273-278` contradicts the chosen formula when it says “its ink is centred in that box.” The stated upward displacement of `max(u,d)×s` is correct; describe the metric block as centred and the ink as contained with potentially unequal edge gaps.

4. **The impact prose miscounts the size changes and contradicts its own correct endpoints.** `proposal.md:35-48` says a two-line block drops “up to four 0.5 pt steps,” calls two fixtures one-step drops, and calls `brother_24mm_multiline` the largest drop. Its own 21.5→17.5 multiline numbers are eight steps. More significantly, `brother_24mm_printed_on.yaml:17-26` has an 8.0 mm box and max 24 pt: `8×72/25.4 = 22.677` pt, while one-line reserved demand is `1.209961s`, so 18.5 pt needs 22.384 pt and 19 pt needs 22.989 pt; it drops 24→18.5, eleven steps and more than the multiline fixture’s four points. `brother_24mm_lines_divider.yaml:26-35` similarly drops 20→17.5, five steps, not one. Correct the qualitative step counts and “largest” claim while retaining the arithmetically correct endpoints.

### Suggestions

- No change requested on the central reservation choice: because centre placement leaves equal slack above and below the metric block, containment requires `reserve/2 ≥ u` and `reserve/2 ≥ d`, hence `2×max(u,d)`. The issue’s `u+d` is insufficient for an asymmetric font unless placement also becomes asymmetric.
- No change requested on applying the reservation to both size search and the multiline cap. Omitting it from the cap would leave fixed-size centred text able to clip, contrary to the stated containment model.
- No change requested on the newly possible 422 outcomes. `Overflow::Fail` consumes `text_fits` (`src/render/helpers.rs:702-720`) and the one-line floor consumes `block_height` (`:733-739`); once centred ink is part of the metric model, exempting those paths would make the overflow policy disagree with fitting. The break is disclosed in `proposal.md:53-63` and `design.md:93-107`.
- The catalog arithmetic checks out: the tightest maximum is `brother_12mm.yaml:25-28`, where 18 pt needs 21.779 pt in a 7.9 mm/22.394 pt box. The stated affected-fixture endpoints also check out, as does Avery’s three-line demand of about 47.58 pt in its 46.8 pt box; only the impact prose’s step counts and ranking are wrong.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Make the exact-size acceptance scenario mechanically determinate by specifying its width or explicitly fixing the two-line count at every decisive old/new candidate.
2. Express the line cap as `max(1, floor(...))`, account for the preceding one-line failure check, and qualify the multiline containment scenario by the documented 0.01 pt tolerance.
3. Replace the content-height scenario’s asymmetric-font claim that the ink is centred with the correct metric-block-centred, declared-band-contained behavior.
4. Correct the proposal’s step counts and remove or repair the false claim that the multiline fixture has the largest drop.

CHANGES_APPLIED: yes

## Rebuttals

All four Required Changes were applied and re-checked by the reviewer in read-only mode, scoped to
those items. Three re-check passes were needed:

1. **Acceptance scenario determinate.** Applied: the scenario fixes the box at 120 mm wide by 18.1 mm
   tall and names the two decisive candidates. Two re-checks rejected the accompanying explanation
   before it was right — first for claiming two lines at every candidate from 32 pt (false: the value
   is three lines there), then for claiming the larger candidates break to three lines (false: it
   stays two lines through 26.5 pt). The scenario now says every larger candidate fails on height at
   whatever count it breaks to, with 24.5 pt worked. Re-check: satisfied, verifying 50.52 pt at 24.0,
   51.57 pt at 24.5, 50.46 pt at 19.5 and 51.75 pt at 20.0 against a 51.31 pt box.
2. **Line cap.** Applied: the requirement states `max(1, floor(...))`, names the one-line check that
   makes the floor safe, and the multiline scenario now requires that check to have passed and
   carries the 0.01 pt qualification. Re-check: satisfied.
3. **Content-height scenario.** Applied: the metric block is centred and the declared band contained,
   with the two edge gaps written out, equal only when `u = d`. Re-check: satisfied.
4. **Impact step counts.** Applied: the drops are stated as endpoints and ranked correctly — 5.5 pt
   (`printed_on`), 4.0 pt (`multiline`), 2.5 pt (`lines_divider`). Re-check: satisfied.

No finding was disputed.
SPECS_SHA256: 79a8c9b925be890f58ab459cf1b6f73f93bfcf3d621a403446cc27a1a580c5a0
