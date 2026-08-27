## ADDED Requirements

### Requirement: A group is renamed in place

`PUT /api/template-groups/{path}` SHALL rename one group's directory. The path segment carries the
whole group path of the group being renamed, so `PUT /api/template-groups/Shipping/Pallets` renames
`Shipping/Pallets`. The request body is `{ "name": "<segment>" }`.

The body SHALL carry the `name` key, and its value SHALL be one path segment, not a path. A rename
changes the group's own name and never its parent: `Shipping/Pallets` may become `Shipping/Euro`, and
cannot become `Warehouse/Pallets`. `/` is already forbidden inside a segment, so a `name` carrying one
fails ordinary segment validation; there is no separate rule and no route by which a group changes
parent. A body omitting the key, or carrying a non-string, is a `400`.

**Addressing the group.** The path SHALL be percent-decoded once and then validated as a group path,
and a malformed percent sequence SHALL be rejected with `400` before decoding, both exactly as
`DELETE /api/template-groups/{path}` specifies. The service SHALL resolve every component of the path
by exact entry name, and a segment matching no entry byte for byte is a `404` even where the
filesystem would have opened a case-variant. Renaming is not the place to discover that `warehouse`
and `Warehouse` are one directory here: the caller named an exact group, and renaming a differently
spelled one would rename a group nobody asked to rename.

Resolution SHALL follow the no-symlink rule of the `template-registry` capability. This route mutates
the templates tree and is therefore a write endpoint under that requirement: a request-supplied path
whose component is a symbolic link SHALL be `422` with `details.reason`
`template_group_unsafe_path`. A component that exists but is not a directory SHALL have the same
status and reason, with a message saying that it is not a directory rather than calling it a symbolic
link. A component that does not exist is `404`.

**Preconditions, all checked before anything is renamed.** The service SHALL refuse, with nothing
renamed and nothing created:

- a `name` failing group-segment validation, with `422` and `details.reason`
  `template_group_invalid`;
- a rename whose result would exceed the whole-path limits in the source-subtree snapshot on which
  the request acts, with `422` and `details.reason` `template_group_invalid`. Segment validation is
  not sufficient: the group path limits of 255 characters and 1024 UTF-8 bytes apply to a whole
  path, so lengthening one ancestor segment can push the renamed group, or any group beneath it,
  past them. The service SHALL therefore walk the source subtree, compute the post-rename path of
  the renamed group and of every discoverable group in that snapshot, and SHALL refuse when any of
  them fails whole-path validation.

Like every other guarantee the `template-registry` capability makes about the tree, this precondition
is bounded by the snapshot the request acted on. The service serializes its own writes but cannot
exclude an operator or another process writing the templates directory. After a successful rename,
the service SHALL walk the renamed subtree again before answering. If a directory that raced the
precondition is present in that post-mutation snapshot and its resulting group path exceeds a
whole-path limit, the response SHALL be `500`; the rename remains performed and SHALL NOT be rolled
back. A directory created after that audit is outside the response's guarantee and surfaces through
the ordinary load-path behavior on a later reload. The service SHALL NOT claim synchronization with
external filesystem writers.

**Performing the rename.** The rename SHALL be a single no-replace rename of the directory: an
operation that fails when an entry of the destination name already exists, rather than one that
checks first and renames second. A check followed by an ordinary rename is not equivalent, because an
ordinary rename replaces an empty destination directory, and the interval between the two is a window
in which a destination can appear. Where the destination exists, the response SHALL be `409` with
nothing renamed and both groups intact. A rename SHALL NOT merge two groups: merging stays what it is
today, moving templates one at a time.

Where the platform offers no no-replace rename, the service SHALL refuse the request rather than fall
back to an ordinary rename, and SHALL NOT report a rename it did not perform.

**Recasing never switches to an ordinary replacing rename.** Device and inode identify an underlying
filesystem object, not a unique directory entry: two sibling bind-mount entries can report the same
pair. The supported platforms expose no portable directory-entry identity that can be compared and
then coupled atomically to a later ordinary rename. A listing or metadata check followed by ordinary
rename would therefore be both an unsound identity test and a check-then-destructive-operation race.

For byte-different source and destination names the service SHALL always issue the same single
no-replace rename described above, with no preliminary identity check authorizing a replacing
operation. This permits `shipping` to `Shipping` on a case-sensitive filesystem, including the Linux
deployment filesystem, because `Shipping` is a free destination name. If a case-folding filesystem
aliases the two spellings and reports the destination as existing, the response SHALL be `409` and
nothing SHALL be renamed; safe recasing is unsupported on that filesystem. If a filesystem can
perform the no-replace call and the post-rename confirmation observes the requested spelling, the
ordinary `200` applies. A case-sensitive filesystem on which `Shipping` already exists as a distinct
sibling likewise returns `409`. The service SHALL NOT use an ordinary rename for any of these cases.

