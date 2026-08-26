# template-registry Specification

## Purpose
Defines how the templates directory is turned into the served template registry: which files are read, what happens to a file whose content is unusable, how two files claiming one id resolve, and how the refused files are reported to the operator. The registry is rebuilt from disk by this same contract at startup and on every reload.

## Requirements

### Requirement: Startup survives every template content fault

The service SHALL start whenever the templates tree is readable, regardless of the content of the
files in it. A file that fails to parse, fails validation, carries a rejected key, sits under a
directory whose name cannot be a group name, bears a filename that cannot be an id, or claims an id
another file already holds SHALL be excluded from the served set and reported as broken, and SHALL
NOT abort startup.

The service SHALL abort startup only when the templates directory itself, or a subdirectory of it
that is not skipped, cannot be read or enumerated, or an individual file cannot be read.

This requirement supersedes the `docs/SPEC.md` §3 sentence "Parsing rejects unknown fields
(`deny_unknown_fields`). An invalid template aborts server startup." for its second sentence only;
unknown-field rejection is unchanged.

#### Scenario: A malformed file does not stop the service

- **WHEN** the templates tree holds one valid template and one file that is not valid template YAML
- **THEN** the service starts
- **AND** the valid template is served
- **AND** the malformed file is reported as broken

#### Scenario: A badly named directory does not stop the service

- **WHEN** a directory whose name cannot be a group name holds a template
- **THEN** the service starts
- **AND** that template is reported as broken
- **AND** every other template is served

#### Scenario: An unreadable templates directory is fatal

- **WHEN** the templates directory cannot be read
- **THEN** the service exits with a fatal error naming the directory

#### Scenario: An unreadable subdirectory is fatal

- **WHEN** a subdirectory of the templates tree cannot be enumerated
- **THEN** the service exits with a fatal error naming it

### Requirement: Templates load in a deterministic order

The registry SHALL walk the templates directory recursively and load every file whose extension is
`yaml` or `yml`, compared case-insensitively, in ascending byte order of the file's path relative to
the templates directory. Files with any other extension SHALL be ignored, as SHALL every directory
whose name begins with `.`, together with everything beneath it.

Skipping outranks reporting, and the order is decided here rather than left to an implementation. A
leading `.` also fails group-name validation, so a directory such as `.attic` satisfies both rules at
once; it SHALL be skipped silently and SHALL NOT be reported as an invalid directory. Reporting it
would defeat the point of the dot convention, which is to give an operator somewhere inside
`templates/` to keep files the service should not look at. The same holds at any depth: a dot
directory nested under a valid group is skipped whole, and a valid-looking directory nested under a
dot directory is never reached to be judged.

Ordering is over the raw bytes of the relative path as the filesystem reports them, not over a
decoded string, so it is well defined even where a name is not valid UTF-8.

A file or directory whose name is not valid UTF-8 SHALL NOT be served. A directory so named is
refused together with everything beneath it, and a file so named is refused on its own; both are
reported as broken with their path converted lossily, and the message SHALL say the name is not valid
UTF-8, so an operator seeing two identical-looking paths knows why. Nothing is lost by the lossy
conversion beyond the report itself: an id must be non-empty ASCII letters, digits, `-` and `_`, so a
name that is not valid UTF-8 can never be a valid id, can never enter the served set, and can never
appear in a collision's `details.files`.

Loading in that order rather than in enumeration order is what makes the winner of an id collision a
property of the tree's contents instead of a property of this filesystem's enumeration. Two loads of
the same tree SHALL therefore produce the same served set, the same id-to-file mapping, and the same
broken list, on any machine.

#### Scenario: Load result does not depend on filesystem order

- **WHEN** the same tree of template files is loaded on two machines whose directory enumeration
  order differs
- **THEN** both loads serve the same templates from the same files
- **AND** both report the same broken files

#### Scenario: Nested files are loaded

- **WHEN** a valid template sits at `templates/Shipping/Pallets/euro.yaml`
- **THEN** it is served

#### Scenario: A dot-directory is skipped rather than reported as invalid

- **WHEN** `templates/.attic/` holds template files
- **THEN** nothing beneath it is served
- **AND** nothing beneath it, and not `.attic` itself, appears in `broken`
- **AND** `.attic` is not offered as a group

#### Scenario: A dot-directory inside a valid group is skipped too

- **WHEN** `templates/Warehouse/.old/pallet.yaml` exists alongside `templates/Warehouse/bin.yaml`
- **THEN** `bin` is served
- **AND** nothing under `.old` is served or reported

#### Scenario: A name that is not valid UTF-8 is refused, not served

- **WHEN** a file in the tree has a name that is not valid UTF-8
- **THEN** it is not served
- **AND** it is reported as broken with a lossily converted path and a message saying the name is not
  valid UTF-8

#### Scenario: A dot-directory is not walked

- **WHEN** `templates/.attic/old.yaml` holds a valid template
- **THEN** it is neither served nor reported as broken

### Requirement: A duplicate template id refuses the colliding file

When two or more files in the templates tree share a filename stem, and therefore an id, the registry
SHALL serve the one that **parses and validates** and whose path relative to the templates directory
sorts first in byte order among those, and SHALL refuse each later such file for that id.
`Shipping/pallet.yaml` therefore beats `Warehouse/pallet.yaml`, and `pallet.yaml` beats `pallet.yml`
in the same directory.

Only a **loadable** file can win or lose an id contest, and loadable means every gate this capability
sets, not merely the content ones: its path is valid UTF-8, no directory on its path is a
dot-directory or bears a name that fails group-name validation, its filename stem is a valid id, and
its content parses and validates. A file failing any of these is refused on that fault, reported with
it rather than as a duplicate, and does not claim the id.

