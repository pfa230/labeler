## ADDED Requirements

### Requirement: A template write answers for the file it wrote, or fails

`POST /api/templates`, `PUT /api/templates/{id}` and `PUT /api/templates/{id}/group` SHALL, after
the directory re-read that follows their write, confirm both that the id they acted on is served from
the file they wrote and that the served content is byte-identical to what they wrote. They SHALL NOT
answer `2xx` with content the caller did not submit, whether it comes from another file or from the
same filename replaced by another writer.

The confirmation, not the mere existence of a duplicate, decides the outcome. A file elsewhere in
the directory declaring the same id but sorting after the written file does not displace it: the
written file still wins the id, the confirmation passes, and the request SHALL succeed normally while
that other file is reported in `broken[]` like any other refused file.

When the confirmation fails, the response SHALL be told apart by cause. `409` requires all four of
the following, and the service SHALL verify them against the directory rather than infer them from
the id no longer resolving to the written path:

- the id is served from a different file, which is the case a later-sorting duplicate cannot produce;
- the file the request wrote still exists;
- it still declares that id;
- it still holds what the request wrote.

Those are what the `409` asserts: that two files claim the id, that the caller's write is intact, and
that it is the one being refused. Any of them failing means something other than a collision happened
to the written file, and the response SHALL be `500` instead:
- the written file is gone, renamed, or re-identified;
- it holds content the request did not write, another writer having replaced it;
- the id is served from nothing at all.

No `500` case SHALL be reported as a collision: there is no second file declaring the id to name, and
no intact copy of the caller's write to point at. In particular, an external rename of the written
file to another name still serving the caller's own content is not a collision, because the file the
error would have to name no longer exists.

Each endpoint fails its own way, specified once:

- `PUT /api/templates/{id}` SHALL, when the id is served from a different file, fail with
  `409 TemplateIdCollision`, whose message names the id, the file the request wrote, and the file now
  serving the id. It SHALL keep the write it made: that
  write went to the file the registry held for the id, which is the file the caller addressed. Its
  `409` means "your edit is saved in that file, and that file no longer serves this id", and the
  written file is reported as broken by `GET /api/templates`.
- `POST /api/templates` SHALL fail the same way, with `409 TemplateIdCollision`. Its written file
  keeps the caller's content and is reported as broken, exactly as `PUT`'s is. No endpoint undoes a
  write: the file the caller submitted stays on disk, named in the error and visible in `broken[]`,
  where the operator resolves it.
- `PUT /api/templates/{id}/group` SHALL fail the same way `PUT /api/templates/{id}` does; its full
  contract, including the response table this adds `409` to, lives in the `template-groups`
  capability and is changed by this same change.

This requirement supersedes the `docs/SPEC.md` §2.0 bullets for `POST /templates` and `PUT
/templates/{id}` as to what a successful write may return, and the §2 endpoint table's success
column for those two endpoints to the extent of adding `409` to their possible responses. It does
not touch the frozen account of `PUT /templates/{id}/group`, which is already superseded by
`template-groups`.

#### Scenario: A create whose id is won by another file does not report success

- **WHEN** a file declaring `id: badge` is present on disk under a name sorting before `badge.yaml`,
  and `POST /api/templates` submits a body declaring `id: badge`
- **THEN** the response is `409` and not `201`
- **AND** the response body is not the other file's template

#### Scenario: A replace whose id moved to another file fails loudly

- **WHEN** `PUT /api/templates/badge` succeeds in writing the file the registry held for `badge`, and
  the re-read then serves `badge` from a different file that appeared on disk meanwhile
- **THEN** the response is `409 TemplateIdCollision` naming `badge`, the written file, and the file
  now serving `badge`
- **AND** the caller's content is in the file it wrote
- **AND** that file is reported as broken by `GET /api/templates`

#### Scenario: A later-sorting duplicate does not fail the write

- **WHEN** `PUT /api/templates/badge` writes `badge`'s file, and another file declaring `id: badge`
  exists under a name sorting after it
- **THEN** the response is `200` with the caller's own content
- **AND** the other file is reported as broken by `GET /api/templates`

#### Scenario: A group update is unaffected by a later-sorting duplicate

- **WHEN** `PUT /api/templates/{id}/group` patches the file serving the id, and another file declaring
  that id exists under a name sorting after it
- **THEN** the response is `200`
- **AND** the other file is reported as broken by `GET /api/templates`

#### Scenario: A replaced file is not reported as the caller's own

- **WHEN** a write endpoint writes its file and, before the re-read, another writer replaces that same
  filename with different valid content declaring the same id
- **THEN** the response is `500` reporting the template missing after the write
- **AND** the response does not present that content as the caller's
- **AND** the response is not `409`

