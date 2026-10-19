## Why

Labels hides the registry's refused entries and says "No templates yet" when every entry is broken. Operators need the filenames and errors, then a way to reload after fixing files on disk without restarting the service.

Fixes #194

Source issue: https://github.com/pfa230/labeler/issues/194

## What Changes

- Show a refused-entry count above the printable grid, with expandable paths and refusal messages independent of search/category filters.
- Distinguish an all-refused registry from an empty installation.
- Add Reload templates to rebuild the registry and refresh the list, with visible failure feedback.

## Capabilities

### New Capabilities

### Modified Capabilities

- `ui`: refused-entry diagnostics and recovery after on-disk repair.

## Impact

`ui/src/pages/Templates.tsx`, existing API types/query hooks and Labels tests. Reuse the existing listing and registry-reload endpoints; repairs remain on disk. No browser repair editor, backend changes or new dependency.
