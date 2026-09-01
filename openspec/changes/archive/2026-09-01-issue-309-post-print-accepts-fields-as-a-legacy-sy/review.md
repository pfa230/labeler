# Plan review

AUTHOR: claude
REVIEWER: codex
VERDICT: APPROVE_WITH_CHANGES
ROUNDS: 1

## Findings

1. The OpenAPI contract does not expose the unknown-key prohibition. The request contract rejects every unlisted key (`specs/print-request-body/spec.md:11-18,28-32`), but its OpenAPI requirement and scenario assert only that `data` is required and `fields` absent (`specs/print-request-body/spec.md:44-45,120-124`). `design.md:89-93` plans the same incomplete assertion. An object schema can omit `fields` while still permitting it as an additional property, so that test could pass while the published contract contradicts the endpoint. Utoipa 5.5 already translates `deny_unknown_fields` into `additionalProperties: false`; the plan must pin that output.

2. Neither explicit-empty-map scenario is adequately verified. The service requirement says `{}` reaches the template (`specs/print-request-body/spec.md:20-22`), but its scenario uses an unknown template and accepts `404` (`specs/print-request-body/spec.md:66-72`). The handler returns at template lookup before constructing `LabelInput` (`src/api.rs:2551-2556`), so this proves only deserialization. Separately, the UI requires a no-input template to send `data: {}` (`specs/print-request-body/spec.md:151-156`), while the design only plans conversions of existing assertions (`design.md:105-110`), none of which covers an empty submitted map.

## Required changes

- Amend the OpenAPI requirement and scenario to require `PrintRequest.additionalProperties` to be `false`, and extend the planned schema test to assert it.
- Replace or supplement the server empty-map scenario with an HTTP case that proves `{}` enters template processing—either a successful no-parameter template or a known parameter-validation outcome from an empty map.
- Add an explicit UI test using a `single` template with no entered inputs, asserting the `/api/print` body owns `data: {}` and has no `fields` property.

The author applies these fully specified changes, and NO further review follows.

CHANGES_APPLIED: yes
SPECS_SHA256: de4f3d0359a4d228e3fe58f5195aa9c89f927c603a55dd8ca43b7165fb96fda1