#### Scenario: A vanished write is a 500, not a collision

- **WHEN** a write endpoint writes its file and, before the re-read, that file is removed, and no
  other file declares the id
- **THEN** the response is `500` reporting the template missing after the write
- **AND** the response is not `409`

#### Scenario: The ordinary write path is unaffected

- **WHEN** a write endpoint succeeds and the id is served from the file it wrote
- **THEN** the response is the endpoint's normal success status with that file's template

### Requirement: A create never overwrites a file it did not create

Before deciding that an id is free, `POST /api/templates` SHALL re-read the templates directory, so
the decision is made against the files on disk rather than against a registry that may predate them.

Two independent things block a create, and neither covers the other:

- an id held by any file the registry could serve it from, under any filename. This is the set of
  files that parse and validate: a file that fails either never contributes an id to the registry and
  cannot claim one, which matches how the load path already decides who holds an id;
- the destination filename `{id}.yaml` already existing, whatever it contains, including a file that
  parses badly or declares some other id entirely.

Either SHALL yield `409 TemplateExists` with nothing written. Together they mean an invalid file
declaring the id does not block the create by its content, but does block it if it happens to occupy
`{id}.yaml`.

This requirement supersedes the `docs/SPEC.md` §2.0 bullet "`POST /templates` creates from a raw YAML
body; the `id` comes from the body; `409 Conflict` if it already exists" as to what "already exists"
means, and the §3 sentence "`POST /templates` writes a new template as `{id}.yaml`" as to how that
file is created.

`POST /api/templates` SHALL publish its file only if that filename does not already exist, as a single
filesystem operation rather than a check followed by a write. The staging file it publishes from SHALL
itself be created exclusively, failing rather than truncating if its name is taken and never following
a symlink out of the templates directory. A file that appears at the destination name between the
re-read and the publish SHALL therefore cause `409 TemplateExists`, and its content SHALL be left
untouched.

Failing to remove the staging file after publication SHALL NOT change the response: the publication
already decided it, `201` or `409`, and a leftover staging file carries an extension the registry
ignores, so it is litter rather than a template. The service SHALL log it and answer as it would have.
That is what keeps "a `409 TemplateExists` leaves the directory unchanged" true of every file the
registry would load, which is the guarantee that requirement makes.

Those two rules together mean a `409 TemplateExists` from this endpoint always leaves every file the
registry would load exactly as it was. They do not cover a colliding file under a *different* name arriving in the same window;
that case is caught after the write by the confirmation, answered `409 TemplateIdCollision`, and the
caller's file stays on disk and refused, as it does for the other write endpoints. Nothing is deleted
to tidy it away.

#### Scenario: A create loses to a file the registry had not loaded

- **WHEN** a file declaring `id: badge` is copied into the templates directory without a reload, and
  `POST /api/templates` submits a body declaring `id: badge`
- **THEN** the response is `409 TemplateExists`
- **AND** the templates directory holds exactly the files it held before the request
- **AND** `badge` is still served from the copied file

#### Scenario: A create does not clobber a file that appears at its own filename

- **WHEN** `POST /api/templates` passes its pre-write check for `badge`, and another writer creates
  `badge.yaml` before the request publishes
- **THEN** the response is `409 TemplateExists`
- **AND** `badge.yaml` holds the other writer's content, not the request's

#### Scenario: A collider appearing mid-request leaves the caller's file quarantined

- **WHEN** `POST /api/templates` passes its pre-write check for `badge` and publishes `badge.yaml`,
  and a file declaring `id: badge` under an earlier-sorting name appears before the post-write re-read
- **THEN** the response is `409 TemplateIdCollision` naming both files
- **AND** `badge.yaml` still holds the caller's content
- **AND** `badge.yaml` is reported as broken by `GET /api/templates`

### Requirement: A delete is refused while the id collides on disk

`DELETE /api/templates/{id}` SHALL refuse the request with `409 TemplateIdCollision` when any file in
the templates directory other than the one serving the id also declares that id. The message SHALL
name the id and the filenames declaring it, so the operator knows which file to fix first.

A refused delete SHALL remove no file, SHALL prune no favorites, and SHALL leave the served set
unchanged.

A delete SHALL only be carried out when the id is declared by exactly one file, so a `204` means the
id was declared once at the moment the service checked and its file is gone.

That check is made against the templates directory as re-read while the request holds the write lock.
A file another process creates after the check is outside what a `204` promises, exactly as it is for
the write endpoints; it surfaces through the ordinary load-path channels at the next reload. The
service SHALL NOT claim more than its snapshot supports.

A file refused for a *different* id SHALL NOT block a delete.

