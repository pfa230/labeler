## Why

CSV parsing loses a supplied `__proto__` field, and grid reads can show an inherited `constructor` value for a cell that holds no value. These are valid parameter names under the existing contract and must survive import, editing and submission just like ordinary names.

Source issue: https://github.com/pfa230/labeler/issues/422

Fixes #422

## What Changes

- Use the existing own-key writer when assembling CSV row data.
- Use the existing own-key reader in both grid data-cell reads and column getters.
- Cover special-name CSV ingestion, grid display/editing and outgoing batch data with regressions in the existing UI test suites.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None. This change implements `parameters`: Parameters are declared as a sequence, and `ui`: CSV Import screen, Grid cells keep their values, What a screen submits. No contract delta is needed; `.openspec.yaml` sets `skip_specs: true`.

## Impact

Implementation is limited to `ui/src/lib/csv.ts`, `ui/src/components/LabelGrid.tsx` and their existing CSV, grid and Import tests. Reuse `getOwnKey` and `setOwnKey` from `ui/src/lib/labelInputs.ts`; no new helper, dependency, parameter restriction or API change is required.

## Non-Goals

No connector work, prototype-security framework, new object model, CSV syntax change, backend change or refactor of already-correct submission and computed-key update paths. The complete request in #422 is included; no follow-up issue is needed.
