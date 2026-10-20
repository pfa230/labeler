# Proposal

## Why

Import always gives an empty Status column the same width share as data, before there are any results to show. Its generic name also obscures whether a result came from printing or download validation.

Source issue: https://github.com/pfa230/labeler/issues/387

Fixes #387

## What Changes

- Show the result column only while row results exist and name the operation that produced them.
- Keep full per-row failure details readable from the result cell.
- Retain one success message for downloads, with no per-row download-success marks.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `ui`: batch-result visibility and operation-specific presentation.

## Impact

Touches `Import.tsx`, `LabelGrid.tsx` and existing grid/Import tests. Reuses the existing annotations and their invalidation paths. No API changes or dependencies.

Non-goals: print history, persistent operation logs, per-row download-success state and changing batch validation or execution.