**What moves.** No template file is moved within the directory, no template's bytes are read or
rewritten, and no template id changes, so favorites and job history keep pointing at the same
templates. Everything the directory holds follows it: its templates, its subgroups and their
templates at any depth, and any file the registry refused, which has no group of its own to update
because its group is the directory holding it. Renaming `Shipping` to `Freight` therefore turns the
group `Shipping/Pallets` into `Freight/Pallets`.

The operation SHALL be idempotent: renaming a group to the name it already has, byte for byte, SHALL
return `200` and rename nothing.

Responses:

| Status | Meaning |
| --- | --- |
| `200` | Renamed. Body is `{ "group": "<new path>" }`. |
| `400` | The encoded path is malformed, the decoded path fails validation, or the body is not `{ "name": string }`. |
| `404` | No such directory under `templates/`. |
| `409` | The destination name is occupied, including when a folding filesystem aliases a case-only destination and cannot safely recase it. |
| `422` | The new name fails segment validation; the pre-rename snapshot would put the renamed group or a group below it over a whole-path limit; or the request-supplied source path crosses a symbolic link or non-directory component. |
| `500` | The rename failed, the platform offers no no-replace rename, the tree could not be re-read afterwards, or the post-rename snapshot contains a raced descendant over a whole-path limit. |

Before resolving the path, the service SHALL re-read the templates tree, so the directory it renames
is chosen against the tree as it is rather than against a registry that may predate it. After the
rename it SHALL perform the post-mutation subtree audit above, re-read the tree, and confirm that the
new path is served as a group and that the old path is no longer a group, on the same terms the
`template-registry` capability sets for every template write. A failed audit or confirmation is
`500`; because the directory rename has already published, it is not undone.

`GET /api/openapi.json` SHALL document this route: its path parameter, its request body as
`{ "name": string }`, and every status in the table above, alongside the error reasons named here.

This requirement supersedes the `docs/SPEC.md` §2 endpoint table and §2.0 to the extent of adding
this route. Every other route in that table is unchanged.

#### Scenario: Renaming a top-level group

- **WHEN** `templates/Warehosue/` holds `bin-tag.yaml` and `PUT /api/template-groups/Warehosue` is
  called with `{ "name": "Warehouse" }`
- **THEN** the response is `200` with `{ "group": "Warehouse" }`
- **AND** the file is at `templates/Warehouse/bin-tag.yaml`
- **AND** `GET /api/template-groups` lists `Warehouse` and not `Warehosue`

#### Scenario: A rename changes the last segment only

- **WHEN** `PUT /api/template-groups/Shipping/Pallets` is called with `{ "name": "Euro" }`
- **THEN** the response is `200` with `{ "group": "Shipping/Euro" }`
- **AND** `Shipping` still exists

#### Scenario: Descendants follow the renamed group

- **WHEN** `templates/Shipping/Pallets/euro.yaml` exists and `Shipping` is renamed to `Freight`
- **THEN** the group `Freight/Pallets` holds `euro.yaml`
- **AND** neither `Shipping` nor `Shipping/Pallets` is listed

#### Scenario: Template ids and favorites are untouched

- **WHEN** a favorited template in `Warehosue` is carried along by a rename to `Warehouse`
- **THEN** its id is unchanged
- **AND** it is still favorited

#### Scenario: A template's bytes are not rewritten

- **WHEN** a group holding a commented template file is renamed
- **THEN** the file's bytes are unchanged, comments included

#### Scenario: A refused file follows the directory

- **WHEN** `templates/Warehosue/` holds a file that fails to parse and the group is renamed to
  `Warehouse`
- **THEN** the response is `200`
- **AND** the file is reported under `broken` at its new path by `GET /api/templates`

#### Scenario: An occupied destination is refused, never merged

- **WHEN** `templates/Shipping/` and `templates/Warehouse/` both exist and
  `PUT /api/template-groups/Shipping` is called with `{ "name": "Warehouse" }`
- **THEN** the response is `409`
- **AND** both directories are unchanged
- **AND** no template has moved

#### Scenario: An empty destination directory is not replaced

- **WHEN** `templates/Shipping/` holds templates, `templates/Warehouse/` exists and is empty, and
  `Shipping` is renamed to `Warehouse`
