# Proposal

## Why

Navigating away discards Labels filters and the edited Import batch because both screens keep their state only in mounted components. Operators need to inspect templates and return, including through Back or refresh, without rebuilding the batch.

Source issue: https://github.com/pfa230/labeler/issues/206

Fixes #206

## What Changes

- Restore Labels and Import data, operator choices and scroll position after navigation, Back and refresh within the current browser tab.
- Put Labels filters and Import's template selection in shareable URLs while keeping batch contents in the tab's private draft.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `ui`: restoration of Labels and CSV Import views.

## Impact

Touches `Templates.tsx`, `Import.tsx`, `LabelGrid.tsx`, shell navigation/scroll handling, a small tab-draft helper and their UI tests. Uses existing browser and router facilities without a new dependency.

Non-goals: persistence across browser sessions, server-side drafts, history of individual edits, and unsaved-YAML navigation guards or data-router conversion (https://github.com/pfa230/labeler/issues/143).