The location gates matter as much as the content ones, and omitting them would be a live defect
rather than pedantry: validation is content-only now, so `templates/bad:name/pallet.yaml` parses and
validates perfectly while sitting under a directory whose templates are all refused. Were it eligible
it would sort before `templates/Warehouse/pallet.yaml` and displace a file that is in every way
serviceable, leaving the id served by nothing.

Two other questions turn on the same distinction, and are answered differently on purpose:

- **Whether a create may publish.** The destination filename being taken blocks a conditional create
  whatever the occupying file contains, including one that fails to parse. That test is about the
  name on disk, not about the served set, and is stated in the create requirement.
- **Whether a delete is refused.** A delete is refused when any *other* file the registry would walk
  shares the id's stem, whether or not it parses. A broken file named `pallet.yml` beside the served
  `pallet.yaml` is precisely the ambiguity the refusal exists to surface, and skipping it because it
  does not parse would delete one of two files the operator has to reconcile.

  "Would walk" is the boundary, and it is drawn to match the load path exactly. A file the walk never
  reaches is not a contender and SHALL NOT block a delete: anything under a dot-directory, which is
  skipped whole. A file the walk reaches but refuses on its location — an invalid directory name
  anywhere on its path, a filename stem that is not a valid id, a path that is not valid UTF-8 — is
  likewise not a contender, because it can never serve that id under any repair short of moving or
  renaming it, and blocking deletes on it would make an unrelated broken directory freeze the whole
  API surface for an id it cannot hold.

  `details.files` on the resulting `409` SHALL therefore list exactly the contenders, on the same
  terms. A path that is not valid UTF-8 cannot be a contender at all, so no lossily converted path
  can appear there, and the guarantee that every entry names a file the operator can act on holds
  without qualification.

A refused file SHALL:

- be excluded from the served set, leaving the winning file's template served and unchanged;
- appear in the broken list, identified by its own path relative to the templates directory;
- carry an error message naming the duplicated id, and the path of the file it collides with.

Refusing a colliding file SHALL NOT eject, alter, or invalidate the template already accepted for
that id, and SHALL NOT affect any other template in the tree.

A duplicate id SHALL NOT produce an HTTP error response at load. The `template_duplicate_id` reason
of `docs/SPEC.md` §10.1 therefore does not reach the wire: it names the cause carried in a `broken`
entry's message.