- **THEN** the response is `409`
- **AND** `templates/Warehouse/` still exists
- **AND** `templates/Shipping/` and its templates are unchanged

#### Scenario: A destination appearing after the request began is still refused

- **WHEN** an entry of the destination name comes into existence between the request being received
  and the rename being issued
- **THEN** the rename fails because the destination exists, rather than replacing it
- **AND** the response is `409`

#### Scenario: Recasing a group is performed

- **WHEN** `templates/shipping/` exists on a case-sensitive Linux filesystem, `Shipping` is free, and
  `PUT /api/template-groups/shipping` is called with `{ "name": "Shipping" }`
- **THEN** the response is `200` with `{ "group": "Shipping" }`
- **AND** `GET /api/template-groups` lists `Shipping` and not `shipping`

#### Scenario: Unsafe recasing on a folding filesystem is refused

- **WHEN** the filesystem folds case, so `Shipping` is reported as existing when `shipping` is
  renamed to `Shipping`, and the no-replace call refuses the occupied destination
- **THEN** the response is `409`
- **AND** no ordinary replacing rename is attempted
- **AND** the group remains named `shipping`

#### Scenario: A case-differing sibling is not the group itself

- **WHEN** the filesystem distinguishes case, `templates/shipping/` and `templates/Shipping/` both
  exist as two groups, and `shipping` is renamed to `Shipping`
- **THEN** the no-replace rename refuses the occupied destination with `409`
- **AND** both directories and their templates are unchanged

#### Scenario: A rename whose result exceeds the whole-path limits is refused

- **WHEN** renaming a group would make its own path longer than 255 characters or 1024 UTF-8 bytes
- **THEN** the response is `422` with `details.reason` `template_group_invalid`
- **AND** nothing is renamed

#### Scenario: A descendant crossing the whole-path limits refuses the rename

- **WHEN** renaming an ancestor would leave the ancestor's own path valid but push a group beneath it
  past the whole-path limits
- **THEN** the response is `422` with `details.reason` `template_group_invalid`
- **AND** nothing is renamed
- **AND** every group below the ancestor is still listed by `GET /api/template-groups`

#### Scenario: A raced descendant crossing a whole-path limit is a post-rename failure

- **WHEN** every descendant in the pre-rename snapshot passes the resulting whole-path limits, the
  rename succeeds, and an external writer adds a descendant before the post-mutation audit whose
  resulting path exceeds a limit
- **THEN** the response is `500`
- **AND** the directory rename remains performed
- **AND** the service does not claim that the post-mutation tree satisfies the precondition

#### Scenario: A new name carrying a slash is refused

- **WHEN** `PUT /api/template-groups/Shipping` is called with `{ "name": "Warehouse/Pallets" }`
- **THEN** the response is `422` with `details.reason` `template_group_invalid`
- **AND** nothing is renamed

#### Scenario: An invalid new name is refused

- **WHEN** a group is renamed to `..`, to a 200-character name, or to `CON`
- **THEN** the response is `422` with `details.reason` `template_group_invalid`
- **AND** nothing is renamed

#### Scenario: A body omitting the key is rejected

- **WHEN** `PUT /api/template-groups/Shipping` is called with the body `{}`
- **THEN** the response is `400`
- **AND** nothing is renamed

#### Scenario: An unknown group is a 404

- **WHEN** the endpoint is called for a directory that does not exist under `templates/`
- **THEN** the response is `404`

#### Scenario: A case-mismatched source is not renamed

- **WHEN** `templates/Warehouse/` exists and `PUT /api/template-groups/warehouse` is called on a
  filesystem that folds case
- **THEN** the response is `404`
- **AND** `templates/Warehouse/` still exists under that name

#### Scenario: A malformed percent sequence is rejected before decoding

- **WHEN** the endpoint is called with `%ZZ` in the path
- **THEN** the response is `400`

#### Scenario: A symlinked group path is refused

- **WHEN** `templates/Outside` is a symbolic link and the endpoint is called for `Outside`
- **THEN** the response is `422` with `details.reason` `template_group_unsafe_path`
- **AND** nothing is renamed

#### Scenario: A path component that is a file, not a directory, is refused as such

- **WHEN** `templates/Shipping` is a regular file and the endpoint is called for `Shipping`
- **THEN** the response is `422` with `details.reason` `template_group_unsafe_path`
- **AND** the message says the component is not a directory, not that it is a symbolic link

#### Scenario: Renaming a group to its own name changes nothing

- **WHEN** `PUT /api/template-groups/Warehouse` is called with `{ "name": "Warehouse" }`
- **THEN** the response is `200` with `{ "group": "Warehouse" }`
- **AND** the directory is unchanged

