## Review Metadata

- **Round**: 1
- **Prior round**: none

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/text-ink-containment/spec.md, design.md, plus AGENTS.md/CLAUDE.md, docs/SPEC.md §3.1, docs/adr/0045-vertical-text-alignment.md, docs/adr/0049-weight-aware-text-measurement.md, docs/adr/0050-ink-reservation-at-slot-edges.md, docs/adr/0082-text-overflow-policy.md, docs/adr/README.md, src/render/helpers.rs, src/render/mod.rs, src/lib.rs, openspec/specs/layout-sizing/spec.md and the other existing capability specs, catalog/tape/brother/*.yaml, tests/fixtures/templates/*.yaml, fonts/InterVariable.ttf
- **Issue**: #245

SPECS_SHA256: <VALUE>

## Findings

### Critical (blocking)

1. **The delta ignores and contradicts an existing OpenSpec requirement.** The proposal says no existing capability covers vertical fitting (`proposal.md:67-69`) and therefore supplies only an ADDED requirement (`specs/text-ink-containment/spec.md:8-45`). In fact, `openspec/specs/layout-sizing/spec.md:615-697` already specifies the complete text-fitting and overflow pipeline: its shrink step includes the alignment-dependent ink reservation (`:632-636`), and its policy explicitly says centred text has no reservation and may clip below `1.21 × font_size` (`:686-691`). The proposed requirement requires the opposite and extends that changed metric into overflow decisions (`specs/text-ink-containment/spec.md:94-100`). Archiving this delta would leave two authoritative requirements prescribing incompatible outcomes. The revision must modify the existing `Text is laid out against the box it will get, and what does not fit is authored` requirement, carrying its complete post-change contract, while handling the named frozen §3.1 supersessions without duplicating a conflicting contract.

2. **The scoped implementation cannot satisfy the central exact-containment contract.** The spec defines fitting with an exact `≤ H` boundary (`specs/text-ink-containment/spec.md:22-25`) and requires the rendered descender to leave both boundary raster rows clear (`:47-53`). The current predicate deliberately accepts a block up to `0.01pt` taller than the box (`src/render/helpers.rs:574-582`). The proposal scopes implementation to changing the `Center` arm and related comments/tests (`proposal.md:73-81`), and the design describes rollback as reverting that one arm (`design.md:140-143`); neither addresses the positive height tolerance or introduces a raster safety margin. Even without that tolerance, reserving exactly to the declared metric boundary proves only that ink does not exceed the geometric boundary, not that the first and last raster rows are blank. The contract, algorithm, and intended raster assertion must be reconciled.

### Moderate

1. **The fixed-size contract contradicts itself.** The first requirement says that for a fixed `font_size` the reservation is “visible only in the line count” (`specs/text-ink-containment/spec.md:31-33`). The second requirement instead makes it visible through new `422 text_does_not_fit` outcomes for both the one-line floor and `overflow: fail` (`:94-100`, `:109-123`). That matches the actual control flow: `text_fits` gates `Overflow::Fail` and the one-line `block_height` check (`src/render/helpers.rs:702-739`). State the fixed-size effects consistently.

2. **The repository impact arithmetic omits another deterministically affected fixture.** `tests/fixtures/templates/brother_24mm_lines_divider.yaml:26-35` gives its first centred line a 7.5 mm-high box and `font_size.max: 20`. With the stated Inter metrics, the proposed one-line need is `20 × (1490 + 2×494) / 2048 = 24.199pt`, while the box is only `7.5 × 72 / 25.4 = 21.260pt`; it therefore drops to approximately 17.5pt. The proposal’s named catalog arithmetic is correct, as are its calculations for `brother_24mm_printed_on` and `avery5163_asset_tag`, but its “Measured against what this repo ships” impact account (`proposal.md:33-42`) misses this fixture and the existing render exercised at `src/render/mod.rs:5542-5563`.

3. **The plan has no explicit render-and-look verification step.** The design asks for a raster-based automated assertion (`design.md:135-138`), but nowhere commits to rendering representative PNGs and visually inspecting centred single-line, multiline, ellipsized, and boundary cases. This is a rendering behavior change, and the project review rules require that loop rather than treating metric or test success alone as visual proof.

### Suggestions

- Keep `2 × max(u, d)` if placement remains unchanged. A metric box centred by `#align` receives equal slack on both sides, so containment requires each half to cover its own overflow; `u + d` is sufficient only when `u = d` or placement is shifted asymmetrically. Inter’s supplied values make both formulas `988/2048 = 0.482421875em`, so the catalog calculations do not distinguish them.
- Applying the same reservation to the multiline line-count inverse is internally correct for the promised containment model. Omitting it would leave fixed-size multiline blocks clipping even though size-range blocks use the reservation.
- The new `422` outcomes are a coherent consequence of enforcing ADR-0082 against the widened metric model, and they are disclosed as breaking behavior. They nevertheless must be expressed by modifying the existing `layout-sizing` requirement rather than by leaving its explicit old centred-policy language in force.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: REVISE

## Required Changes (APPROVE_WITH_CHANGES only)

CHANGES_APPLIED: n/a

## Rebuttals