This requirement supersedes the `docs/SPEC.md` §2.0 `DELETE /templates/{id}` bullet as to what a
successful delete guarantees, and the §2 endpoint table's response column for that endpoint to the
extent of adding `409`.

#### Scenario: Delete refuses while two files claim the id

- **WHEN** `alpha.yaml` and `zed.yaml` both declare `id: badge`, `badge` is served from `alpha.yaml`,
  and `DELETE /api/templates/badge` is called
- **THEN** the response is `409 TemplateIdCollision` naming `badge`, `alpha.yaml` and `zed.yaml`
- **AND** both files are still on disk
- **AND** `badge` is still served from `alpha.yaml`
- **AND** the favorites for `badge` are untouched

#### Scenario: The delete succeeds once the operator resolves the collision

- **WHEN** the operator removes or re-ids the colliding file and calls `DELETE /api/templates/badge`
- **THEN** the response is `204`
- **AND** `badge` is no longer served
- **AND** the favorites for `badge` are pruned

#### Scenario: A delete never leaves the id served by another file

- **WHEN** `DELETE /api/templates/{id}` returns `204` and no other process writes to the templates
  directory during the request
- **THEN** `GET /api/templates/{id}` returns `404`

### Requirement: The collision error is its own code

`TemplateIdCollision` with status `409` SHALL be the error code for exactly two conditions, and for
no other:

- `DELETE /api/templates/{id}` finding the id declared by more than one file;
- any of the three write endpoints finding, after its write, that the id is served from a different
  file while the file it wrote survives intact and refused.

An id declared by more than one file is otherwise not an error at all: the load path refuses the
colliding file and reports it in `broken[]`, and a duplicate that does not displace the written file
changes nothing. This code names a request whose result the service will not misdescribe, never the
mere existence of a duplicate.

Its `details` SHALL carry the id under `template` and, under `files`, the filenames that declared it
in the directory reading on which the service based its decision, in the same snapshot sense that
governs the delete refusal. It SHALL NOT be read as an assertion about the directory at the moment the
response is written: another process can rename or remove one of those files in between, and the
service does not re-check them to build an error message. Those
entries SHALL be bare filenames, exactly as `broken[]` reports them, never paths: the templates
directory's location is server configuration and does not belong in an error body.

`TemplateIdCollision` SHALL NOT carry a `details.reason`: `reason` is scoped to the `RenderFailed`,
`InvalidRequest`, `UnsupportedLayoutItem` and `TemplateInvalid` codes, and this is none of them.

`TemplateExists` keeps its meaning of "this id is already taken and nothing was written", and stays
the code `POST /api/templates` answers before it writes, whether the id was found in the re-read
directory or the filename turned out to be taken at publish time.

This requirement extends the `docs/SPEC.md` §10 error-code table with one row; every other row of
that table is unaffected.

#### Scenario: A collision error identifies the files

- **WHEN** any endpoint answers `409 TemplateIdCollision`
- **THEN** `error.code` is `TemplateIdCollision`
- **AND** `error.details.template` is the id
- **AND** `error.details.files` lists the bare filenames declaring that id, with no directory part
- **AND** `error.details.reason` is absent

## MODIFIED Requirements

### Requirement: A `422` from a template write means nothing was written

`POST /api/templates` and `PUT /api/templates/{id}` SHALL return `422 TemplateInvalid` only for a
submitted body that fails parsing or validation, which is rejected before anything is written. The
directory re-read that follows a successful write SHALL NOT produce a `422`: the faults it can still
raise are an unreadable directory or file, which is a `500`, and an id collision, which is a `409`.

`DELETE /api/templates/{id}` SHALL NOT return `422`. A file refused for a different id SHALL NOT
block a delete; a file declaring the id being deleted refuses it with `409`, per the requirement on
deleting a colliding id.

This requirement supersedes the `docs/SPEC.md` §2.0 `PUT /templates/{id}` bullet's account of a `422`
having two sources with different effects, including "Callers must not read a `422` as 'nothing was
saved'", and the §2.0 `DELETE /templates/{id}` bullet's `422` clause. It also supersedes the §2
endpoint table's success column for `POST /templates/reload` (`200 {"count":N}` / `422`), which now
reads `200 { "count": N, "broken_count": N }` / `500`.

#### Scenario: An invalid body is rejected before the write

- **WHEN** `PUT /api/templates/{id}` receives a body that fails validation
- **THEN** the response is `422`
- **AND** the stored file is unchanged

#### Scenario: A refused sibling does not block a delete

- **WHEN** `DELETE /api/templates/{id}` is called on a served template while another file in the
  directory is refused for an unrelated id
- **THEN** the response is `204`
- **AND** the refused file is still reported as broken