## MODIFIED Requirements

### Requirement: A group is a directory under the templates directory

A group SHALL be a directory under `{LABELER_CONFIG_DIR}/templates/`. A template's group is the path
of the directory holding its file, relative to `templates/`, with `/` between segments. A template
file at the root of `templates/` is *ungrouped*, and ungrouped is a first-class state, not an error.

Directories nest to any depth, and a nested directory is a nested group: `templates/Shipping/Pallets/`
is the group `Shipping/Pallets`, whose parent is `Shipping`. Unlike the field this replaces, `/` in a
group name therefore carries meaning: it separates one directory from the next.

A group exists exactly as long as its directory exists. A directory holding no template files is a
group like any other: it is listed, and a template can be moved into it. That is a deliberate change
from the field model, where a group existed only while some template named it.

The registry SHALL skip any directory whose name begins with `.`, together with everything beneath
it. Nothing in such a directory is loaded, reported, or offered as a group, which gives an operator
a place to park files inside `templates/` without them becoming templates.

**Group name validation.** A group path the service is asked to create or address SHALL be valid
when, after stripping leading and trailing whitespace from the whole path, it is non-empty, is at
most 255 characters and 1024 bytes encoded as UTF-8, and every `/`-separated segment satisfies all
of:

- non-empty;
- at most 64 characters *and* at most 255 bytes encoded as UTF-8, so a name that passes validation
  cannot fail later against a filesystem's per-component byte limit;
- free of control characters, including tab, carriage return, and line feed;
- free of `/`, `\`, `<`, `>`, `:`, `"`, `|`, `?`, and `*`;
- not `.` and not `..`;
- no leading or trailing whitespace, no leading `.`, and no trailing `.`;
- not one of the reserved device names, which for this rule are `CON`, `PRN`, `AUX`, `NUL`,
  `COM1`–`COM9`, `LPT1`–`LPT9`, and the superscript-digit forms `COM¹`, `COM²`, `COM³`, `LPT¹`,
  `LPT²` and `LPT³`, which Windows reserves as well. They SHALL be compared without regard to case,
  and against the segment with any trailing `.`-suffix removed, because Windows reserves `CON.txt`
  and `NUL.yaml` exactly as it reserves `CON`. `CONSOLE` and `CONS` are unreserved and SHALL be
  accepted; only a device name itself, alone or extension-bearing, is refused.

These rules are what make a group name a directory name that Windows, macOS and Linux all accept. They are stricter than the field
they replace, which allowed any non-control character.

**Case.** Group paths SHALL be compared exactly, including case, so `Warehouse` and `warehouse` are
two groups and `?group=warehouse` does not match `Warehouse`. Exact comparison is correct on every
platform: where the filesystem folds case the two spellings cannot both exist, so no comparison is
ever asked to tell them apart.

Whether a directory whose name differs only by case from an existing sibling can be created SHALL be
answered by the filesystem holding that directory, and SHALL NOT be predicted. Resolution and
creation of each requested segment SHALL distinguish these outcomes:

- The service SHALL first list the parent. If an entry with the requested name byte for byte is
  present, it SHALL resolve that exact entry as a directory without following symbolic links. An
  exact directory is reused, preserving the existing open-exact-group behavior; an exact symbolic link or
  non-directory is `422` with `details.reason` `template_group_unsafe_path` and is not reused.
- If the first listing contains no exact entry, the service SHALL attempt directory creation
  exclusively. A successful creation creates the requested exact directory. Where the filesystem distinguishes case,
  this is how `Warehouse` and `warehouse` become two directories and two groups.
- If that exclusive create reports that the name exists, the service SHALL re-list the parent. If an
  exact entry has appeared, it SHALL apply the same safe-open rule: reuse it only when it is a
  directory, and refuse an exact file or symbolic link as unsafe.
- If no exact entry is present on that re-list, the service SHALL resolve the requested spelling in
  the parent without following symbolic links. If that spelling still resolves while no byte-exact
  name is listed, the filesystem has supplied a non-exact alias: a directory alias is `422` with
  `details.reason` `template_group_case_conflict`, naming the existing group by its stored spelling,
  and is not reused. To select that spelling for the error message only, the service SHALL compare
  each entry from the parent listing by `(st_dev, st_ino)` with the resolved requested spelling; that
  comparison SHALL NOT authorize reuse, rename, or any other mutation. An aliased file or symbolic
  link is `422` with `details.reason` `template_group_unsafe_path` and is not followed. If the
  spelling no longer resolves, the occupant vanished and the service SHALL retry the exclusive create
  once. Success creates and uses the exact directory.
