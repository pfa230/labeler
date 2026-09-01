## ADDED Requirements

### Requirement: The listing that resolves a group directory is read from the descriptor, and is complete or fails

Resolving, creating, renaming and deleting a group directory are decided by listing a parent
directory. `template-groups` already specifies what that listing is used for: list the parent and
match an entry byte for byte, re-list after an exclusive create reports the name exists, resolve
every component of a delete path by exact entry name, and compare listed entries by
`(st_dev, st_ino)` to name an existing group by its stored spelling. This requirement governs the
listing itself, and is the only place `template_registry_io` is defined for one.

**Scope.** This requirement covers the enumeration behind group resolution, creation, rename and
delete, and behind the exact-name and case-only-sibling decisions `template-groups` specifies. It
does not restate those matching rules, which `template-groups` owns. It does not govern the
read-only enumerations that build the registry at load and reload or answer
`GET /api/template-groups`: those walk the tree by path, have never been descriptor-relative, and are
unchanged here.

The service SHALL obtain the listing from the open descriptor it already holds for that directory.
It SHALL NOT enumerate through a pathname resolved independently of that descriptor, which includes
the descriptor-naming pseudo-filesystem paths `/proc/self/fd/<n>` and `/dev/fd/<n>` as well as the
directory's own absolute or relative path. Reopening relative to the descriptor is permitted and is
not such a pathname: `openat(fd, ".")` resolves through the descriptor rather than independently of
it, and an implementation may perform one internally.

The listing SHALL therefore describe the directory the descriptor refers to, whatever happens
concurrently to the path it was opened through: renaming or replacing that path SHALL NOT redirect
the listing to another directory.

The listing SHALL NOT depend on any pseudo-filesystem, including `/proc` and `/dev/fd`. A host on
which the service runs but which provides no such filesystem SHALL list directories normally. There
is no unsupported-platform outcome and no error reporting one.

Two kinds of entry SHALL be omitted from the listing, and no others: the traversal aliases `.` and
`..`, and any entry whose name is not valid UTF-8. Both omissions are intentional and neither is a
failure. Every retained entry SHALL be reported under the exact spelling the filesystem stores, since
matching an entry byte for byte and naming an existing group by its stored spelling both depend on
it.

Subject only to those two omissions, the listing SHALL be complete or the operation SHALL fail.
Complete means the enumeration reached the directory's normal end without a read error; it does not
mean that every name the directory holds appears. A listing truncated by a failure to read SHALL NOT
be returned and SHALL NOT be answered from, because a caller cannot tell a name that is absent from
the directory from a name that is absent from a truncated listing, and would otherwise answer "no
such group" or "no case conflict" on that evidence.

`template_registry_io` SHALL be returned for a listing only when the operating system reports a
failure reading that directory. When it is returned the response is `500 RenderFailed` with
`details.reason` `template_registry_io`, as the requirements naming that reason already specify.

This requirement supersedes the `docs/SPEC.md` §10.1 `details.reason` table row
`| RenderFailed | template_registry_io | Reading the templates directory failed. |`, for listings of
this kind, in two ways. It is narrowed: a listing that fails only because the host lacks a
pseudo-filesystem is not a failure to read the directory, and no longer occurs. It is widened: a
failure part way through reading a directory is such a failure, where it previously went unreported
and yielded a short listing. No other §10.1 row is affected, and no new `details.reason` value is
introduced.

#### Scenario: An empty group is deleted on a host with no `/proc`

- **WHEN** the service runs on a host that provides no `/proc` filesystem, `templates/Warehouse/` is
  an empty directory, and `DELETE /api/template-groups/Warehouse` is called
- **THEN** the response is `204` and the directory is removed
- **AND** no `500` carrying `details.reason` `template_registry_io` is returned

#### Scenario: An existing group is reused on a host with no `/proc`

- **WHEN** the service runs on a host that provides no `/proc` filesystem, `templates/Warehouse/`
  exists, and a template is created with `PUT /api/templates/{id}?group=Warehouse`
- **THEN** the response is `201` and the template file is written into the existing
  `templates/Warehouse/`
- **AND** no second directory is created and no `500` carrying `details.reason`
  `template_registry_io` is returned

#### Scenario: A case-only sibling is still decided by the stored spelling

- **WHEN** `templates/Warehouse/` exists on a filesystem that distinguishes case and a template is
  created with `PUT /api/templates/{id}?group=warehouse`
- **THEN** the listing reports the stored spelling `Warehouse`, which does not match `warehouse` byte
  for byte
- **AND** `warehouse` is created as a distinct sibling rather than reusing `Warehouse`, as
  `template-groups` requires

#### Scenario: A directory that cannot be read to the end fails rather than answering from part of it

- **WHEN** the operating system reports a failure part way through reading a directory whose listing
  resolves a group
- **THEN** the request fails with `500 RenderFailed` and `details.reason` `template_registry_io`
- **AND** no group is resolved, created, renamed or deleted on the strength of the entries read
  before the failure

#### Scenario: An unreadable directory is still an IO failure

- **WHEN** a directory whose listing resolves a group cannot be read at all
- **THEN** the request returns `500 RenderFailed` with `details.reason` `template_registry_io`
