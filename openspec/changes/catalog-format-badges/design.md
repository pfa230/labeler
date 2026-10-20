# Design

## Context

At `3dddfb20`, the index generator already parses and validates each template, then emits `format` as a string. `CatalogEntry` carries that string and Catalog prints it in a description list. `FormatBadge` needs a full API `TemplateFormat` only to read its type and count `positions`. Existing badge tests cover its icons, palette tokens and labels; the catalog CI step checks the generated index. See the proposal and deltas for scope.

## Goals / Non-Goals

**Goal:** Supply exactly the badge's missing sheet count and reuse its rendering.

**Non-goal:** Publish coordinates or load template YAML just to display a card.

## Decisions

- User decision: include sheet counts in Catalog, matching installed-template badges. The user accepted this option during scoping on 2026-10-19.
- Default: retain the `format` string and add the numeric `position_count` only to sheet entries. Derive it from the already validated `TemplateFormat::Sheet.positions.len()`. A count is the only additional published value needed; a full format object publishes geometry the badge never reads. Regenerate the committed index through the existing generator.
- Refactor the existing badge to receive its type and a required count for sheets instead of full geometry. Installed-template callers pass `positions.length`; Catalog passes the published count. Update its existing tests/callers rather than adding a second badge, fabricating coordinate arrays or introducing a countless branch. Retain the current icons, palette tokens and `single` / `sheet · N` labels.
- Update the Catalog entry type to represent the selected format/count contract and use the shared badge in the existing card. Rely on the generated index contract; add no old-index fallback or runtime schema framework.
- Extend generator tests to compare emitted counts with a validated sheet's positions. Extend existing Catalog tests to assert both badge labels and reuse existing badge presentation tests. Exercise Catalog and installed-template badges in both themes in a browser, with evidence under `.agent-runs/`. Tests must fail on an incorrect/missing count or Catalog continuing to print its plain string.

## Risks / Trade-offs

The catalog index and its consumer change together. This follows the repository's pre-1.0 replacement policy and adds no compatibility branch for an older index.

## Migration Plan

Regenerate `catalog/index.json` with the generator change. Use the existing CI freshness check; no template or stored-data migration is required.