- If the one retry again reports that the name exists, the service SHALL perform one final re-list
  and no-follow classification by the same rules. An exact directory is reused, an exact or aliased
  file or symbolic link is unsafe, and a non-exact directory alias is a case conflict. If the name
  vanishes again before that final classification, the unstable race is `500` with `details.reason`
  `template_registry_io`; there is no third create attempt.
- A failure to list, safely open, or create for any reason other than the bounded exists race SHALL
  be `500` with `details.reason` `template_registry_io`. There is no unbounded retry loop.

The answer is therefore per directory, given by the filesystem holding it. No rule predicts case
behaviour, so none can predict it wrongly: a tree whose subdirectories sit on differently behaving
mounts is handled with each parent answering for itself, and a folding no lowercase mapping
implements is handled without the service knowing which folding the filesystem performs. `Größe` and
`GRÖSSE` are refused where that filesystem aliases them and created where it does not.

The service SHALL NOT create, move, or remove anything under `templates/` in order to determine case
behaviour, at startup or at any other time, per the `template-registry` capability's requirement that
the service never writes to the templates tree unasked. The only write is the requested creation
itself.

**Directories the service did not create.** A directory that exists on disk with a name failing the
validation above SHALL NOT quarantine the whole load. Every template beneath it SHALL be refused and
reported as broken with a message naming the directory and the rule it breaks, and the directory
SHALL NOT be offered as a group. Every template outside it loads normally.

The post-change set of top-level template fields is:

| Field | Type | Notes |
| --- | --- | --- |
| `name` | string | Required, non-empty. |
| `description` | string | Optional. |
| `unit` | `"mm"` \| `"in"` | Length unit for all coordinates/sizes in the template. |
| `dpi` | integer > 0 | Raster resolution for PNG output. |
| `format` | object | See `docs/SPEC.md` §3.1. |
| `params` | map | Optional. Map of parameter name → `ParamSpec`. See `docs/SPEC.md` §3.0. |
| `options` | map | Optional, legacy. Map of option name → allowed values, desugared into an enum `params` entry. Still accepted; not for new templates (ADR-0055, ADR-0056). |
| `layout` | list | Tree of layout items. See `docs/SPEC.md` §4. |
| `version` | string | Optional, free-form. |

`id` and `group` are absent from that table on purpose: both are now carried by the file's location,
and both SHALL be rejected as unknown top-level keys, per the `template-registry` capability. Parsing
still rejects unknown fields.

This requirement supersedes the `docs/SPEC.md` §3 top-level field table, and only that table. Every
other rule in §3, and the frozen §3.0 and §3.1 subsections, stay authoritative.

#### Scenario: A directory is the group

- **WHEN** a valid template file is stored at `templates/Warehouse/bin-tag.yaml`
- **THEN** the template loads
- **AND** its group is `Warehouse`

#### Scenario: A file at the root is ungrouped

- **WHEN** a valid template file is stored at `templates/bin-tag.yaml`
- **THEN** the template loads
- **AND** it is reported as ungrouped, not as broken

#### Scenario: A nested directory is a nested group

- **WHEN** a valid template file is stored at `templates/Shipping/Pallets/euro.yaml`
- **THEN** its group is `Shipping/Pallets`

#### Scenario: An empty directory is still a group

- **WHEN** `templates/Archive/` exists and holds no template file
- **THEN** `Archive` is listed as a group
- **AND** a template can be moved into it

#### Scenario: A dot-directory is invisible

- **WHEN** `templates/.attic/old.yaml` holds a valid template
- **THEN** it is not served, not reported as broken, and `.attic` is not a group

#### Scenario: A directory name that cannot be a group name refuses only its own templates

- **WHEN** `templates/bad:name/x.yaml` holds a valid template and `templates/Warehouse/y.yaml` holds
  another
- **THEN** `x.yaml` is reported as broken with a message naming `bad:name`
- **AND** `bad:name` is not offered as a group
- **AND** `y.yaml` is served normally

#### Scenario: An exact existing directory is reused

- **WHEN** the parent listing finds an exact entry for a requested segment and that entry opens as a
  directory without following a symbolic link
- **THEN** the request reuses that directory
- **AND** it does not attempt to create that segment

#### Scenario: An exact directory created during resolution is reused

- **WHEN** the first parent listing finds no exact entry, exclusive creation reports that the name
  exists, and the re-list finds an exact directory created by another writer
