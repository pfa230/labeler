## Why

The print form submits manually typed numbers outside declared bounds without identifying the field. A current-browser probe reproduced both Download and Print sending `0` and `11` for a number parameter bounded by `[1, 10]`, even though the input's native range flags recognized each violation.

Source issue: https://github.com/pfa230/labeler/issues/238

Fixes #238

## What Changes

- Show a textual, accessible error beside an out-of-range numeric parameter as it is edited.
- Disable Print and Download while any print-form numeric parameter violates either declared bound; accept equality and restore availability after correction.
- Preserve blank/default omission and keep the bounds gate independent of live-preview results.
- Cover integer/number controls, one-sided and two-sided bounds, correction and clearing through the existing print-form interaction tests.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `ui`: Specify print-form numeric-bound feedback and action availability, and reconcile the existing submission and preview wording with that narrow gate.

## Impact

Implementation uses the existing `ui/src/lib/labelInputs.ts`, `ui/src/components/ParamInput.tsx`, `ui/src/pages/print/PrintForm.tsx` and print-form tests. No API, dependency, server coercion or template-schema changes are required.

## Non-Goals

No Import grid validation, inferred layout limits, completeness checks, value clamping, numeric parsing changes, general validation framework or preview suppression. The full request in #238 is included; no follow-up issue is needed.
