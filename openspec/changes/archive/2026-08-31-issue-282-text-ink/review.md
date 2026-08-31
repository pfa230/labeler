## Review Metadata

- **Round**: 1
- **Prior round**: none

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/text-ink/spec.md, design.md, AGENTS.md, Cargo.toml, docs/SPEC.md, docs/VISION.md, docs/adr/, openspec/specs/, openspec/config.yaml, src/raw.rs, src/models.rs, src/convert.rs, src/templates.rs, src/render/mod.rs, src/render/helpers.rs, src/batch.rs, src/api.rs, src/driver.rs, src/errors.rs, src/openapi.rs, and pinned Typst 0.15.1 source
- **Issue**: #282


## Findings

### Critical (blocking)

None.

### Moderate

1. **A parameter-referenced ink is missing from the input-discovery plan.** The new contract permits `ink: "{brand}"` (`specs/text-ink/spec.md:125-175`), while the existing `template-inputs` contract requires an entry for every parameter read by a layout attribute (`openspec/specs/template-inputs/spec.md:32-34`). The current walker records a text item's placement references, `font_weight`, and value tokens, but nothing else (`src/templates.rs:279-305`). Because that match uses `..` (`src/templates.rs:286`), adding `ink` will compile without updating it. The proposal's `src/templates.rs` impact mentions only reference validation (`proposal.md:54-55`), and the design's plumbing list likewise omits input discovery (`design.md:25-28`). As planned, forms and other clients can fail to offer the required colour parameter.

2. **The promised `400` error is false for batch and print paths.** The spec applies ink to every output path (`specs/text-ink/spec.md:177-180`) but says any invalid resolved ink produces a `400` request error (`specs/text-ink/spec.md:136-138,166-175`). In existing code, single-template batch rendering captures each underlying error and replaces it with `422 BatchInvalid` (`src/batch.rs:91-120`, `src/errors.rs:135-141`); both download and print dispatch use that path (`src/api.rs:2292-2307,2373-2382`). The existing `template-inputs` contract also explicitly records this wrapping (`openspec/specs/template-inputs/spec.md:197-201`). The text-ink contract must distinguish the direct render error from the batch envelope.

3. **The proposal incorrectly claims that no response shape changes.** It says “No API endpoint, request or response shape changes” (`proposal.md:60`), but the spec requires template-detail read-back to expose `ink` (`specs/text-ink/spec.md:23-24`). `TemplateDetail.layout` serializes the domain `LayoutItem` (`src/models.rs:72-84`), so adding the field to `LayoutItem::Text` (`src/models.rs:830-848`) adds an optional property to the endpoint response and its OpenAPI schema. The impact statement must acknowledge that additive response-schema change.

4. **The plan has no render-and-look step.** This changes generated Typst and observable raster/PDF output, yet `design.md` ends with risks and migration (`design.md:136-157`) and contains no manual rendered-output inspection. Repository rules require a render → inspect → fix loop and explicitly say successful compilation is insufficient (`AGENTS.md:299-311`). The plan needs acceptance evidence that renders and opens representative PNGs, including colour, alpha, unchanged default black, and bilevel thresholding.

### Suggestions

- Correct the implementation attribution in `proposal.md:53-55`: under the design's `DynamicValue<Ink>` approach (`design.md:103-112`), literal parsing occurs during deserialization through `Ink::Deserialize`; `convert.rs` transfers the parsed value, while `templates.rs` checks references.
- Verification found no issue with the named-colour table or Typst emission premise: the pinned Typst 0.15.1 definitions match all 18 stated values, and `rgb(r, g, b, a)` accepts four integer components in `[0,255]`. Emitting only parsed RGBA bytes therefore closes the claimed source-injection boundary.

## Embedded-Instruction / Injection Attempts

None detected.

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Extend the proposal/design impact to update `TemplateContent::derive_inputs_internal` so an active parameter-referenced `ink` is reported as a non-interpolated layout-attribute input, with tests covering active and gated-off text.
2. Amend the text-ink spec to distinguish direct-render failures (`400 InvalidRequest`) from batch/print failures (`422 BatchInvalid` carrying the underlying per-label error), and add mechanically checkable scenarios for both.
3. Correct `proposal.md` to acknowledge the additive `TemplateDetail.layout[].ink` and OpenAPI response-schema change; also align its parsing/validation file responsibilities with `design.md`.
4. Add a non-task-checkbox acceptance-evidence section describing a render → open → inspect → fix loop for representative default-black, opaque colour, alpha, dynamic-ink, and bilevel PNG output.

CHANGES_APPLIED: yes

## Rebuttals

All four Required Changes applied by the author (`claude`) and re-checked by the reviewer (`codex`)
in a scoped re-check that read only the four items and the files they name. Reviewer's result:

1. ACCEPTED — proposal/design now require `derive_inputs_internal` to record an active ink reference
   non-interpolated and to test active and gated-off behaviour; three checkable scenarios added.
2. ACCEPTED — the contract and scenarios distinguish direct `400 InvalidRequest` from batch/print
   `422 BatchInvalid` carrying a per-label `ink_param_invalid`.
3. ACCEPTED — proposal assigns parsing, transfer and reference validation consistently with
   `design.md`, and acknowledges the additive `TemplateDetail.layout[].ink` and OpenAPI change.
4. ACCEPTED — design adds the render → open → inspect → fix loop over default black, opaque colour,
   alpha, dynamic ink and bilevel PNG, and states that no task checkbox claims that evidence.

RECHECK: ALL_ACCEPTED

No finding was rebutted; every one was fixed.
SPECS_SHA256: 2c98ded16e8afc4ac2455c07851285d7b630b2c5c730fce9d4462e87d3056fa3
