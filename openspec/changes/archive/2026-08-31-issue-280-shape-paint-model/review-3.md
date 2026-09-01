## Review Metadata

- **Round**: 3
- **Prior round**: round 2 returned REVISE (2 Critical, 3 Moderate, 3 Suggestions); artifacts revised

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/shape-paint/spec.md, specs/layout-sizing/spec.md, specs/flow-layout/spec.md, design.md (plus source files read: `src/models.rs`, `src/raw.rs`, `src/convert.rs`, `src/render/mod.rs`, `src/render/helpers.rs`, `src/templates.rs`, `src/openapi.rs`, `src/api.rs`, `src/driver.rs`, `docs/SPEC.md`, `docs/AUTHORING.md`, `docs/adr/README.md`, `AGENTS.md`, `Cargo.lock`, existing `openspec/specs/layout-sizing/spec.md` and `openspec/specs/flow-layout/spec.md`, repository-wide capability and fixture inventories, `tests/acceptance_issue_263.rs`, installed OpenSpec 1.9.0 validator/parser sources, and installed Typst 0.15.1 library/layout/render sources)
- **Issue**: #280

SPECS_SHA256: <VALUE>

## Findings

### Critical (blocking)

None.

### Moderate

1. **The artifacts still make a decision for separate issue #282 while declaring it independent and out of scope.** The proposal says text colour belongs to #282 and “stays independent of this change” (`proposal.md:42-44`), but later says this change rejects the monochrome position of both #280 and #282 and that ADR-0091 records that reversal (`proposal.md:97-108`). The design repeats that ADR-0091 “records that it reverses the scope both #280 and #282 were filed with” (`design.md:96-123`). Because #282 is explicitly separate, issue #280 may establish an arbitrary-colour shape vocabulary, but its proposal and ADR plan must not change #282’s acceptance criteria. Rephrase these claims so #282 remains free to make its own ink decision.

2. **Explicit YAML `null` has no refusal scenario or implementation decision, despite the one-spelling contract.** The contract says omitted `stroke` is the sole spelling of no outline and omitted `rounded` is the sole spelling of square corners (`specs/shape-paint/spec.md:127-141`, `:169-176`); it also types `background` as a colour and `stroke.color` as a colour (`specs/shape-paint/spec.md:18-22`, `:127-133`). Ordinary `Option<T>` deserialization would silently turn `stroke: null`, `background: null`, `rounded: null`, and `color: null` into absence/default, contradicting those rules. This repository already uses presence-preserving `Option<Option<T>>` plus `deserialize_present_typed` when null must differ from omission (`src/raw.rs:63-70`, `:243-257`). Require and test load-time refusal of explicit null for these paint values, and record the presence-preserving raw representation in the design.

3. **The positive-number contract exceeds the precision of the existing Typst length emitter, but the implementation impact omits that helper.** Every finite thickness greater than zero is accepted and must render at its declared thickness (`specs/shape-paint/spec.md:125-146`), and the radius contract likewise accepts every finite positive value (`specs/shape-paint/spec.md:169-176`). The existing `format_length` rounds every value to four decimal places (`src/render/helpers.rs:245-266`), so a valid value such as `0.00001` becomes `0mm`/`0in`: a stroke disappears and a radius becomes square. The design discusses only non-finite values (`design.md:125-132`), while the proposal’s code inventory does not include `render/helpers.rs` (`proposal.md:109-114`). Either define a positive lower bound that survives source formatting or plan a precision-preserving formatter, and add a boundary scenario.

### Suggestions

1. Specify whether an omitted `stroke.color` is materialized as canonical black in `GET /api/templates/{id}` or remains absent. The input default is normative (`specs/shape-paint/spec.md:127-138`), but the read-back scenarios cover only explicitly authored colours (`specs/shape-paint/spec.md:296-313`), leaving two observably different API representations permitted.

2. Correct the supersession sentence saying “this requirement, and the five below” (`specs/shape-paint/spec.md:32-35`); seven requirement headers follow it. Name the affected requirements or say “the requirements in this capability” so the frozen-section boundary is unambiguous.

3. Checks that passed: OpenSpec 1.9.0 strict validation succeeds; the three `MODIFIED` requirement names resolve exactly to existing requirements; retaining the stale flow-layout scenario title is justified because scenario-loss comparison is name-based (`requirement-blocks.js:260-287`, `validator.js:514-529`), while its normative body now says `stroke` (`specs/flow-layout/spec.md:234-241`). Typst 0.15.1 is pinned (`Cargo.lock:3993-3997`, `:4103-4107`, `:4139-4143`), supports the claimed paint forms, clamps radius as described, and paints a shape’s fill before its stroke (`typst-layout-0.15.1/src/shapes.rs:561-591`, `:626-635`; `typst-render-0.15.1/src/shape.rs:39-79`). The service emits the container rectangle before its children (`src/render/mod.rs:2077-2102`). The blast-radius inventory is accurate: one YAML fixture contains `frame:`, one acceptance test embeds it, and five render unit tests construct `Frame`. ADR-0091 is next after ADR-0090 (`docs/adr/README.md:97-99`), no existing capability already owns shape paint, and the design includes rendering and opening the resulting image (`design.md:234-238`).

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: REVISE

## Required Changes (APPROVE_WITH_CHANGES only)

CHANGES_APPLIED: n/a

## Rebuttals
