# 55. Standardize on value interpolation for text and QR items

**Status:** Accepted (supersedes the dual-binding clause in [ADR-0010](0010-variable-interpolation-layer.md))

## Context

ADR-0010 introduced `value: "..."` string interpolation alongside legacy `name: field`. Because `value: "{field}"` completely subsumes `name: field`, maintaining both created redundant parsing in `raw.rs`/`convert.rs`, duplicate validation branches in `templates.rs`, and cognitive overhead in authoring.

## Decision

- Standardize all `text` and `qr` layout items on mandatory `value: String`.
- Remove `name:` entirely from `text` and `qr` item schemas.
- `image` items continue to support `name:` (multipart upload image lookup) and `src:` (asset file).
- Migrate all catalog and fixture templates to `value: "{field}"`.

## Consequences

- Eliminates redundant validation and AST branches.
- Unifies template syntax ahead of layout parameterization (Issue #162).
- Breaking change for external templates using `name:` on text/qr items.