- **THEN** the request safely opens and reuses that exact directory
- **AND** the response is not `template_group_case_conflict`

#### Scenario: An occupant that vanishes before the re-list permits one retry

- **WHEN** exclusive creation reports that the name exists, the re-list contains no exact entry, and
  the requested spelling no longer resolves without following symbolic links
- **THEN** the service repeats the exclusive create once
- **AND** when that repeated create succeeds, the newly created exact directory is used
- **AND** no further create retry is attempted

#### Scenario: A non-exact alias is refused without reuse

- **WHEN** exclusive creation reports that the name exists, the re-list contains no exact entry, and
  the requested spelling still resolves to a directory under the filesystem's name rules
- **THEN** the response is `422` with `details.reason` `template_group_case_conflict`
- **AND** the message names the existing group by its stored spelling
- **AND** the differently spelled directory is not reused

#### Scenario: A repeatedly vanishing occupant is an I/O failure

- **WHEN** the one repeated exclusive create again reports that the name exists and the requested
  spelling no longer resolves at the final classification
- **THEN** the response is `500` with `details.reason` `template_registry_io`
- **AND** no third create is attempted

#### Scenario: A file or symlink is refused as unsafe

- **WHEN** an exact entry, or a non-exact entry to which the requested spelling resolves after an
  exists result, is a regular file or symbolic link
- **THEN** the response is `422` with `details.reason` `template_group_unsafe_path`
- **AND** the entry is not opened as a group or written through

#### Scenario: A directory-resolution I/O failure is not a case conflict

- **WHEN** a parent re-list, safe open, or exclusive create fails with an I/O error other than the
  bounded `EEXIST` race
- **THEN** the response is `500` with `details.reason` `template_registry_io`
- **AND** the failure is not reported as `template_group_case_conflict`

#### Scenario: A case-only clash is refused rather than merged

- **WHEN** the filesystem folds case, the group `Shipping/Warehouse` exists, and the service is
  asked to create `Shipping/warehouse`
- **THEN** the exclusive creation fails because the name already exists
- **AND** the response is `422` with `details.reason` `template_group_case_conflict`
- **AND** the message names the existing group `Shipping/Warehouse`
- **AND** no directory is created
- **AND** `Shipping/Warehouse` is not opened or written into

#### Scenario: An ASCII case clash is caught

- **WHEN** the filesystem folds case, the group `Warehouse` exists, and the service is asked to
  create `WAREHOUSE`
- **THEN** the response is `422` with `details.reason` `template_group_case_conflict`
- **AND** the message names the existing group `Warehouse`

#### Scenario: A case-differing sibling is created where the filesystem distinguishes case

- **WHEN** the filesystem distinguishes case, the group `Warehouse` exists, and the service is asked
  to create `warehouse`
- **THEN** the exclusive creation succeeds
- **AND** `templates/warehouse/` exists alongside `templates/Warehouse/`
- **AND** both are listed as groups

#### Scenario: Determining case behaviour writes nothing

- **WHEN** the service starts against a templates tree, and when it refuses a create on the
  case-conflict rule
- **THEN** it has created, moved and removed nothing under `templates/` other than a requested
  creation that succeeded
- **AND** `GET /api/template-groups` lists no entry the operator did not create

#### Scenario: A folding the rule does not cover is not claimed to be caught

- **WHEN** the group `Größe` exists and the service is asked to create `GRÖSSE`
- **THEN** the service refuses the request on no predicted-equality check of its own
- **AND** the outcome is whatever the filesystem gives: `422 template_group_case_conflict` naming
  the existing group `Größe` where it aliases the two names, and a created group where it does not

#### Scenario: A filesystem alias is refused, never silently reused

- **WHEN** exclusive creation reports that the name exists, the following parent listing contains
  no byte-exact entry, and the requested spelling still resolves to a directory
- **THEN** the response is `422` with `details.reason` `template_group_case_conflict`
- **AND** the message names the existing group by its stored spelling
- **AND** the existing directory is not reused for the request
- **AND** nothing is written into it

#### Scenario: Case is significant between existing groups

- **WHEN** `templates/Warehouse/` and `templates/warehouse/` both exist on a case-sensitive
  filesystem
- **THEN** they are two groups
- **AND** `?group=warehouse` returns only the second one's templates

#### Scenario: A superscript device name is refused

- **WHEN** the service is asked to create a group whose segment is `COM¹`
- **THEN** the response is `422` with `details.reason` `template_group_invalid`

#### Scenario: A reserved device name with an extension is refused

- **WHEN** the service is asked to create a group whose segment is `NUL.yaml`
- **THEN** the response is `422` with `details.reason` `template_group_invalid`

