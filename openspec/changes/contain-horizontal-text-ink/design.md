## Context

See `proposal.md` for scope. The current `text_width` sums character advances, while `line_inks` already shapes the emitted paragraph for vertical extents. `TextFit` carries that vertical reservation to a clipped Typst text box. Horizontal ink and shaped advance are not carried through the fit.

### Reproduction completed during scoping

The application source is the published `5680c296` tree, unchanged by the three planning commits in this session. Built its release binary/library offline and linked an artifact-only probe against that library and its exact Cargo dependency artifacts. At 300 DPI, 24 pt Inter, weight 400:

| Current input and context | Measured cause | Raster evidence |
|---|---|---|
| Forty `W` characters in a dynamic-width single; emitted `WWWWWWWWWWW...` | Character advances total 272.847656 pt; shaped advances total 275.109375 pt; outline reaches about 273.621094 pt | 29 changed pixels lie wholly beyond the text box in the unclipped control. Disabling optical overhang still leaves those pixels lost. |
| `j` in a capped content-width container | Outline starts about 0.492188 pt left of its advance origin | 18 pixels are cut on the left. Disabling optical overhang does not fix it. |
| `...` centered in its content-width box | Optical punctuation protrusion despite the ordinary 18 pt advance | 28 pixels are cut on the right. Disabling optical overhang removes this clipped/unclipped difference. |

The probe renders each actual label, captures its emitted Typst through the existing debug trace, adds empty page space to the right without moving the text, and compares the same source with clipping on and off. It counts differences in columns wholly outside the item's horizontal bounds while excluding top/bottom edge rows, so vertical antialiasing differences do not masquerade as horizontal clipping. A separate Rust font/shaping probe supplies the advance and outline measurements; outline numbers above use the font's bounding-box query and are diagnostic approximations, not the proposed fractional-outline measurement.

A 66-case survey covers the three requested contexts, narrow/wide endings and weights 400/900. Overflowing centered narrow- and wide-character runs reproduce the ellipsis loss too. This establishes the three causes; it does not establish a completed fix. The clipped/unclipped ellipsis images were opened and the truncated final dot is visible. Scripts, YAML, emitted sources, PNGs, measurements and survey results are local run artifacts under `.agent-runs/issue-257/`, not repository fixtures.