This requirement supersedes the `docs/SPEC.md` §3 top-level field table entry for `id` ("Required,
non-empty, unique across the directory") to the extent of what a collision does and of where
uniqueness is enforced: the id comes from the filename, uniqueness holds across the whole tree, and a
collision refuses the colliding file rather than refusing to run. It also supersedes the §10.1 row
for `template_duplicate_id` as to where that reason appears.

#### Scenario: A file under an invalid directory does not contest the id

- **WHEN** `templates/bad:name/pallet.yaml` and `templates/Warehouse/pallet.yaml` both parse and
  validate, and `bad:name` fails group-name validation
- **THEN** `pallet` is served from `templates/Warehouse/pallet.yaml`
- **AND** `templates/bad:name/pallet.yaml` is reported as broken for its directory, not as a
  duplicate id

#### Scenario: A broken earlier file does not claim the id

- **WHEN** `templates/Shipping/pallet.yaml` fails to parse and `templates/Warehouse/pallet.yaml` is
  valid
- **THEN** `pallet` is served from `templates/Warehouse/pallet.yaml`
- **AND** the broken file is reported with its parse error, not as a duplicate

#### Scenario: Two files declare one id

- **WHEN** `Shipping/pallet.yaml` and `Warehouse/pallet.yaml` are both valid
- **THEN** the service starts
- **AND** `pallet` is served from `Shipping/pallet.yaml`
- **AND** `Warehouse/pallet.yaml` is reported as broken with a message naming `pallet` and
  `Shipping/pallet.yaml`
- **AND** every other template in the tree is served normally

#### Scenario: Two extensions in one directory collide

- **WHEN** `Warehouse/pallet.yaml` and `Warehouse/pallet.yml` are both valid
- **THEN** `pallet` is served from `Warehouse/pallet.yaml`
- **AND** `Warehouse/pallet.yml` is reported as broken

#### Scenario: A colliding file cannot evict the served template

- **WHEN** a tree serving `pallet` from `Shipping/pallet.yaml` gains `Warehouse/pallet.yaml`, and the
  registry is reloaded
- **THEN** `pallet` is still served from `Shipping/pallet.yaml`
- **AND** `Warehouse/pallet.yaml` is reported as broken

#### Scenario: A collision resolves once the operator fixes it

- **WHEN** the operator renames or deletes the refused file and reloads the templates
- **THEN** the collision is gone from the broken list
- **AND** the surviving files are served

### Requirement: Refused templates are reported, not silently dropped

Every file the registry refuses SHALL be reported through all three of these channels:

- a warning log line at startup, carrying the file and the reason it was refused;
- the `broken` list of `GET /api/templates`, as `{ path, error }` entries carrying that same reason,
  omitted when empty;
- the `broken_count` of the `POST /api/templates/reload` response, alongside `count`, counting the
  refused files.

**BREAKING**: a `broken` entry's file is reported under `path`, as a path relative to the templates
directory, where it was reported under `filename` as a bare basename. A bare filename no longer
identifies a file: `Shipping/pallet.yaml` and `Warehouse/pallet.yaml` are two files with one
basename, and reporting either as `pallet.yaml` would leave the operator unable to tell which file to
fix.

`POST /api/templates/reload` SHALL succeed and swap in the new registry whenever the templates tree is
readable, including when some files were refused. It SHALL NOT fail on refused files.

This requirement supersedes the `docs/SPEC.md` §2.0 bullet "`POST /templates/reload` re-scans the dir
and returns `{ "count": N }`", and the §2.0 sentence "A reload that fails (an invalid file on disk)
returns `422` and keeps the previously-loaded set, so a bad file never takes the service down." A
reload now fails only when the tree or one of its files cannot be read; it then returns
`500 RenderFailed` with `details.reason` `template_registry_io` and keeps the previously-loaded set.
It also supersedes the §2 endpoint table's success column for `POST /templates/reload`
(`200 {"count":N}` / `422`), which now reads `200 { "count": N, "broken_count": N }` / `500`.

#### Scenario: Reload reports the collision instead of failing

- **WHEN** a duplicate id is created on disk and `POST /api/templates/reload` is called
- **THEN** the response is `200` with `count` for the served templates and `broken_count` including
  the refused file
- **AND** `GET /api/templates` lists the refused file under `broken` with its message

#### Scenario: A refused file is identified by its path

- **WHEN** `templates/Shipping/pallet.yaml` fails to parse
- **THEN** its `broken` entry carries `path` `Shipping/pallet.yaml`

#### Scenario: A refused file at the root carries a bare name

- **WHEN** `templates/broken.yaml` fails to parse
- **THEN** its `broken` entry carries `path` `broken.yaml`

#### Scenario: Reload keeps the live set when the directory is unreadable

- **WHEN** the templates directory cannot be read and `POST /api/templates/reload` is called
- **THEN** the request returns `500 RenderFailed` with `details.reason` `template_registry_io`
- **AND** the previously-loaded templates stay served

### Requirement: A `422` from a template write means nothing was written

Every `422` these endpoints raise SHALL describe the request, SHALL be decided before anything is
written, and SHALL leave the tree exactly as it was. There are two families of them, and no third:

- **the submitted body** fails parsing or validation, which is `422 TemplateInvalid`;
- **a group path in the request** fails validation, clashes with an existing sibling by case alone,
  or crosses a symbolic link, which is `422` carrying `template_group_invalid`,
  `template_group_case_conflict` or `template_group_unsafe_path` as the `template-groups` capability
  specifies. This family reaches `PUT /api/templates/{id}` through its `group` query parameter and
  `PUT /api/templates/{id}/group` through its body, and both are decided before any directory is
  created or any byte written.

A `422` therefore never describes a fault the service found in a file on disk. The tree re-read that
follows a successful write SHALL NOT produce one: the faults it can still raise are an unreadable
tree or file, which is a `500`, and an id collision, which is a `409`. A file refused for a different
id SHALL NOT block a delete; a file declaring the id being deleted refuses it with `409`, per the
requirement on deleting a colliding id.

`DELETE /api/templates/{id}` takes no group path, so the second family cannot reach it through the
request. It can still find its own backing file unsafe to unlink, which is not a `422` at all: that
is a `500` carrying `template_group_unsafe_path`, because nothing in the request was wrong and the
caller cannot fix it by asking differently. `DELETE /api/template-groups/{path}` does take a group
path, and answers `400` for both members of the second family, as its own requirement states.

This requirement supersedes the `docs/SPEC.md` §2.0 `PUT /templates/{id}` bullet's account of a `422`
having two sources with different effects, including "Callers must not read a `422` as 'nothing was
saved'", and the §2.0 `DELETE /templates/{id}` bullet's `422` clause.

#### Scenario: An invalid body is rejected before the write

- **WHEN** `PUT /api/templates/{id}` receives a body that fails validation
- **THEN** the response is `422`
- **AND** the stored file is unchanged

#### Scenario: An invalid group path is rejected before the move

- **WHEN** `PUT /api/templates/{id}/group` receives a group path that fails validation
- **THEN** the response is `422`
- **AND** no file is moved and no directory is created

#### Scenario: A refused sibling does not block a delete

- **WHEN** `DELETE /api/templates/{id}` is called on a served template while another file in the tree
  is refused for an unrelated id
- **THEN** the response is `204`
- **AND** the refused file is still reported as broken

### Requirement: A template write answers for the file it wrote, or fails

`PUT /api/templates/{id}` and `PUT /api/templates/{id}/group` SHALL, after the tree re-read that
follows their write, confirm both that the id they acted on is served from the file they wrote or
moved and that the served content is byte-identical to it. They SHALL NOT answer `2xx` with content
the caller did not submit, whether it comes from another file or from the same path replaced by
another writer.

The confirmation, not the mere existence of a duplicate, decides the outcome. A file elsewhere in the
tree sharing the id but sorting after the written file does not displace it: the written file still
wins the id, the confirmation passes, and the request SHALL succeed normally while that other file is
reported in `broken[]` like any other refused file.

When the confirmation fails, the response SHALL be told apart by cause. `409` requires all four of
the following, and the service SHALL verify them against the tree rather than infer them from the id
no longer resolving to the written path:

- the id is served from a different file, which is the case a later-sorting duplicate cannot produce;
- the file the request wrote or moved still exists at the path it wrote;
- that path still yields that id, which for a filename-borne id means the file is still named
  `{id}.yaml` or `{id}.yml`;
- it still holds what the request wrote or moved.

Those are what the `409` asserts: that two files claim the id, that the caller's write is intact, and
that it is the one being refused. Any of them failing means something other than a collision happened
to the written file, and the response SHALL be `500` instead:

- the written file is gone or renamed;
- it holds content the request did not write, another writer having replaced it;
- the id is served from nothing at all.

No `500` case SHALL be reported as a collision: there is no second file declaring the id to name, and
no intact copy of the caller's write to point at.

Each endpoint fails its own way:

- `PUT /api/templates/{id}` SHALL, when the id is served from a different file, fail with
  `409 TemplateIdCollision`, whose message names the id, the path the request wrote, and the path now
  serving the id. It SHALL keep the write it made: that write went to the file the registry held for
  the id on a replace, or to the destination the caller named on a create. Its `409` means "your edit
  is saved at that path, and that path no longer serves this id", and the written file is reported as
  broken by `GET /api/templates`.
- `PUT /api/templates/{id}/group` SHALL fail the same way; its full contract, including the response
  table this `409` belongs to, lives in the `template-groups` capability. Its write is a move, so the
  file the `409` names is the destination.

  A move publishes at the moment its destination exists, not at the moment its source stops existing.
  On the atomic path the two coincide. On the link-then-unlink fallback they do not, and a failure of
  the second step SHALL be `500`: the destination keeps the caller's file, the source is left exactly
  as it is, no directory the request created is removed, and the two paths become an ordinary
  duplicate-id collision that the next load reports through `broken[]`. The service SHALL NOT unlink
  the destination to tidy this up, which would undo a published write, and SHALL NOT report it as
  `409`, which asserts a second file claiming the id rather than the request's own two copies. The
  source SHALL be unlinked through the same resolved directory handle and `O_NOFOLLOW` discipline as
  every other mutation, so a source replaced between resolution and removal is refused rather than
  followed.

No endpoint undoes its write: the file the caller submitted stays on disk, named in the error and
visible in `broken[]`, where the operator resolves it.

This requirement supersedes the `docs/SPEC.md` §2.0 bullets for `POST /templates` and
`PUT /templates/{id}` as to what a successful write may return, and the §2 endpoint table's success
column for those endpoints to the extent of adding `409`.

#### Scenario: A create whose id is won by another file does not report success

- **WHEN** `PUT /api/templates/pallet` creates its file, and a file sharing the id under an
  earlier-sorting path appears before the post-write re-read
- **THEN** the response is `409` and not `201`
- **AND** the response body is not the other file's template
- **AND** the created file still holds the caller's content, reported as broken

#### Scenario: A replace whose id moved to another file fails loudly

- **WHEN** `PUT /api/templates/pallet` succeeds in writing the file the registry held for `pallet`,
  and the re-read then serves `pallet` from a different file that appeared on disk meanwhile
- **THEN** the response is `409 TemplateIdCollision` naming `pallet`, the written path, and the path
  now serving `pallet`
- **AND** the caller's content is in the file it wrote
- **AND** that file is reported as broken by `GET /api/templates`

#### Scenario: A later-sorting duplicate does not fail the write

- **WHEN** `PUT /api/templates/pallet` writes `Shipping/pallet.yaml`, and `Warehouse/pallet.yaml`
  exists
- **THEN** the response is the endpoint's normal success status with the caller's own content
- **AND** `Warehouse/pallet.yaml` is reported as broken by `GET /api/templates`

#### Scenario: A move whose source cannot be removed after linking keeps both

- **WHEN** a move on the link-then-unlink fallback publishes its destination and the source cannot
  then be removed
- **THEN** the response is `500`
- **AND** the destination keeps the caller's file
- **AND** the source is left where it is, and the next load reports the two as a duplicate id
- **AND** no directory the request created is removed

#### Scenario: A group update is unaffected by a later-sorting duplicate

- **WHEN** `PUT /api/templates/{id}/group` moves the file serving the id, and another file sharing
  that id exists at a later-sorting path
- **THEN** the response is `200`
- **AND** the other file is reported as broken by `GET /api/templates`

#### Scenario: A replaced file is not reported as the caller's own

- **WHEN** a write endpoint writes its file and, before the re-read, another writer replaces that
  same path with different valid content
- **THEN** the response is `500` reporting the template missing after the write
- **AND** the response does not present that content as the caller's
- **AND** the response is not `409`

#### Scenario: A vanished write is a 500, not a collision

- **WHEN** a write endpoint writes its file and, before the re-read, that file is removed, and no
  other file holds the id
- **THEN** the response is `500` reporting the template missing after the write
- **AND** the response is not `409`

#### Scenario: The ordinary write path is unaffected

- **WHEN** a write endpoint succeeds and the id is served from the file it wrote
- **THEN** the response is the endpoint's normal success status with that file's template

### Requirement: A delete is refused while the id collides on disk

`DELETE /api/templates/{id}` SHALL refuse the request with `409 TemplateIdCollision` when any file in
the templates tree other than the one serving the id is a **contender** for that id, in the exact
sense the duplicate-id requirement defines: the registry's walk reaches it, and nothing about its
location disqualifies it from ever serving the id. Sharing the filename stem is what makes it a
contender; failing to parse does not disqualify it, since an unparseable namesake is precisely the
ambiguity this refusal exists to surface.

A file that is not a contender SHALL NOT block a delete: anything under a dot-directory, which the
walk never reaches, and anything the walk refuses on its location — an invalid directory name on its
path, a stem that is not a valid id, a path that is not valid UTF-8. None of those can serve the id
under any repair short of moving or renaming, so blocking on them would let an unrelated broken
directory freeze an id's whole API surface.

The message SHALL name the id and the paths of the contenders, so the operator knows which file to
fix first.

A refused delete SHALL remove no file, SHALL prune no favorites, and SHALL leave the served set
unchanged.

A delete SHALL only be carried out when exactly one contender holds the id, so a `204` means the id
had one contender at the moment the service checked and its file is gone.

A delete SHALL remove the template's file and nothing else. It SHALL leave the directory that held it
in place, even when that file was the last template in it: the group survives its members and is
removed only through `DELETE /api/template-groups/{path}`.

That check is made against the templates tree as re-read while the request holds the write lock. A
file another process creates after the check is outside what a `204` promises, exactly as it is for
the write endpoints; it surfaces through the ordinary load-path channels at the next reload. The
service SHALL NOT claim more than its snapshot supports.

A file refused for a *different* id SHALL NOT block a delete, and neither SHALL a namesake that is
not a contender.

This requirement supersedes the `docs/SPEC.md` §2.0 `DELETE /templates/{id}` bullet as to what a
successful delete guarantees, and the §2 endpoint table's response column for that endpoint to the
extent of adding `409`.

#### Scenario: A location-invalid namesake does not block a delete

- **WHEN** `templates/Warehouse/pallet.yaml` is served and `templates/bad:name/pallet.yaml` exists
  under a directory whose name fails validation
- **THEN** `DELETE /api/templates/pallet` returns `204`
- **AND** `templates/bad:name/pallet.yaml` is untouched and still reported as broken

#### Scenario: A namesake under a dot-directory does not block a delete

- **WHEN** `templates/Warehouse/pallet.yaml` is served and `templates/.attic/pallet.yaml` exists
- **THEN** `DELETE /api/templates/pallet` returns `204`

#### Scenario: Delete refuses while two files claim the id

- **WHEN** `Shipping/pallet.yaml` and `Warehouse/pallet.yaml` both exist, `pallet` is served from the
  first, and `DELETE /api/templates/pallet` is called
- **THEN** the response is `409 TemplateIdCollision` naming `pallet` and both paths
- **AND** both files are still on disk
- **AND** the favorites for `pallet` are untouched

#### Scenario: The delete succeeds once the operator resolves the collision

- **WHEN** the operator removes or renames the colliding file and calls `DELETE /api/templates/pallet`
- **THEN** the response is `204`
- **AND** `pallet` is no longer served
- **AND** the favorites for `pallet` are pruned

#### Scenario: A delete leaves the group behind

- **WHEN** the last template in `Shipping` is deleted
- **THEN** the response is `204`
- **AND** `Shipping` is still listed by `GET /api/template-groups`

#### Scenario: A delete never leaves the id served by another file

- **WHEN** `DELETE /api/templates/{id}` returns `204` and no other process writes to the templates
  tree during the request
- **THEN** `GET /api/templates/{id}` returns `404`

### Requirement: The collision error is its own code

`TemplateIdCollision` with status `409` SHALL be the error code for exactly these conditions, and for
no other:

- `DELETE /api/templates/{id}` finding the id held by more than one file;
- `PUT /api/templates/{id}` or `PUT /api/templates/{id}/group` finding, after its write, that the id
  is served from a different file while the file it wrote survives intact and refused;
- `PUT /api/templates/{id}/group` finding the destination directory already holding a file for that
  id, which it refuses before moving anything.

An id held by more than one file is otherwise not an error at all: the load path refuses the
colliding file and reports it in `broken[]`, and a duplicate that does not displace the written file
changes nothing. This code names a request whose result the service will not misdescribe, never the
mere existence of a duplicate.

Its `details` SHALL carry the id under `template` and, under `files`, the paths that held it in the
tree reading on which the service based its decision, relative to the templates directory. **BREAKING**:
those entries were bare filenames and are now relative paths, for the same reason `broken` entries
are: a basename no longer identifies a file. They still never carry the templates directory's own
location, which is server configuration and does not belong in an error body.

It SHALL NOT be read as an assertion about the tree at the moment the response is written: another
process can rename or remove one of those files in between, and the service does not re-check them to
build an error message.

`TemplateIdCollision` SHALL NOT carry a `details.reason`: `reason` is scoped to the `RenderFailed`,
`InvalidRequest`, `UnsupportedLayoutItem` and `TemplateInvalid` codes, and this is none of them.

`template_group_mismatch` and `unsupported_precondition` are additions to the reason registry of
`docs/SPEC.md` §10.1, which is frozen and therefore does not list them; the create requirement above
is their published home. Both ride `InvalidRequest`, one of §10's four reasoned codes, so that
scoping rule is unchanged.

`PreconditionFailed` with status `412` SHALL be the code for a conditional request whose precondition
did not hold, which today is `PUT /api/templates/{id}` sent with `If-None-Match: *` against a taken
id or a taken destination filename. It means "this id is already taken and nothing was written",
which is the meaning `TemplateExists` carried for the removed `POST /api/templates`.

This requirement extends the `docs/SPEC.md` §10 error-code table with these rows and removes the
`TemplateExists` row along with the endpoint that raised it; every other row of that table is
unaffected.

#### Scenario: A collision error identifies the files

- **WHEN** any endpoint answers `409 TemplateIdCollision`
- **THEN** `error.code` is `TemplateIdCollision`
- **AND** `error.details.template` is the id
- **AND** `error.details.files` lists the paths holding that id, relative to the templates directory
- **AND** `error.details.reason` is absent

#### Scenario: A failed precondition is its own code

- **WHEN** `PUT /api/templates/{id}` with `If-None-Match: *` finds the id taken
- **THEN** the response is `412` with `error.code` `PreconditionFailed`
- **AND** nothing was written

### Requirement: A template's id is its filename stem

A template's id SHALL be the stem of its filename: the basename with the `yaml` or `yml` extension
removed. `templates/Shipping/pallet.yaml` is the template `pallet`, in the group `Shipping`.

An id SHALL be non-empty and composed only of ASCII letters, digits, `-` and `_`, which is the
charset the file-backed routes already require of a path id. A file whose stem falls outside it SHALL
be refused and reported as broken, with a message naming the file and the rule. That is a quarantine
like any other: the rest of the tree loads.

An id SHALL be unique across the whole tree, not merely within one directory, and two files whose
stems are equal collide however far apart they sit.

`id` SHALL be rejected as an unknown top-level key, on the same terms as any other unknown field, and
so SHALL `group`. A file carrying either is refused and reported as broken carrying the parser's own
message, exactly as a file carrying any other unknown key is. Nothing special is owed to a file
written for the old model: it is an invalid template, reported like every other invalid template.
There is no second declaration of the id to agree or disagree with the
filename, so no request can provoke the `template_id_mismatch` reason of `docs/SPEC.md` §10.1.

That reason SHALL remain declared in the reason registry while ceasing to be emitted, exactly as
`template_duplicate_id` did when a duplicate id stopped reaching the wire. §10.1 is frozen, so its
row for `template_id_mismatch` stands, and the gate that pins the registry against that table asserts
in both directions: a row with no declared reason fails it. Retiring the emit site is this change's
business; retiring the row is not, and teaching the gate to forgive a missing row would be a contract
change no requirement here authorizes.

The id SHALL NOT change when a template moves between groups: the file keeps its name, so favorites
and job history keep resolving to the same template.

This requirement supersedes, in `docs/SPEC.md`:

- the §3 top-level field table entry for `id`;
- the §10.1 row for `template_id_mismatch`;
- the whole §3 paragraph beginning "A file's name need not match the `id` inside it", including its
  claims that the registry keys on the `id` in the file, that `GET /templates/{id}/source`, `PUT` and
  `DELETE` act on the file the id was remembered from, and that "`POST /templates` writes a new
  template as `{id}.yaml`". Under this requirement the filename *is* the id, so a name that does not
  match is not a divergence to remember but a different template; the file-backed routes resolve
  through the registry exactly as before, and `POST /templates` no longer exists.

The post-change field table lives in the `template-groups` capability.

#### Scenario: The filename names the template

- **WHEN** a valid template file is stored at `templates/Shipping/pallet.yaml`
- **THEN** `GET /api/templates/pallet` returns it
- **AND** its group is `Shipping`

#### Scenario: A `.yml` extension names the same way

- **WHEN** a valid template file is stored at `templates/pallet.yml`
- **THEN** its id is `pallet`

#### Scenario: An id declared in the file is refused

- **WHEN** a file otherwise valid declares a top-level `id:` key
- **THEN** it is refused and reported as broken with a message naming the unknown field

#### Scenario: A group declared in the file is refused

- **WHEN** a file otherwise valid declares a top-level `group:` key
- **THEN** it is refused and reported as broken with a message naming the unknown field

#### Scenario: A filename that cannot be an id is refused

- **WHEN** `templates/my template.yaml` holds an otherwise valid template
- **THEN** it is refused and reported as broken with a message naming the file
- **AND** every other template in the tree is served

#### Scenario: Two directories cannot hold the same id

- **WHEN** `templates/Shipping/pallet.yaml` and `templates/Warehouse/pallet.yaml` are both valid
- **THEN** one of them is refused as a duplicate id, per the duplicate-id requirement

### Requirement: A create is a conditional `PUT`, and never overwrites a file it did not create

`PUT /api/templates/{id}` SHALL create the template when the id is free and replace it when the id is
held, the client naming the resource it writes. It takes the same raw `text/yaml` body as before.

It SHALL accept an optional `group` query parameter, a group path validated by the `template-groups`
capability, naming the directory a **created** file is written to; absent means the root of
`templates/`, so the template is ungrouped. Any missing directory on that path SHALL be created.

On a **replace**, the file stays where it is: the service SHALL write the file the registry serves
for the id, wherever in the tree that is. A `group` parameter present on a replace SHALL be accepted
only when it equals the template's current group; otherwise the request SHALL be refused with `400`
and `details.reason` `template_group_mismatch`, whose message names `PUT /api/templates/{id}/group` as
the way to move a template. Replacing SHALL NOT move a file, so a caller cannot move a template by
accident while editing it.

A caller that means "create, and fail if it exists" SHALL send `If-None-Match: *`. `*` is the only
value this endpoint supports: an `If-None-Match` carrying entity tags instead SHALL be refused with
`400` and `details.reason` `unsupported_precondition`, rather than being ignored, since ignoring a
precondition a caller sent is how an overwrite happens that nobody asked for. With that header,
a request whose id is already held, or whose destination filename is already taken by anything at
all, SHALL be refused with `412 PreconditionFailed` and nothing written. Without it, the same request
replaces. This is what replaces the removed `POST /api/templates` and its `409 TemplateExists`.

Before deciding whether the id is free, the service SHALL re-read the templates tree, so the decision
is made against the files on disk rather than against a registry that may predate them. Files reach
this tree by hand.

**Two publish mechanisms, and which one runs.** A request classified as a create publishes
*exclusively*: a single filesystem operation that fails rather than overwriting if the destination
name is taken. A request classified as a replace publishes by *replacing* the file at the path the
registry serves for the id. The staging file both publish from SHALL itself be created exclusively,
failing rather than truncating if its name is taken, and never following a symlink out of the
templates tree.

A file that appears at the destination name between the pre-write re-read and the publish is
therefore caught by the exclusive publish, never overwritten by it. What happens next depends on the
header, and SHALL be exactly this:

- with `If-None-Match: *`, the request is refused `412` and the file that appeared keeps its content;
- without it, the request re-classifies as a replace of that same path and publishes again by
  replacing. It SHALL re-classify at most once: a second exclusive failure means the tree is being
  written by another process faster than this request can act, and the response SHALL be `500` rather
  than a retry loop.

The no-symlink rule outranks the re-classification, and the order is not optional. Before a request
re-classifies, it SHALL establish that the occupying destination is not a symbolic link, by opening
that name in the resolved directory in a way that refuses to follow one. A destination that is a
symbolic link SHALL abort with `500` and `details.reason` `template_group_unsafe_path`, whatever the
header said and whether the request began as a create or a replace: replacing a link would write
through it, which is the one thing the containment rule exists to prevent. "Occupied by anything at
all" therefore means anything the service may itself publish over, and a symlink is not among them.

The re-classification is what makes an unconditional `PUT` mean "create or replace" without ever
overwriting through a check-then-write race: the first attempt refuses to clobber, and only then does
the request decide, knowingly, to replace.

**A create refused before it publishes SHALL leave no group behind.** Creating the directories on the
way to `?group=` happens before the destination can be found taken, so a request refused *before it
publishes its file* — `412`, an unsafe path, an invalid body — SHALL remove every directory it
created, innermost first, stopping at the first that is no longer empty. A retried conditional create
SHALL NOT be able to litter the group tree with empty groups that then need the delete route to clean
up. A directory that existed before the request SHALL NOT be removed.

Like every other guarantee this capability makes about the tree, this one is bounded by the snapshot
the request acted on. If another process populates a directory this request created before the
cleanup reaches it, the cleanup stops there as specified and that directory remains a group. The
`422` guarantee that the tree is exactly unchanged carries the same condition, stated once here for
both.

Once the file is published the rule stops applying, and does not conflict with the post-write
failures below: a `409` or a `500` from the confirmation keeps the caller's file exactly where it was
written, so the directory holding it is not empty and SHALL NOT be removed. No endpoint undoes its
own write, and no cleanup rule may be read as authorising one.

Failing to remove the staging file after publication SHALL NOT change the response: the publication
already decided it, and a leftover staging file carries an extension the registry ignores, so it is
litter rather than a template. The service SHALL log it and answer as it would have.

Responses:

| Status | Meaning |
| --- | --- |
| `201` | Created. Body is the new `TemplateDetail`. |
| `200` | Replaced. Body is the updated `TemplateDetail`. |
| `400` | The path id is invalid, a `group` parameter contradicts a replaced template's group, or `If-None-Match` carries anything but `*`. |
| `409` | After the write, the id is served from a different file. |
| `412` | `If-None-Match: *` was sent and the id, or the destination filename, is already taken. |
| `422` | The submitted body fails parsing or validation, or the `group` parameter fails validation, clashes by case, or crosses a symbolic link. Nothing is written. |
| `500` | The write failed, the tree could not be re-read, or the written template is missing afterwards. |

`GET /api/openapi.json` SHALL document the `group` parameter, the `If-None-Match` header, and every
status above.

This requirement supersedes, in `docs/SPEC.md`:

- the §2 endpoint-table row for `PUT /api/templates/{id}` and the §2.0 bullet for
  `PUT /templates/{id}`, in full: creation, group placement, conditional requests, the removal of the
  body-`id`-must-equal-path-`id` `400`, and the removal of the `404` for an id that does not exist,
  which a create-or-replace `PUT` cannot raise;
- the §2 endpoint-table row for `POST /api/templates` and the §2.0 bullet
  "`POST /templates` creates from a raw YAML body; the `id` comes from the body; `409 Conflict` if it
  already exists", both of which are deleted with the endpoint. No route answers `POST /api/templates`
  after this change, and a request to it SHALL be a `405` or `404` from the router like any other
  unrouted method, not a template error;
- the §12 **Unreleased** entry's naming of raw-YAML `POST` among the template management endpoints
  (#10). That entry records what shipped and stays true of the past; it is named here so no reading
  of it survives as a current obligation.

- the sentence in the §12 changelog entry dated 2026-08-08 reading "The browser fetches the entry
  from GitHub raw and POSTs it to the existing `POST /api/templates` — the server makes no outbound
  request, so air-gapped installs behave the same and fall back to pasting YAML." Everything that
  sentence says about *where the bytes come from* stands: the browser still fetches the catalog entry
  itself, the server still makes no outbound request, air-gapped installs still behave the same, and
  pasting YAML is still the fallback. Only the request it sends changes.

**The post-change catalog install.** The browser SHALL fetch the catalog entry, take the id from
`catalog/index.json`, and send `PUT /api/templates/{id}` with `If-None-Match: *`. A `412` SHALL mean
"you already have this one", which is what the install view reads to offer a diff instead of
overwriting; that is the same signal the removed `409 TemplateExists` carried, and the only change on
the UI side is which status it matches. An install SHALL NOT send the header's absence as a way to
overwrite silently: replacing an installed template stays a deliberate act through the editor.

Together with the id requirement above, which supersedes the §3 paragraph naming `POST /templates`,
no frozen clause still publishes that endpoint.

#### Scenario: A catalog install refuses to overwrite what is already installed

- **WHEN** the catalog view installs an entry whose id the registry already holds
- **THEN** the request is `PUT /api/templates/{id}` with `If-None-Match: *`
- **AND** the response is `412`
- **AND** the stored template is unchanged
- **AND** the view offers a diff rather than reporting an install

#### Scenario: A PUT to a free id creates

- **WHEN** `PUT /api/templates/pallet` is called with a valid body and no template `pallet` exists
- **THEN** the response is `201`
- **AND** the file is at `templates/pallet.yaml`

#### Scenario: A create places the file in a group

- **WHEN** `PUT /api/templates/pallet?group=Shipping/Pallets` creates a template
- **THEN** the file is at `templates/Shipping/Pallets/pallet.yaml`
- **AND** both directories exist

#### Scenario: A PUT to a held id replaces in place

- **WHEN** `pallet` is served from `templates/Shipping/pallet.yaml` and `PUT /api/templates/pallet`
  is called with a valid body
- **THEN** the response is `200`
- **AND** the file at `templates/Shipping/pallet.yaml` holds the new body
- **AND** no other file is created

#### Scenario: A replace cannot move a template

- **WHEN** `pallet` is in `Shipping` and `PUT /api/templates/pallet?group=Warehouse` is called
- **THEN** the response is `400` with `details.reason` `template_group_mismatch`
- **AND** nothing is written

#### Scenario: A conditional create refuses a taken id

- **WHEN** `PUT /api/templates/pallet` is called with `If-None-Match: *` while `pallet` is served
- **THEN** the response is `412`
- **AND** the stored file is unchanged

#### Scenario: An entity-tag precondition is refused, not ignored

- **WHEN** `PUT /api/templates/pallet` is called with `If-None-Match: "abc123"`
- **THEN** the response is `400` with `details.reason` `unsupported_precondition`
- **AND** nothing is written

#### Scenario: An unconditional PUT does not replace a symlinked destination

- **WHEN** `PUT /api/templates/pallet` without `If-None-Match` finds its destination occupied and
  that name is a symbolic link
- **THEN** the response is `500` with `details.reason` `template_group_unsafe_path`
- **AND** the request does not re-classify as a replace
- **AND** the link and its target are unchanged

#### Scenario: An unconditional PUT whose destination appears mid-request replaces it

- **WHEN** `PUT /api/templates/pallet` without `If-None-Match` is classified as a create, and the
  destination file appears before it publishes
- **THEN** the exclusive publish refuses rather than overwriting
- **AND** the request re-classifies as a replace and publishes by replacing that path
- **AND** the response is `200`

#### Scenario: A conditional create refuses a taken filename

- **WHEN** `PUT /api/templates/pallet` is called with `If-None-Match: *` and `group=Shipping`, and
  `templates/Shipping/pallet.yaml` exists but fails to parse
- **THEN** the response is `412`
- **AND** that file's content is untouched

#### Scenario: A create does not clobber a file that appears at its own filename

- **WHEN** a conditional create passes its pre-write check for `pallet`, and another writer creates
  the destination file before the request publishes
- **THEN** the response is `412`
- **AND** the destination holds the other writer's content, not the request's

#### Scenario: A create refused before publishing leaves no new group behind

- **WHEN** `PUT /api/templates/pallet?group=Shipping/Pallets` with `If-None-Match: *` creates both
  directories and is then refused `412` because the id is taken
- **THEN** neither directory remains
- **AND** `GET /api/template-groups` lists exactly the groups it listed before the request

#### Scenario: A create that published keeps its group even when the confirmation fails

- **WHEN** a create publishes its file into a directory it created and the post-write confirmation
  then fails with `409`
- **THEN** the caller's file is still at the path it was written to
- **AND** the directory holding it still exists and is still listed as a group

#### Scenario: A refused create does not remove a group that already existed

- **WHEN** the same request is refused and `Shipping` already existed while `Shipping/Pallets` did not
- **THEN** `Shipping/Pallets` is removed
- **AND** `Shipping` remains

#### Scenario: A create loses to a file the registry had not loaded

- **WHEN** a file is copied into the templates tree without a reload and a conditional create is
  submitted for the id it declares
- **THEN** the response is `412`
- **AND** the tree holds exactly the files it held before the request

### Requirement: No path the service writes to crosses a symbolic link

Every path the service creates, writes, moves, or removes under `templates/` SHALL be resolved
component by component, and a component that is a symbolic link SHALL abort the operation with
nothing written. This holds for every component of every path an operation touches: the directories a
create or a move makes on the way to a group, the **source** file a move reads and unlinks, the
destination file itself, the staging file a write publishes from, and the directory a group delete
removes.

Resolving the ancestors is not sufficient on its own: the operation that finally mutates the
filesystem SHALL itself be performed relative to the resolved directory, addressing only a single
name within it. An operation that re-states the whole path as a string undoes the resolution, because
the kernel walks that string again and follows whatever the ancestors point at by then.

Lexical validation of a group path is not sufficient and SHALL NOT be relied on for containment. A
group path of `Outside` is perfectly valid, and `templates/Outside -> /etc` makes it resolve outside
the tree; an operator can plant such a link, and so can anything else with write access to the config
directory. Refusing symlinked components is what keeps the containment guarantee mechanical rather
than lexical.

The check SHALL be made on the path the operation is about to use, not on a snapshot taken earlier,
so a link planted between a check and a write cannot be followed. Resolving each component and
opening it in a way that refuses to follow a link satisfies this; a `canonicalize`-then-compare does
not, because it resolves links rather than refusing them.

The refusal SHALL be reported by what the caller could have done differently. A symbolic link in a
group path the **request supplied** is the caller's to fix, and is `422` with `details.reason`
`template_group_unsafe_path` for the write endpoints and `400` for
`DELETE /api/template-groups/{path}`. A symbolic link the service finds in a path it derived itself —
the backing file of an id, the file a delete unlinks — names nothing the caller wrote, and is `500`
carrying the same reason. Both leave the tree untouched.

The load path SHALL remain unaffected by this requirement: it skips symlinked directories while
walking, a property this change creates rather than inherits, and reading a symlinked *file* is a
read, not a write.

#### Scenario: A create through a symlinked group directory is refused

- **WHEN** `templates/Outside` is a symbolic link to a directory outside the templates tree, and
  `PUT /api/templates/pallet?group=Outside` is called
- **THEN** the response is `422` with `details.reason` `template_group_unsafe_path`
- **AND** no file is written anywhere

#### Scenario: A symlinked source file is refused

- **WHEN** the file the registry serves for an id is itself a symbolic link and a move or a write
  targets it
- **THEN** the response is `500` with `details.reason` `template_group_unsafe_path`, the request
  having named nothing wrong
- **AND** neither the link nor its target is unlinked or modified

#### Scenario: A symlinked destination file is refused

- **WHEN** the destination `templates/Warehouse/pallet.yaml` is itself a symbolic link and a write
  targets it
- **THEN** the response is `500` with `details.reason` `template_group_unsafe_path`
- **AND** the link's target is not modified

#### Scenario: A group delete does not follow a link

- **WHEN** `templates/Outside` is a symbolic link and that group is deleted
- **THEN** the response is `400` with `details.reason` `template_group_unsafe_path`
- **AND** the target directory still exists

### Requirement: The service never writes to the templates tree unasked

The service SHALL NOT rewrite, move, or rename anything under `{LABELER_CONFIG_DIR}/templates/` on
its own, at startup or at any other time, and no command it ships does so either. There is no
migration, automatic or manual.

A file written for the field model carries `id:` and possibly `group:`, which are now unknown keys,
so it is refused and reported as broken like any other invalid template. This requirement states what
the service will *not* do; what anyone does about such a file is outside it.

#### Scenario: The service rewrites nothing by itself

- **WHEN** the service starts against a templates tree full of files carrying legacy keys
- **THEN** every one of them is reported as broken
- **AND** not one file has been moved, renamed, or rewritten
- **AND** the service serves whatever files are already valid
