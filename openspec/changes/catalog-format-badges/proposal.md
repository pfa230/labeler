# Proposal

## Why

Catalog cards show plain format strings while installed-template cards use a format badge with sheet position counts. The catalog needs the validated count to present the same useful badge before installation.

Source issue: https://github.com/pfa230/labeler/issues/211

Fixes #211

## What Changes

- Publish a sheet position count from the existing validated catalog-index generator.
- Use the existing format badge on Catalog cards, including the sheet count.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `templates`: catalog-index count and Catalog's reference to the common format presentation.
- `ui`: common format badge presentation across installed and catalog cards.

## Impact

Touches `src/bin/catalog-index.rs`, generated `catalog/index.json`, `ui/src/api/catalog.ts`, `Catalog.tsx`, `FormatBadge.tsx` and its existing callers/tests. No server endpoint or template schema changes.

Non-goals: publishing sheet geometry, fetching YAML to discover counts at browse time, a countless badge variant and changing catalog installation.