Typst documents [clipping](https://typst.app/docs/reference/layout/box/#parameters-clip) and its default-enabled [optical overhang](https://typst.app/docs/reference/text/text/#parameters-overhang). The latter is an intentional typesetting feature; the bounded label renderer should turn it off rather than measure a second optical-protrusion model.

## Goals / Non-Goals

**Goals:** One horizontal width/ink model for measurement, fitting and emission, keeping emitted text inside the existing clip and preserving the current vertical model.

**Non-Goals:** See the proposal. Keep the current layout resolver, script/direction segmentation, supported-font boundary, line-breaking policy, overflow modes and error vocabulary. No new authored option or margin is introduced.

## Decisions

### 1. Extend the existing shaped measurements to both axes

Routine default: retain the existing paragraph/run shaping and safe line-slice reshaping, adding horizontal advances and x bounds to its results. Accumulate shaped x advances and offsets in the line's rendered order. Include advances for lines that have no outlined glyphs, such as spaces. Extend the existing fractional outline-extrema calculation to x as well as y; the integer-rounded bounding-box query is adequate for identifying the cause above but not for the 0.01 pt fitting tolerance.

Keep one measurement result for the candidate/emitted lines. Reuse it for both axes instead of shaping separately for horizontal and vertical ink. Preserve the existing metric-height rejection before expensive shaping where height alone already refuses the candidate. Width cannot be rejected by an unshaped character sum: the reproduced W run establishes that it is not the rendered advance.

### 2. Use the measured horizontal reservation throughout fitting

Use the `text` delta's block formulas: the widest shaped advance defines the horizontal metric block; line positions within it follow left/center/right alignment; outer ink produces `l` and `r`. Left/right reserve their sum, while center reserves twice the larger side so each line's advance box can remain centered.

Use this same comparison for candidate sizing, wrapping and ellipsis width checks, retained-line selection, failure decisions and final intrinsic width. Measure the lines actually emitted, including the marker; measurements from the unshortened input cannot establish its final bounds. Keep the current vertical reservations and tolerance.

This intentionally lets asymmetric ink make center-aligned content wider than left/right-aligned content. Amend `text`: Layout box as well as Measured block and ink reservation so the contract does not simultaneously require alignment-independent intrinsic width. When there is no horizontal overhang, alignment still leaves intrinsic width unchanged. Existing `layout` rules consume the resulting intrinsic size without modification.

### 3. Emit matching insets and ordinary punctuation positioning

Carry the required horizontal reservation to the existing text emitter. Inset left-aligned text by `l`, right-aligned text by `r`, and keep centered advance boxes centered without a one-sided inset, as the delta specifies. Combine this with the existing vertical padding, using the existing box/align/pad primitives.

Set `overhang: false` for emitted text. Keep text, container and page clips. The one-flag experiment fixes centered punctuation but leaves W advance drift and j overhang; it is necessary but insufficient. Shaped advances alone likewise leave j outside its box. A fixed or DPI-based pad does not explain these font-dependent measurements, and removing clipping only allows ink to escape a capped container or label.

### 4. Use the existing renderer tests and a small regression set

Extend the current independent Typst advance comparison with the positive-kerning W case. Verify horizontal overhang and center-versus-edge reservation with j while preserving the existing vertical cases. Expected advances/bounds must come from actual Typst output or independent font geometry, not the helper being tested.

Extend the existing clipped/unclipped raster containment helper to compare horizontal columns as well as its current vertical rows, preserving raster phase and keeping the label edge out of the control comparison. Add the three failing examples above, including an overflowing centered narrow-character run, and narrow/wide ending-glyph controls. Assert the emitted marker and no loss of final ink; for the selected dynamic-label cases inspect/assert that the final ink stops before the last raster column. Verify that restoring each corresponding old behavior makes its regression fail. Do not turn the 66-case exploratory survey into a new matrix or harness.

Run the existing wrapping, shrink, fail/ellipsis, empty-value, script/mark and vertical ink tests against the shared model. Update only text-width expectations demonstrably changed by shaped advances or measured ink. Render representative before/after PNGs and inspect the result as required by the issue; that attended visual inspection is not a checkbox task.

## Risks / Trade-offs

- Shaped widths can change label lengths, shrink decisions and truncation even without visible clipping. This is an accepted pre-1.0 behavior replacement; derive changes from emitted geometry and update the authoring explanation.
- Horizontal shape placement and whitespace advances must agree with the emitted paragraph. Reuse the current run segmentation and independent Typst checks rather than add another shaping backend.
- A shaped-width fit costs more than a character sum. Reuse the existing shaping pass and metric-height rejection; no speculative cache or performance subsystem is added.
- A nonempty last raster column alone is not proof of clipped ink. Use an unclipped same-phase control to identify lost ink and keep the selected edge-clearance assertions tied to the reproduced cases; no universal extra-pixel margin is promised.

## Validation

Scoping completed the release build and reproduction above; no application source has been edited and the correction is unimplemented. Implementation runs Rust formatting, clippy and the full Rust tests plus strict OpenSpec validation. Run artifacts stay in `.agent-runs/`. No UI, template-schema, printer or physical-print gate is added.

## Cut pass

| Kept item | Requirement or explicit request | Simpler version and concrete failure |
|---|---|---|
| Shaped advance and fractional x bounds (task 1.1) | `text`: Measured block and ink reservation; #257's measured cause | The old advance sum is 2.261719 pt short for the reproduced W line. Shaped advance alone misses j's negative bearing. Extend existing results and outline math. |
| Shared horizontal fitting/intrinsic width and alignment reservation (task 1.2) | `text`: Layout box, Measured block and ink reservation, Auto-shrink, Overflow policy | Changing only final box width leaves earlier size/overflow decisions inconsistent. Symmetric center reserve retains centered advance boxes with asymmetric ink. |
| Horizontal insets and optical-overhang disable (task 2.1) | `text`: Measured block and ink reservation | Reservation without matching emission still clips; disabling optical overhang alone leaves both other reproduced failures. Use existing Typst primitives and keep clips. |
| Three raster regressions and ending-glyph controls (task 2.2) | #257's three contexts, current pixel reproduction and final-ink acceptance | Helper-only assertions cannot establish emitted ink containment. Extend the existing control comparison instead of retaining the exploratory survey as a test framework. |
| Authoring explanation (task 2.3) | #257's current bounds rule; repository rule that the guide follows the spec | An unchanged guide omits the reason content width can grow with shaped advance/overhang. Revise only its text-fitting explanation. |
| Rust gates (task 3.1) | Repository gates for the planned Rust diff | The new fixtures alone do not exercise existing wrapping, truncation and vertical guarantees; use current suites. |
| Strict delta validation (task 3.2) | Repository gate for the `text` delta | Use the pinned validator and complete modified blocks; no new content checker. |

Cut: constant padding, DPI margins, clipping removal, a separate horizontal shaper, another font/measurement abstraction, new caches, response fields, template settings, the 66-case survey as permanent tests, layout-schema changes and unrelated rendering cleanup. Each retained measurement/emission step addresses a reproduced failure; existing vertical machinery and test oracles are reused.
