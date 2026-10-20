# Proposal

## Why

The print-form list editor requires repeated earlier/later clicks to move an element several positions. A drag handle makes that move direct while the existing buttons provide the equivalent keyboard interaction.

Source issue: https://github.com/pfa230/labeler/issues/347

Fixes #347

## What Changes

- Add drag handles that move list elements to another displayed position.
- Retain the visible earlier/later buttons as the keyboard path.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `ui`: drag interaction for the print-form list control.

## Impact

Touches `ui/src/components/ParamInput.tsx` and its existing list/submission tests. No API, template schema or dependency changes.

Non-goals: grid list editing, cross-list moves, a new keyboard interaction mode and a drag-and-drop framework.
