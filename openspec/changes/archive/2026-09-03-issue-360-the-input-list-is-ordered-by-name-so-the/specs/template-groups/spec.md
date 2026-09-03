# Delta: template-groups — params row reflects sequence

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
| `params` | sequence | Optional. Sequence of parameter entries each carrying `name` plus its `ParamSpec` fields. See `docs/SPEC.md` §3.0 as superseded by `template-inputs`. |
| `layout` | list | Tree of layout items. See `docs/SPEC.md` §4. |
| `version` | string | Optional, free-form. |

`id` and `group` are absent from that table on purpose: both are now carried by the file's location,
and both SHALL be rejected as unknown top-level keys, per the `template-registry` capability. Parsing
still rejects unknown fields.

`options` is absent for a different reason: the field is deleted, and the table is exhaustive against
it. `params` is the only way to declare a typed input, including an `enum`. A template carrying a
top-level `options:` key SHALL be refused at load as an unknown top-level key, in an error naming
`options`, and SHALL NOT be desugared into a `params` entry, accepted with a warning, or accepted at
all. The refusal is the ordinary template-content fault: the file is quarantined and reported through
the paths the `template-registry` capability specifies, and the server still starts and still serves
every other template. There is no alias and no deprecation window.

The same refusal SHALL apply to a template submitted over HTTP, because the body of a write is parsed
on the same terms a file on disk is. A `PUT /api/templates/{id}` whose YAML body carries a top-level
`options:` key SHALL be rejected with `422`, `error.code` `TemplateInvalid` and
`error.details.reason` `template_parse_failed`, in a message naming `options`. The rejection SHALL be
decided before anything is written, as the `template-registry` capability's requirement that a `422`
from a template write means nothing was written already demands: replacing an existing template
leaves its stored file byte-for-byte unchanged, and a create-only write (`If-None-Match: *`) creates
no file.

This requirement supersedes the `docs/SPEC.md` §3 top-level field table, and only that table. Every
other rule in §3 stays authoritative. The frozen §3.0 subsections are partitioned and stay
authoritative as follows: its opening declaration/container example is governed by `template-inputs:
Template params are declared as a sequence and published as an array`, its per-entry/type table by
`datetime-params: A datetime parameter names an instant, not a rendering`, and its "Namespace rules
and reserved names" list by `interpolation-tokens: A bare name is a bare name, and no word is
reserved`; §3.1 stays authoritative.

#### Scenario: A top-level `options:` key is refused

- **WHEN** a template file declares a top-level `options:` map alongside `name`, `unit`, `dpi`,
  `format` and `layout`
- **THEN** the template fails to load and is reported as broken with an error naming `options`
- **AND** no `enum` parameter is created from it, and the server still starts and still serves every
  other template

#### Scenario: A `PUT` body carrying `options:` is rejected before the write

- **WHEN** `PUT /api/templates/{id}` receives a YAML body declaring a top-level `options:` map
- **THEN** the response is `422` with `error.code` `TemplateInvalid`, `error.details.reason`
  `template_parse_failed`, and a message naming `options`
- **AND** an existing template at that id is left byte-for-byte unchanged, and no file is created
  when the write was create-only

#### Scenario: The same choices declared as a parameter load

- **WHEN** the same template instead declares `params: [{ name: orientation, type: enum, values: [...] }]`
- **THEN** the template loads, and `orientation` is a declared `enum` parameter on every path that
  reports one

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

- **WHEN** the filesystem folds case, the group `Warehouse` exists, and the service is asked
  to create `WAREHOUSE`
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
