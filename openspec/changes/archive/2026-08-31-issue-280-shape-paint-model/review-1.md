## Review Metadata

- **Round**: 1
- **Prior round**: none

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/shape-paint/spec.md, design.md (plus source files read: `src/models.rs`, `src/raw.rs`, `src/convert.rs`, `src/render/mod.rs`, `src/render/helpers.rs`, `src/templates.rs`, `src/openapi.rs`, `src/parse.rs`, `docs/SPEC.md`, `docs/AUTHORING.md`, `docs/DEPLOY.md`, `docs/adr/README.md`, `AGENTS.md`, `Cargo.toml`, `Cargo.lock`, `openspec/specs/layout-sizing/spec.md`, `openspec/specs/flow-layout/spec.md`, repo-wide `openspec/specs/` search, `tests/fixtures/templates/`, `tests/acceptance_issue_263.rs`, and the installed Typst 0.15.1 `color.rs`/`shapes.rs` sources)
- **Issue**: #280

SPECS_SHA256: <VALUE>

## Findings

### Critical (blocking)

1. **The core requirement contradicts itself about whether a line accepts `background`.** It defines both `container` and `line` as shapes, then says every shape SHALL accept both optional declarations and all four combinations, including fill-only and fill-plus-stroke. Four lines later it says `background` on a line SHALL be refused (`specs/shape-paint/spec.md:11-25`). The later restriction repeats that only containers accept backgrounds (`specs/shape-paint/spec.md:219-223`). This contract is impossible to implement. Define separate categories—e.g. every shape may be stroked, while only shapes with an interior may be filled—and scope the four-combination guarantee to containers.

2. **The delta incorrectly declares that no existing OpenSpec capability governs the behavior being changed.** `proposal.md:58-64` says there are no modified capabilities, but `layout-sizing` requires that a physical `frame` outline remain unrotated (`openspec/specs/layout-sizing/spec.md:578-623`), while `flow-layout` expressly allows packed containers to carry `frame` (`openspec/specs/flow-layout/spec.md:197-200`) and requires a zero-width container’s `frame` stroke to remain visible (`openspec/specs/flow-layout/spec.md:353-385`). Removing `frame` leaves these accepted requirements stale and contradictory. Add proper `MODIFIED` deltas for the affected existing requirements, replacing the spelling while preserving or deliberately changing their complete contracts as required by `AGENTS.md:19-27`.

### Moderate

1. **Non-finite numeric paint values have no contract or scenario.** Thickness and radius are only said to be greater than zero (`specs/shape-paint/spec.md:98-109`, `specs/shape-paint/spec.md:128-143`). YAML/Rust `f32` can carry NaN and infinity; the current analogous checks use only `<= 0` (`src/templates.rs:1866-1869`, `src/templates.rs:1966-1969`), while source generation blindly formats an `f32` as a Typst length (`src/render/helpers.rs:245-266`). The repository explicitly checks finiteness where required elsewhere (`src/convert.rs:135-150`). Require finite positive values and add load-time refusal scenarios for NaN and positive infinity.

2. **The externally visible colour normalization is absent from the spec, and the design’s example is factually wrong.** `TemplateDetail` exposes the serialized layout (`src/models.rs:72-87`, `src/models.rs:830-892`), and the design says an authored name is returned canonically as RGBA (`design.md:100-108`), but no requirement defines that API representation or round trip. Worse, it says `red` becomes `#ff0000ff`; Typst 0.15.1 defines `red` as `#ff4136` (`/home/pfa/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/typst-library-0.15.1/src/visualize/color.rs:123-127`, `:311-312`). Specify serialization in the contract and correct the mapping to `#ff4136ff` if Typst’s named constants really are authoritative.

3. **The blast-radius claim is false and the migration plan omits known breakage.** `design.md:25-26` says `frame:` occurs in one file and nowhere else. Besides the fixture, `tests/acceptance_issue_263.rs:553-568` embeds a `frame:` template, and `src/render/mod.rs:3799-3801`, `:3839-3841`, `:4015-4017`, `:4739-4741`, and `:7197-7199` construct `Frame` directly. Deleting `Frame` therefore breaks substantially more than the stated file, yet `design.md:160-166` does not include those migrations. Correct the impact analysis and migration coverage.

4. **The plan knowingly exceeds the issue scope it describes without traceable authorization.** It says the narrow solution is one fill key, then introduces breaking changes to lines, frames, and radius for hypothetical future shapes (`proposal.md:8-13`, `proposal.md:28-33`, `proposal.md:42-46`). It also says #280 requested a monochrome vocabulary and deliberately reverses that constraint (`proposal.md:75-80`). Since GitHub issues are the sole tracker (`AGENTS.md:34-38`), #280 must be updated to authorize these acceptance-criteria changes, or the unrelated future-proofing should be split or narrowed.

5. **Stroke geometry at the resolved box boundary is underspecified.** The design says paint does not affect layout (`design.md:47-48`), while the requirement says it is painted on the outer box (`specs/shape-paint/spec.md:180-189`). The current renderer places a Typst rectangle exactly at the resolved box edges (`src/render/mod.rs:2042-2046`, `src/render/mod.rs:2084-2089`), so a centered stroke extends beyond that box and can be clipped at a label edge or overlap neighboring content. State whether strokes are centered, inset, or outset and whether their out-of-box ink is clipped.

### Suggestions

- Correct the OpenAPI impact wording: `Frame` is not explicitly present in the registration list today (`src/openapi.rs:89-174`), despite `proposal.md:81-84` saying it will be dropped there.
- Checks that passed: Typst 0.15 is the pinned API family (`Cargo.toml:24-28`); its installed primary source supports rect fill/stroke/radius, clamps radii, and paints fill before stroke (`typst-layout-0.15.1/src/shapes.rs:560-595`, `:628-635`, `:765-779`). The current renderer emits the frame before children (`src/render/mod.rs:2077-2102`). ADR-0091 is the next free number (`docs/adr/README.md:97-99`), and the plan includes the required render-and-inspect activity (`design.md:154-158`; `AGENTS.md:299-306`).

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: REVISE

## Required Changes (APPROVE_WITH_CHANGES only)

CHANGES_APPLIED: n/a

## Rebuttals
