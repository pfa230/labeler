# 73. Group is a directory; ID is the filename

Date: 2026-08-26

## Status

Accepted. Issue [#227](https://github.com/pfa230/labeler/issues/227). Supersedes [ADR-0061](0061-template-group-yaml-field.md) and [ADR-0062](0062-service-may-rewrite-single-template-key.md).

## Context

ADR-0061 and ADR-0062 previously specified storing template `id` and `group` as metadata fields inside template YAML files while keeping filesystem storage flat, using a specialized line patcher to modify the `group:` key without stripping comments or reformatting YAML.

In practice, this created several architectural issues:
1. Conflated identity with file contents: Moving or organizing files on disk into folders had no effect on group categorization, and copying a template file to a new filename created ID collisions rather than a new template.
2. In-place patching complexity: Modifying hand-authored YAML in place required non-trivial line-patching heuristics that were fragile and added unnecessary server complexity.
3. User expectations: Users and automation expect that placing template files into directories (e.g. `templates/Shipping/Pallets/box.yaml`) naturally assigns them to hierarchical groups and that filename stems define template IDs.

## Decision

**Make directory structure determine template group membership and file stem determine template ID.**

1. **Identity leaves the YAML file**:
   - The top-level `id:` and `group:` keys are removed from `TemplateContent`. Including either key in a template YAML file is rejected with `422 Unprocessable Entity` (`deny_unknown_fields`).
   - A template's `id` is strictly its filename stem (`<id>.yaml` or `<id>.yml`).
   - A template's `group` is strictly its directory path relative to `{LABELER_CONFIG_DIR}/templates/`, normalized to POSIX `/` separators. Files directly in the root templates directory have no group (`group: null`).

2. **Symlink boundary**:
   - Symlinks (whether to files or directories, pointing inside or outside the templates directory tree) are silently ignored during discovery and tree walking.
   - Symlinks are never followed, never traversed, never moved, and never unlinked or deleted by any API operation.

3. **Safe filesystem operations (`rustix`)**:
   - All filesystem operations within the template tree use descriptor-relative system calls (`openat`, `unlinkat`, `renameat`, `mkdirat`) with `O_NOFOLLOW` to guarantee race-free boundary confinement and prevent symlink traversal attacks.
   - Atomic publish and replace use temporary staging files in the target directory followed by atomic rename.

4. **API operations**:
   - `PUT /api/templates/{id}` with optional `?group=` query parameter handles both creation (with `If-None-Match: *`) and replacement. `POST /api/templates` is removed.
   - `PUT /api/templates/{id}/group` physically moves the template YAML file on disk to the target group directory, leaving any emptied source directory in place.
   - `GET /api/template-groups` returns all discoverable directory paths in ascending Unicode code-point order.
   - `DELETE /api/template-groups/{*path}` removes an empty group directory via `unlinkat(AT_REMOVEDIR)`, mapping `ENOTEMPTY` to `409 Conflict` and `ENOENT` to `404 Not Found`.

## Consequences

- Supersedes ADR-0061 (YAML `group:` field) and ADR-0062 (YAML line patcher), eliminating in-place YAML rewriting.
- Template files contain only rendering specifications (name, unit, dpi, params, format, layout).
- Organizing templates on disk via git, rsync, or file manager directly mirrors template grouping and IDs in the application.
- Downgrades back to older versions that require `id:` in YAML content will fail to load templates saved in this format.
