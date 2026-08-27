# 76. The filesystem answers the case question

Date: 2026-08-27

## Status

Accepted. Issue [#202](https://github.com/pfa230/labeler/issues/202). Amends [ADR-0073](0073-group-is-a-directory-id-is-the-filename.md).

## Context

ADR-0073 established that template groups are directories under `{LABELER_CONFIG_DIR}/templates/` and template IDs are filename stems. To avoid colliding with existing directories differing only by case on case-insensitive filesystems, the initial implementation of ADR-0073 performed a predictive lowercase-equality comparison across sibling entries prior to creation.

On case-sensitive Linux filesystems where the service deploys, this predictive check prohibited creating distinct groups that differed only by case (such as `Warehouse` and `warehouse`), even though the capability's normative specification established exact byte-for-byte comparison where the filesystem allows it. Furthermore, predictive checking cannot reliably account for all unicode folding rules or mount-specific filesystem behaviors across different subdirectories without attempting the operation.

## Decision

**Let the filesystem answer whether a case-differing directory can be created, and stop predicting case folding in application logic.**

1. **Remove predictive case checks**:
   - The lowercase-equality pre-check is removed.
   - Group resolution first lists the parent directory: if a byte-exact entry exists, it is safely opened with `O_DIRECTORY | O_NOFOLLOW` and reused (or rejected as `422 template_group_unsafe_path` if it is a file or symlink).

2. **Bounded create-and-classify state machine**:
   - If no byte-exact entry is listed, creation is attempted exclusively (`mkdirat`).
   - If `mkdirat` returns `EEXIST`, the parent directory is re-listed:
     - If a byte-exact directory appeared (concurrent creation), it is safely opened and reused.
     - If no byte-exact entry is present, the requested spelling is resolved without following symlinks. If it resolves to a directory, the filesystem has supplied a non-exact alias: the request is refused with `422 template_group_case_conflict`. The existing stored spelling is located by comparing listed sibling entries by `(st_dev, st_ino)` for the error message only (which never authorizes reuse or mutation).
     - If the entry resolves to a regular file or symbolic link, it is refused with `422 template_group_unsafe_path`.
     - If the occupant vanished between `mkdirat` and re-listing, `mkdirat` is retried once.
     - A second `EEXIST` performs one final re-list and classification. A second vanished occupant returns `500 template_registry_io`. There is no third create attempt.

3. **No-replace directory rename**:
   - `PUT /api/template-groups/{path}` performs an atomic no-replace directory rename via `renameat_with(..., RenameFlags::NOREPLACE)`.
   - The service never switches to an ordinary replacing rename based on inode or listing heuristics.
   - On case-sensitive filesystems, recasing a group (`shipping` to `Shipping`) succeeds as a free destination. On case-folding filesystems that report the alias as occupied, the operation safely returns `409 Conflict` without destroying data.

## Consequences

- Case-differing sibling groups can be created and navigated on case-sensitive filesystems.
- The service never performs intrusive probe writes at startup or unasked modifications to the template tree.
- Safe atomic operations protect against TOCTOU races during group creation and renaming.
