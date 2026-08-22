# 61. A template's group is a YAML field, not its directory

Date: 2026-08-21

## Status

Accepted. Issue [#164](https://github.com/pfa230/labeler/issues/164).

## Context

As the number of label templates in an installation grows, users need a way to organize and categorize them (e.g. by domain such as Warehouse, Shipping, Retail, or Personal).

Two architectures for template grouping were considered:
1. **Filesystem directory hierarchy**: Storing templates in subdirectories under `{LABELER_CONFIG_DIR}/templates/` (e.g. `templates/Warehouse/badge.yaml`).
2. **Metadata field inside template YAML**: Storing an optional top-level `group` field in the template definition while leaving the filesystem directory flat (or directory structure independent of grouping).

Directory-based grouping carries several disadvantages:
- Moving a template between groups renames paths on disk, breaks external volume syncs, git tracking, or symlinks, and risks collisions across folders.
- It conflates physical storage layout with logical presentation categorization.
- Nested directory hierarchies invite complexity (nested groups, path traversal issues, multi-level UI trees).

## Decision

**Store a template's group as an optional top-level `group` string field in its YAML definition, not as a directory structure.**

1. **Flat single level**: A group is a single flat string (up to 64 characters). Slashes (e.g. `Shipping/Pallets`) have no structural meaning or hierarchy; they are plain Unicode text characters.
2. **Grouping is optional**: When omitted or empty, the template is considered ungrouped.
3. **Filtering and list APIs**: Group names are exposed in `TemplateSummary` and `TemplateDetail`, and `GET /api/templates?group=<name>` filters the returned list (with `?group=` filtering for ungrouped templates).
4. **Code-point ordering in UI**: Group chips and lists in the UI sort in ascending Unicode code-point order.

## Consequences

- Templates remain flat in `{LABELER_CONFIG_DIR}/templates/` (or any single directory), and their filesystem paths are decoupled from group assignments.
- Rollback consequence: because `raw.rs` uses `deny_unknown_fields`, an older server binary that does not recognize `group:` will reject/quarantine any template file carrying a `group:` field.
