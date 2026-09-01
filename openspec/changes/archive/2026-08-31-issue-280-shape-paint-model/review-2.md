## Review Metadata

- **Round**: 2
- **Prior round**: round 1 returned REVISE (2 Critical, 5 Moderate); artifacts revised

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md; design.md; specs/shape-paint/spec.md; specs/flow-layout/spec.md; specs/layout-sizing/spec.md; AGENTS.md; docs/SPEC.md; openspec/specs/flow-layout/spec.md; openspec/specs/layout-sizing/spec.md; docs/adr/README.md; docs/adr/0033-capability-aware-rendering.md; src/models.rs; src/raw.rs; src/convert.rs; src/render/mod.rs; src/render/helpers.rs; src/templates.rs; src/openapi.rs; src/api.rs; src/driver.rs; tests/fixtures/templates/avery5163_asset_tag.yaml; tests/fixtures/templates/brother_24mm_lines_divider.yaml; tests/acceptance_issue_263.rs; Cargo.lock; local Typst 0.15.1 library/layout sources
- **Issue**: #280

SPECS_SHA256: <VALUE>

## Findings

### Critical (blocking)

1. **Round 1’s contradictory shape vocabulary remains in `proposal.md`.** The proposal still says every shape carries both `stroke` and `background` and that any shape may be filled (`proposal.md:12-19`). The contract instead defines `line` as a shape without an interior and requires `background` and `rounded` on it to be refused (`specs/shape-paint/spec.md:11-30`). The revised specification is coherent, but the planning artifacts are not. Rewrite the proposal so `stroke` is common to all shapes while `background` and `rounded` belong only to shapes with interiors.

2. **The new `flow-layout` MODIFIED delta still requires a spelling this change removes.** Its field list correctly replaces `frame` with `stroke`, `background`, and `rounded` (`specs/flow-layout/spec.md:49-52`), but the same complete replacement requirement still has a scenario whose containers “each carry `frame`” (`specs/flow-layout/spec.md:110-117`). `shape-paint` requires that spelling to fail and quarantine the template (`specs/shape-paint/spec.md:313-332`). Archiving these deltas would therefore produce mutually impossible canonical requirements. Convert that scenario to the new spelling and update its stale “draws its frame” terminology at `specs/flow-layout/spec.md:234`.

### Moderate

1. **The arbitrary-colour rationale materially overstates the delivered print path.** The proposal and design say the current implementation halftones to PWG `black_1` using `{color_mode, resolution, pixel_type, dither_policy}` (`proposal.md:94-102`; `design.md:100-105`). Actual selection returns PNG only for a bi-level single label and PDF for everything else (`src/driver.rs:17-27`); the PNG path applies a fixed luminance threshold (`src/render/mod.rs:717-720`), explicitly described as “no dithering” (`src/render/helpers.rs:15-26`). Revise the rationale to distinguish ADR-0033’s architectural ownership from what is presently implemented, and state the resulting printability exposure for coloured sheet/PDF output.

2. **The blast-radius correction is incomplete.** The design’s inventory correctly records one YAML fixture, one embedded acceptance-test template, and five direct `Frame` constructions (`design.md:25-30`), matching `tests/fixtures/templates/avery5163_asset_tag.yaml:48`, `tests/acceptance_issue_263.rs:565-568`, and `src/render/mod.rs:3799,3839,4015,4739,7197`. A later decision still reduces the in-repository migration cost to “one fixture” (`design.md:88-91`). Preserve the complete inventory consistently; the embedded template and direct model constructions also require migration.

3. **The risk section describes the rejected colour model, with the wrong cardinality.** The accepted decision defines a project-owned table of sixteen CSS values and explicitly avoids Typst’s constants (`design.md:134-150`; `specs/shape-paint/spec.md:76-91`). The risk section instead says the schema borrows the renderer’s list and contains eighteen names (`design.md:204-205`). Correct or remove that stale risk before it propagates into ADR-0091.

### Suggestions

1. Rephrase “one way to write a colour” (`design.md:42-44`). The input contract intentionally gives the same colour several spellings—named, short hex, long hex, and alpha-bearing equivalents (`specs/shape-paint/spec.md:68-95`)—then provides one canonical read-back spelling (`specs/shape-paint/spec.md:294-301`). “One scalar colour grammar and one canonical output” would match the actual decision.

2. Make the unknown-name scenario structurally explicit. `color` is a field inside `stroke`, not a top-level paint key (`specs/shape-paint/spec.md:18-22,123-130`), but the scenario currently says only `color: chartreuse` (`specs/shape-paint/spec.md:118-121`). Use a complete `stroke: { thickness: ..., color: chartreuse }` example so it exercises colour validation rather than unknown top-level-field rejection.

3. Ensure the OpenAPI plan represents `Color` as the promised canonical string rather than exposing its internal RGBA storage. The proposal says to register `Color` (`proposal.md:103-107`), while the API contract promises `"#rrggbbaa"` strings (`specs/shape-paint/spec.md:294-311`) and `LayoutItem` is API-exposed (`src/openapi.rs:117-118`).

4. Verification record, no change requested: Typst 0.15.1 supports rect fill/stroke/radius, line strokes, and 3/4/6/8-digit hex; its `red` is `#ff4136`; the current renderer emits the frame rect before container children (`src/render/mod.rs:2077-2102`); only `layout-sizing` and `flow-layout` canonically govern the removed `frame` spelling; and ADR-0091 is the next free number after `docs/adr/README.md:99`.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: REVISE

## Required Changes (APPROVE_WITH_CHANGES only)

CHANGES_APPLIED: n/a

## Rebuttals
