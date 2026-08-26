# 62. The service may rewrite one key of a hand-authored template

Date: 2026-08-21

## Status

Superseded by [ADR-0073](0073-group-is-a-directory-id-is-the-filename.md). Issue [#164](https://github.com/pfa230/labeler/issues/164). Qualifies ADR-0006.

## Context

ADR-0006 ("Template edit ownership: manual vs GUI") established that hand-authored YAML templates should not be round-tripped through a full YAML parser/emitter, because full serialization destroys user comments, whitespace formatting, and custom key ordering.

However, organizing templates into groups is a frequent operational task that users perform from the Web UI. Forcing users to manually open text editors on server files just to categorize a template imposes unnecessary friction. Conversely, naive YAML deserialization and serialization would violate ADR-0006 by stripping comments and reordering keys across the entire template file.

## Decision

**Allow the service to update only the top-level `group` key of a template file in place via a targeted line patcher, with strict byte-preservation guarantees.**

1. **Strict byte preservation**:
   - Every line outside the target `group:` line is preserved byte-for-byte.
   - The line terminator (`\n` or `\r\n`) of the file and surrounding lines is preserved.
   - Trailing comments on the `group:` line (e.g. `group: Warehouse  # primary group`) are preserved across value updates.
   - Quoting rules are preserved when values contain special characters.
   - Clearing a group removes the `group:` line entirely; setting and clearing a group restores exact byte equality with the original un-grouped file.
2. **Safety and Refusal**:
   - The patcher refuses multi-document YAML files, flow-mapping roots, block-scalar group values, and files where `group:` cannot be unambiguously located at column 0.
   - Patched files are re-parsed and validated in memory before being committed to disk atomically.
3. **Idempotency**:
   - Updating a template to the group it already possesses performs zero writes and returns `200 OK`.

## Consequences

- Qualifies ADR-0006: the service may modify hand-authored template files *only* via this dedicated, non-destructive line patcher for the single top-level `group` key.
- Hand-authored comments, formatting, and structural indentation in template files remain 100% intact across group moves.
- Full layout edits remain governed by ADR-0006 (or explicit full PUT updates).