#### Scenario: A name merely beginning with a device name is accepted

- **WHEN** the service is asked to create a group whose segment is `CONSOLE`
- **THEN** the request is not refused on the reserved-name rule

#### Scenario: An invalid group name is refused

- **WHEN** the service is asked to create a group whose segment is `..`, or is 200 characters long,
  or contains a control character, or is `CON`
- **THEN** the response is `422` with `details.reason` `template_group_invalid`
- **AND** no directory is created

#### Scenario: Surrounding whitespace is not part of the path

- **WHEN** the service is asked to create the group `"  Warehouse  "`
- **THEN** the group is `Warehouse`

#### Scenario: Whitespace around a segment is a failure

- **WHEN** the service is asked to create the group `Shipping / Pallets`
- **THEN** the response is `422` with `details.reason` `template_group_invalid`

### Requirement: The Labels view browses and edits groups

The Labels view SHALL let a user browse by group and move templates between groups without editing
YAML or touching the filesystem.

It SHALL show the groups as a tree: `All`, every group the service lists, nested to match the
directory structure, and `Ungrouped`. `All` and `Ungrouped` are synthetic entries, not groups, and
both strings are legal directory names, so the view SHALL keep them distinguishable from a real group
of the same name: the synthetic entries SHALL be presented apart from the group tree, and every node
inside the tree SHALL be identified by its group path rather than by its displayed label, so choosing
a real group named `All` filters to that group and not to everything. Sibling nodes SHALL be ordered by ascending Unicode code point,
which is the ordering the API already applies to template ids, so `Warehouse` sorts before
`warehouse`. `Ungrouped` SHALL be offered only while at least one ungrouped template exists. An empty
group SHALL appear in the tree like any other, since a template can be moved into it.

Choosing a group SHALL narrow the grid to that group alone, and SHALL compose with the existing text
search so that both constraints apply together. The view SHALL offer an **include nested** switch
which widens the current group to that group and everything beneath it; it SHALL be off by default,
and SHALL have no effect while `All` or `Ungrouped` is chosen.

While a group other than `All` is chosen, the view SHALL hide the Favorites and Recents rows, exactly
as an active search already does, so that everything on screen belongs to the chosen group. When the
choice is `All` and the search box is empty, both rows return.

When a group and a search term together match nothing, the view SHALL say so rather than render an
empty grid.

Each template card SHALL show its group and SHALL offer a **Move to…** action. The move dialog SHALL
offer the existing groups as a tree, SHALL accept a path that is not yet in use, which creates that
group by the act of moving a template into it, and SHALL offer a way to make the template ungrouped.
It SHALL report a rejected group path, a case clash and an occupied destination as the errors they
are, rather than failing silently.

The view SHALL offer deleting a group, SHALL enable that only for a group holding no templates and no
subgroup, and SHALL report the `409` from a group that turns out not to be empty.

The view SHALL offer renaming the currently selected real group alongside the group filter controls.
The rename action SHALL be reachable from the group filter control for that selected group. The
rename SHALL change that group's own name only, never its parent, so the control SHALL take a name
rather than a path. It SHALL report a rejected name, a name the parent already holds, and a group
that has gone missing as the errors they are, rather than failing silently.

When the renamed group is the chosen filter, or an ancestor of it, the filter SHALL follow the rename
and keep showing the same templates rather than emptying out or falling back to `All`. On API
success, the view SHALL keep rendering its pre-rename template snapshot while it refreshes the
template and group queries. Only after refreshed template data carries the new group paths SHALL it
replace the selected path by whole segments and release that snapshot. The view SHALL therefore
never render the refreshed selection against stale pre-rename template group strings, or the old
selection against refreshed post-rename strings. If either refresh fails, the view SHALL report the
failure, continue rendering the captured pre-rename template snapshot, and retain the old selected
path. The user SHALL be able to retry both refreshes without repeating the rename; after they succeed
and refreshed template data carries the new group paths, the view SHALL replace the selected path
and release the snapshot as above.

The view SHALL support selecting several templates and moving them in one action, reporting per
template which moves succeeded when some fail.

After a successful move, delete or rename the view SHALL reflect the new tree without a manual
reload.

Favorites and Recents SHALL otherwise keep working exactly as they do today: they are per-user
shortcuts over the whole set, keyed by an id a move does not change, so a move SHALL NOT change
either, and neither is reordered or pruned by grouping.

#### Scenario: Filtering the grid to a group

- **WHEN** the user chooses the `Warehouse` node
- **THEN** only Warehouse templates are shown in the grid

