# Proposal

## Why

A stored variable cannot be removed: the API, the store and the Settings page offer only list and upsert, so a key once added stays forever. Clearing its value is no substitute, because templates then read an empty string instead of an unresolved reference.

Source issue: https://github.com/pfa230/labeler/issues/503

Fixes #503

## What Changes

- Add `DELETE /api/variables/{key}`, which removes the key and answers `204` whether or not it was stored.
- Give each stored variable row in Settings a Remove action with an inline Confirm/Cancel step.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `settings`: the **Variables store** requirement gains deletion; the **Settings UI** requirement gains a confirmed Remove on stored variable rows.

## Impact

Touches `src/store.rs`, `src/api/settings.rs` and its OpenAPI path registration, `ui/src/api/queries.ts` and `ui/src/pages/settings/VariablesSection.tsx`, with their tests. No schema migration, new reason slug or dependency.
