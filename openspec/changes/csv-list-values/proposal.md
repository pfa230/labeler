# Proposal

## Why

CSV Import hides declared list columns and drops their string values from requests. Operators need one unambiguous cell spelling that can represent the lists the API already accepts.

Source issue: https://github.com/pfa230/labeler/issues/320

Fixes #320

## What Changes

- Decode JSON arrays of strings in declared list cells; use an empty cell for omission and `[]` for a supplied empty list.
- Show list columns as editable JSON text and retain their text across template switches.
- Identify malformed list cells by row and column and prevent their submission.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `ui`: list-cell presentation, grid column inclusion and CSV list decoding.

## Impact

Touches browser Import resolution and grid presentation in `ui/src/pages/Import.tsx`, `ui/src/lib/` and `ui/src/components/LabelGrid.tsx`, their existing tests, and Import's inline help. The API and parameter declarations need no change.

Non-goals: list inference from separators, new declaration attributes, a second list-cell editor, and CSV export.