#### Scenario: A real group named All is not the All entry

- **WHEN** a group named `All` exists
- **THEN** it appears inside the group tree, distinct from the synthetic `All` entry
- **AND** choosing it filters the grid to that group's templates
- **AND** choosing the synthetic `All` entry still shows every template

#### Scenario: A real group named Ungrouped is not the Ungrouped entry

- **WHEN** a group named `Ungrouped` exists and ungrouped templates also exist
- **THEN** both the group and the synthetic entry are offered, distinguishably
- **AND** choosing the group shows only that directory's templates

#### Scenario: The tree shows nesting

- **WHEN** the groups are `Shipping` and `Shipping/Pallets`
- **THEN** `Pallets` is shown nested under `Shipping`

#### Scenario: Include nested widens the grid

- **WHEN** the user chooses `Shipping` and turns on **include nested**
- **THEN** templates of `Shipping/Pallets` appear in the grid alongside Shipping's own

#### Scenario: Group filter and search compose

- **WHEN** the user chooses `Warehouse` and types `bin` in the search box
- **THEN** only Warehouse templates matching `bin` are shown

#### Scenario: An empty group is selectable

- **WHEN** `Archive` holds no templates
- **THEN** it appears in the tree
- **AND** choosing it shows an empty grid rather than hiding the group

#### Scenario: Ungrouped is offered only when it is non-empty

- **WHEN** every template belongs to a group
- **THEN** the tree offers no `Ungrouped` entry

#### Scenario: Moving one template from the grid

- **WHEN** the user picks **Move to…** on a card and chooses `Warehouse`
- **THEN** the card shows `Warehouse` without a manual page reload

#### Scenario: Creating a group by naming it

- **WHEN** the user types a group path no template uses and confirms the move
- **THEN** the template is moved into that group
- **AND** the new group appears in the tree

#### Scenario: A rejected group name is reported

- **WHEN** the user types a group path that fails validation
- **THEN** the view reports why, and the template is not moved

#### Scenario: Deleting an empty group from the view

- **WHEN** the user deletes a group holding no templates and no subgroup
- **THEN** the group disappears from the tree without a manual reload

#### Scenario: Deleting a non-empty group is refused

- **WHEN** the user attempts to delete a group that still holds a template
- **THEN** the view reports that the group is not empty
- **AND** the group and its templates remain

#### Scenario: Moving several templates at once

- **WHEN** the user selects three templates and moves them to `Shipping`
- **THEN** all three are moved
- **AND** a failure on one of them is reported without hiding the successes of the others

#### Scenario: A group filter hides the Favorites and Recents rows

- **WHEN** the user chooses the `Warehouse` node while favorites exist
- **THEN** the Favorites and Recents rows are not shown
- **AND** choosing `All` again brings both back

#### Scenario: A group and a search that match nothing say so

- **WHEN** the user chooses `Warehouse` and types a term no Warehouse template matches
- **THEN** the view reports that nothing matches

#### Scenario: A move leaves favorites alone

- **WHEN** a favorited template is moved to another group
- **THEN** it is still favorited
- **AND** it appears in the Favorites row again once the filter is cleared

#### Scenario: Renaming the selected group from the filter controls

- **WHEN** the user selects the full-path filter button for `Warehosue` and uses its adjacent rename
  action to rename it to `Warehouse`
- **THEN** the filter control is shown as `Warehouse` without a manual page reload
- **AND** the same templates are under it

#### Scenario: The filter follows the rename

- **WHEN** `Warehosue` is the chosen filter and the user renames it to `Warehouse`
- **THEN** the grid still shows that group's templates
- **AND** the chosen filter is the renamed group, not `All`
- **AND** no intermediate render shows an empty grid because cached group paths and the filter path
  disagree

#### Scenario: The filter follows a renamed ancestor

- **WHEN** `Shipping/Pallets` is the chosen filter and the user renames `Shipping` to `Freight`
- **THEN** the chosen filter is `Freight/Pallets`
- **AND** the grid still shows the same templates
- **AND** the old selection is retained with the pre-rename template snapshot until refreshed data
  carries `Freight/Pallets`

#### Scenario: Renaming onto a name the parent already holds is reported

- **WHEN** `Shipping` and `Warehouse` both exist and the user renames `Shipping` to `Warehouse`
- **THEN** the view reports that the name is taken
- **AND** both groups are unchanged

#### Scenario: A rejected new name is reported

- **WHEN** the user renames a group to a name that fails validation
- **THEN** the view reports why, and the group is not renamed
