# Plan review

AUTHOR: claude
REVIEWER: codex
VERDICT: APPROVE_WITH_CHANGES
ROUNDS: 1

1. **MAJOR — The plan omits the breaking HTTP write behavior.** The artifacts describe only load-time quarantine and claim “No API surface … changes” (`proposal.md:17-25`, `proposal.md:68-71`; `design.md:33-37`, `design.md:86-92`). Both delta specs likewise specify only loading files (`specs/template-groups/spec.md:113-130`; `specs/conditional-visibility/spec.md:15-30`). However, `PUT /api/templates/{id}` passes its raw YAML body through the same parser before writing (`src/api.rs:640-646`, `src/api.rs:743-776`). Removing either legacy field therefore changes accepted HTTP request bodies: such a PUT returns `422 TemplateInvalid` with `details.reason: template_parse_failed` and writes nothing (`src/errors.rs:270-277`, `src/reason.rs:31-35`; `openspec/specs/template-registry/spec.md:270-303`). The planned registry-only tests would not pin that externally observable behavior.

## Required changes

- Revise `proposal.md` and `design.md` to acknowledge that both deleted spellings are also rejected when submitted to `PUT /api/templates/{id}`. Qualify the “No API surface changes” statement to mean no route or OpenAPI schema changes, while recording the changed accepted YAML contract.
- In each affected delta requirement, state that a PUT body containing its deleted spelling is rejected before any write with status `422`, `error.code` `TemplateInvalid`, `error.details.reason` `template_parse_failed`, and an error naming the rejected key; the existing file must remain unchanged on replacement and no file may be created on creation.
- Expand the test plan to exercise both legacy spellings through the HTTP PUT endpoint, asserting the complete error envelope and unchanged filesystem, in addition to the registry-quarantine coverage.

The author applies these changes, and no further plan review follows.

CHANGES_APPLIED: yes
SPECS_SHA256: 89e47a88d1f0e816d603120dec7f19af98e923955b415598eab752b6a0ebd6f5
