# Templates

## Purpose

Defines the template file format (top-level keys, `unit`, `dpi`, `format`), the template registry built from the templates directory, groups, the template and group endpoints, the thumbnail and source endpoints, and the template catalog.

## Requirements

### Requirement: Template file keys

A template SHALL be a YAML mapping with exactly these top-level keys. Every mapping in a template, at any depth, SHALL reject a key it does not define, with an error naming the key; a template carrying one is invalid.

| Key | Type | Required | Constraint |
|---|---|---|---|
| `name` | string | yes | not empty after trimming |
| `description` | string | no | reported as `""` when absent |
| `unit` | `mm` \| `in` | yes | the length unit of every coordinate and size in the template |
| `dpi` | integer | yes | > 0; raster resolution of PNG output |
| `format` | mapping | yes | see "Label format" |
| `params` | sequence | no | see `parameters` |
| `layout` | sequence | yes | see `layout` |
| `version` | string | no | free-form |

`id` and `group` SHALL NOT be keys: they come from the file's location.

#### Scenario: An unknown top-level key is refused

- **WHEN** a template declares `options:` or `id:` alongside the keys above
- **THEN** it is invalid, with an error naming that key

### Requirement: Label format

`format` SHALL be tagged by `type`: `single` (one label) or `sheet` (a grid of identical slots on a page). Lengths are in the template `unit`.

| `type` | Key | Type | Required | Constraint |
|---|---|---|---|---|
| `single` | `width` | Dimension | yes | |
| `single` | `height` | Dimension | yes | |
| `single` | `media_width` | number | no | > 0; nominal media width, read only by `printing`, no effect on rendering |
| `sheet` | `paper_width`, `paper_height` | number | yes | > 0 |
| `sheet` | `label_width`, `label_height` | number | yes | > 0; the size of every slot |
| `sheet` | `positions` | sequence of `[x, y]` | yes | not empty; `x`, `y` ≥ 0; each is a slot's bottom-left corner, page origin bottom-left |

A Dimension SHALL be a number, a parameter reference `"{name}"` resolved from that parameter's value (see `parameters`), or a range `{ min, max }` whose bounds are each a number or a parameter reference. A literal fixed value or literal bound SHALL be > 0, and literal `min` SHALL be ≤ literal `max`. A range needs at least one bound, and a range on `height` renders at its `max`, else its `min`; on `width` it needs both, checked after instantiating parameter defaults, with the message `a dynamic-width single template must specify both width.min and width.max`. At load, parameter references in `format` and `layout` geometry SHALL be instantiated with their defaults and the result validated.

#### Scenario: A sheet with no positions is refused

- **WHEN** a `sheet` format declares `positions: []`
- **THEN** the template is invalid with the message `positions must not be empty`

### Requirement: Auto-length labels

A `single` template whose `width` is a range SHALL be auto-length: at render its width SHALL be the layout's measured content width, given `max` as the available width, clamped to `[min, max]`. How text fits within that budget is owned by `text`. A fixed-width `single` and every `sheet` SHALL use their declared size.

#### Scenario: Width follows content within the bounds

- **WHEN** an auto-length template with `width: { min: 10, max: 100 }` renders content 30 units wide, then content 300 units wide
- **THEN** the labels are 30 and 100 units wide

### Requirement: Invalid templates are refused on write and quarantined at load

A template body SHALL be parsed and validated by the same rules whether it comes from a file at load or from an HTTP write. On a write, a body that fails parsing SHALL answer `422 TemplateInvalid` with reason `template_parse_failed`, and one that fails validation `422 TemplateInvalid` with reason `template_validation_failed`, each with a message naming the path inside the template, before anything is written. At load, the file SHALL be quarantined (see "Registry load").

#### Scenario: An invalid PUT body changes nothing

- **WHEN** `PUT /api/templates/pallet` receives a body with a top-level `options:` key
- **THEN** the response is `422` with `error.code` `TemplateInvalid` and reason `template_parse_failed`, naming `options`
- **AND** no file is created or changed

### Requirement: Registry load

At startup and on every reload the service SHALL build the registry from `{LABELER_CONFIG_DIR}/templates/`, creating that directory at startup when missing and seeding nothing into it. It SHALL walk the tree recursively, load files whose extension is `yaml` or `yml` (case-insensitive), ignore every other file, and skip without reporting every directory whose name begins with `.` together with everything beneath it. It SHALL never create, move, rename or rewrite anything while loading. A symbolic link to a file is read; a symbolic link to a directory is not walked.

Files SHALL be processed in ascending byte order of their path relative to the templates directory, so the result does not depend on directory enumeration order. A file SHALL be refused (excluded from the served set and reported as broken) when, checked in this order:

1. its path is not valid UTF-8 (reported with a lossily converted path and a message saying so);
2. any directory on its path fails group-segment validation or is not valid UTF-8 (message names the directory and the rule);
3. its filename stem is not a valid id;
4. it cannot be read;
5. it fails parsing or validation;
6. its id is already held by an earlier file that passed every check above (message names the id and both paths).

A refused file SHALL NOT claim its id, so a later valid file with the same stem is served. Startup SHALL abort only when a directory in the walk cannot be read or enumerated, or an entry's metadata cannot be read.

#### Scenario: A malformed file does not stop the service

- **WHEN** the tree holds one valid template and one file that is not valid template YAML
- **THEN** the service starts, serves the valid template, and reports the other as broken

#### Scenario: The earlier path wins an id

- **WHEN** `Shipping/pallet.yaml` and `Warehouse/pallet.yaml` are both valid
- **THEN** `pallet` is served from `Shipping/pallet.yaml`
- **AND** `Warehouse/pallet.yaml` is reported as broken with a message naming `pallet` and `Shipping/pallet.yaml`

#### Scenario: Extension order decides within a directory

- **WHEN** `Warehouse/pallet.yaml` and `Warehouse/pallet.yml` are both valid
- **THEN** `pallet` is served from `Warehouse/pallet.yaml` and the `.yml` file is reported as broken

#### Scenario: A broken earlier file does not claim the id

- **WHEN** `bad:name/pallet.yaml` (an invalid directory) or an unparseable `Shipping/pallet.yaml` sorts before a valid `Warehouse/pallet.yaml`
- **THEN** `pallet` is served from `Warehouse/pallet.yaml`
- **AND** the earlier file is reported for its own fault, not as a duplicate

#### Scenario: A dot-directory is invisible

- **WHEN** `templates/.attic/old.yaml` or `templates/Warehouse/.old/pallet.yaml` holds a valid template
- **THEN** it is neither served nor reported, and no group is offered for the dot-directory

#### Scenario: An unreadable templates directory is fatal

- **WHEN** the templates directory cannot be read at startup
- **THEN** the service exits with a fatal error naming the directory

### Requirement: Template id and group come from the path

A template's id SHALL be its filename stem: `templates/Shipping/pallet.yaml` is the template `pallet`. An id SHALL be non-empty and match `^[a-zA-Z0-9_-]+$`, and SHALL be unique across the whole tree. A template's group SHALL be the path of its directory relative to `templates/`, segments joined with `/`; a file at the root is ungrouped. Moving a file between groups SHALL NOT change its id.

#### Scenario: The path names the template

- **WHEN** a valid template is stored at `templates/Shipping/Pallets/euro.yml`
- **THEN** `GET /api/templates/euro` returns it with `group` `Shipping/Pallets`

#### Scenario: A filename that cannot be an id is refused

- **WHEN** `templates/my template.yaml` holds an otherwise valid template
- **THEN** it is reported as broken with a message naming the stem and the rule, and every other template is served

### Requirement: Groups are directories

A group SHALL exist exactly while its directory exists under `templates/`, whether or not it holds templates; directories nest to any depth. A group path SHALL be valid when, after trimming surrounding whitespace, it is non-empty, at most 255 characters and 1024 UTF-8 bytes, and every `/`-separated segment:

- is non-empty, at most 64 characters and 255 UTF-8 bytes;
- contains no control character and none of `/ \ < > : " | ? *`;
- is not `.` or `..`, has no leading or trailing whitespace, and does not start or end with `.`;
- is not a reserved device name (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`, `COM¹`–`COM³`, `LPT¹`–`LPT³`), compared case-insensitively against the part of the segment before its first `.`.

Group paths SHALL be compared exactly, including case. A request that names a group to write into SHALL fail with `422 TemplateInvalid` and reason `template_group_invalid` when the path is invalid, before anything is created.

#### Scenario: Reserved names are refused with or without an extension

- **WHEN** a group segment is `CON`, `nul.yaml` or `COM¹`
- **THEN** the request fails with reason `template_group_invalid`

#### Scenario: A name merely starting with a device name is accepted

- **WHEN** a group segment is `CONSOLE`
- **THEN** it passes validation

#### Scenario: Whitespace around the path is trimmed, around a segment refused

- **WHEN** the group `"  Warehouse  "` is requested
- **THEN** the group is `Warehouse`
- **AND** a request for `Shipping / Pallets` fails with reason `template_group_invalid`

### Requirement: Creating group directories safely

When a write needs a group directory, the service SHALL resolve each segment under its parent without following symbolic links: an entry whose name matches byte for byte and is a directory SHALL be reused; a missing one SHALL be created exclusively. A request-supplied group path SHALL fail with `422` and reason `template_group_unsafe_path` when a segment is an existing symbolic link or non-directory, and with `422` and reason `template_group_case_conflict`, naming the existing group by its stored spelling, when the filesystem reports the name taken by a directory whose spelling differs (a case-folding filesystem). Where the filesystem distinguishes case, a case-variant SHALL be created as a separate group. A directory listing that cannot be read to its end, or another I/O failure, SHALL fail with `500 RenderFailed` and reason `template_registry_io`; nothing is created on the strength of a partial listing.

#### Scenario: A case-only clash on a folding filesystem is refused

- **WHEN** the filesystem folds case, `Shipping/Warehouse` exists, and a create targets `Shipping/warehouse`
- **THEN** the response is `422` with reason `template_group_case_conflict` naming `Shipping/Warehouse`
- **AND** nothing is created or written

#### Scenario: A case-variant is its own group where case is distinguished

- **WHEN** the filesystem distinguishes case, `Warehouse` exists, and a create targets `warehouse`
- **THEN** `templates/warehouse/` is created beside `templates/Warehouse/` and both are listed

#### Scenario: A symlinked group directory is refused

- **WHEN** `templates/Outside` is a symbolic link and a create or move targets the group `Outside`
- **THEN** the response is `422` with reason `template_group_unsafe_path` and nothing is written

### Requirement: Writes never cross a symbolic link

Every file the service writes, moves or removes under `templates/` SHALL be addressed relative to a directory resolved without following symbolic links, and a symbolic link found in a path SHALL abort the operation with nothing changed. A link in a path the request supplied is reported as that endpoint states; a link in a path the service derived itself (an id's backing file, a destination filename) SHALL fail with `500 RenderFailed` and reason `template_group_unsafe_path`.

#### Scenario: A symlinked backing file is not written through

- **WHEN** the file a replace would write is a symbolic link
- **THEN** the response is `500` with reason `template_group_unsafe_path`
- **AND** neither the link nor its target changes

### Requirement: Listing templates

`GET /api/templates` SHALL answer `200 { "templates": [TemplateSummary...], "broken": [{ "path", "error" }...] }`. `templates` SHALL be sorted ascending by id. A summary SHALL carry `id`, `name`, `description`, `unit`, `dpi`, `params`, `format`, and `group` only when the template is grouped. `broken` SHALL list every refused file by its path relative to `templates/` with the refusal message, whatever the filter, and SHALL be omitted when empty.

The optional `group` query parameter SHALL filter, after trimming: absent lists every template; `?group=` (empty) lists the ungrouped ones; `?group=<path>` lists those whose group equals `<path>` exactly. With `nested=true` (default `false`) a non-empty group widens to that group and every group beneath it by whole segments, and an empty group to every template. A group with no matching template SHALL answer `200` with an empty list.

#### Scenario: Nested filtering matches whole segments

- **WHEN** groups `Shipping`, `Shipping/Pallets` and `Shipping2` hold templates and `GET /api/templates?group=Shipping&nested=true` is called
- **THEN** templates of `Shipping` and `Shipping/Pallets` are returned and none of `Shipping2`

#### Scenario: Case is significant

- **WHEN** templates exist in `Warehouse` and `GET /api/templates?group=warehouse` is called
- **THEN** the response is `200` with an empty `templates` list

### Requirement: Template detail and source

`GET /api/templates/{id}` SHALL answer `200` with a TemplateDetail: `id`, `name`, `description`, `group` (omitted when ungrouped), `unit`, `dpi`, `format`, `params`, `layout`, `version` (omitted when absent), `variables`, `inputs` and `param_defaults` (the last two specified by `parameters`), or `404 TemplateNotFound`. `GET /api/templates/{id}/source` SHALL answer `200` with the stored file's bytes as `text/yaml; charset=utf-8`, `400` with reason `template_id_invalid` when the id does not match the id rule, and `404 TemplateNotFound` when the registry holds no such id or its file cannot be read.

#### Scenario: Source returns the file as stored

- **WHEN** `GET /api/templates/pallet/source` is called for a template whose file carries comments
- **THEN** the body is the file's bytes, comments included

### Requirement: Reload

`POST /api/templates/reload` SHALL rebuild the registry from disk and swap it in, answering `200 { "count": <served>, "broken_count": <refused> }`, including when files were refused. When the tree cannot be read it SHALL answer `500 RenderFailed` with reason `template_registry_io` and keep the previously loaded registry. Every refused file SHALL also be logged as a warning.

#### Scenario: Reload reports refused files instead of failing

- **WHEN** a duplicate id appears on disk and reload is called
- **THEN** the response is `200` with `broken_count` counting the refused file
- **AND** `GET /api/templates` lists it under `broken`

#### Scenario: An unreadable tree keeps the live set

- **WHEN** the templates directory cannot be read and reload is called
- **THEN** the response is `500` with reason `template_registry_io` and the previous templates stay served

### Requirement: Create or replace with PUT

`PUT /api/templates/{id}` with a raw YAML body SHALL create the template when the id is free and replace it when held, deciding against a fresh re-read of the tree. Checks run in this order: an `If-None-Match` other than `*` is `400` with reason `unsupported_precondition`; an id not matching the id rule is `400` with reason `template_id_invalid`; an invalid body is `422` (see "Invalid templates are refused on write"); then:

- **Replace** (id held): with `If-None-Match: *` the response SHALL be `412 PreconditionFailed` with nothing written. A `group` query parameter, trimmed, SHALL equal the template's current group (empty meaning ungrouped) or the response is `400` with reason `template_group_mismatch`, whose message names `PUT /api/templates/{id}/group`. The service SHALL write `{id}.yaml` in the directory of the served file, replacing it atomically, and answer `200` with the TemplateDetail.
- **Create** (id free): the file SHALL be written as `{id}.yaml` in the group named by the `group` query parameter (trimmed; absent or empty means the root), creating missing directories. The publish SHALL fail rather than overwrite an existing name. If that name is taken, with `If-None-Match: *` the response SHALL be `412` and the occupant unchanged; without it the request SHALL replace that file, unless it is a symbolic link (`500`, reason `template_group_unsafe_path`). A create SHALL answer `201` with the TemplateDetail.

A request refused before its file is published SHALL remove every directory it created, innermost first, stopping at one no longer empty. Failures to write are `500` with reason `template_write_failed`.

#### Scenario: A PUT to a free id creates in a group

- **WHEN** `PUT /api/templates/pallet?group=Shipping/Pallets` is called with a valid body and no `pallet` exists
- **THEN** the response is `201`
- **AND** the file is at `templates/Shipping/Pallets/pallet.yaml`

#### Scenario: A PUT to a held id replaces in place

- **WHEN** `pallet` is served from `templates/Shipping/pallet.yaml` and `PUT /api/templates/pallet` is called with a valid body
- **THEN** the response is `200` and that file holds the new body

#### Scenario: A replace cannot move a template

- **WHEN** `pallet` is in `Shipping` and `PUT /api/templates/pallet?group=Warehouse` is called
- **THEN** the response is `400` with reason `template_group_mismatch` and nothing is written

#### Scenario: A conditional create refuses a taken id or filename

- **WHEN** `PUT /api/templates/pallet` with `If-None-Match: *` finds `pallet` served, or finds `templates/Shipping/pallet.yaml` present but unparseable when `group=Shipping`
- **THEN** the response is `412` and the existing file is unchanged

#### Scenario: A refused create leaves no new group

- **WHEN** `PUT /api/templates/pallet?group=Shipping/Pallets` with `If-None-Match: *` creates `Pallets` under an existing `Shipping` and is then refused `412`
- **THEN** `Shipping/Pallets` is removed and `Shipping` remains

#### Scenario: An entity-tag precondition is refused

- **WHEN** `PUT /api/templates/pallet` carries `If-None-Match: "abc123"`
- **THEN** the response is `400` with reason `unsupported_precondition` and nothing is written

### Requirement: A write is confirmed against the file it wrote

After writing or moving a file, the service SHALL re-read the tree and answer success only when the id is served from the file it wrote with byte-identical content. Otherwise it SHALL keep the file where it was written and answer:

- `409 TemplateIdCollision` when the id is served from a different file, the written file is among the files refused for that id, and it still holds what the request wrote; `details` SHALL carry `template` (the id) and `files` (relative paths, the serving file first) and no `reason`;
- `500 RenderFailed` with reason `template_missing_after_write` in every other case, such as the written file being removed or replaced by another writer.

A file sharing the id at a later-sorting path does not displace the written file: the write succeeds and that file is reported under `broken`.

#### Scenario: A later-sorting duplicate does not fail the write

- **WHEN** `PUT /api/templates/pallet` writes `Shipping/pallet.yaml` while `Warehouse/pallet.yaml` exists
- **THEN** the response is the normal success with the caller's content
- **AND** `Warehouse/pallet.yaml` is listed under `broken`

#### Scenario: An earlier-sorting file appearing mid-write is a collision

- **WHEN** a file sharing the id appears at an earlier-sorting path before the post-write re-read
- **THEN** the response is `409 TemplateIdCollision` naming both paths
- **AND** the caller's file stays on disk and is listed under `broken`

#### Scenario: A vanished write is not a collision

- **WHEN** the written file is removed before the re-read and no other file holds the id
- **THEN** the response is `500` with reason `template_missing_after_write`

### Requirement: Delete a template

`DELETE /api/templates/{id}` SHALL remove the served file and answer `204`, then drop every user's favorite for that id; recents and job history SHALL be left alone. The directory holding the file SHALL remain. It SHALL answer `400` with reason `template_id_invalid` for an id not matching the id rule and `404 TemplateNotFound` when the re-read tree serves no such id. When another file sharing the id passed every load check and was refused as a duplicate, it SHALL answer `409 TemplateIdCollision` (details as for writes, listing the served file and the duplicates) and remove nothing, prune no favorites. A namesake refused for any other fault SHALL NOT block the delete.

#### Scenario: Delete refuses while two valid files claim the id

- **WHEN** `Shipping/pallet.yaml` and `Warehouse/pallet.yaml` are both valid and `DELETE /api/templates/pallet` is called
- **THEN** the response is `409 TemplateIdCollision` naming `pallet` and both paths
- **AND** both files and the favorites for `pallet` are untouched

#### Scenario: A broken namesake does not block a delete

- **WHEN** `Warehouse/pallet.yaml` is served and `bad:name/pallet.yaml` or `.attic/pallet.yaml` exists
- **THEN** `DELETE /api/templates/pallet` returns `204`

#### Scenario: A delete leaves the group behind

- **WHEN** the last template in `Shipping` is deleted
- **THEN** `Shipping` is still listed by `GET /api/template-groups`

### Requirement: Move a template between groups

`PUT /api/templates/{id}/group` with body `{ "group": "<path>" }` or `{ "group": null }` (the root) SHALL move the template's file, keeping its filename and every byte, into that group, creating missing directories, and answer `200` with the TemplateDetail. The response SHALL be:

| Status | When |
|---|---|
| `400` | the body lacks the `group` key (reason `request_body_invalid`) or `group` is not a string or null; the id fails the id rule (reason `template_id_invalid`) |
| `404` | the re-read tree serves no such id |
| `409 TemplateIdCollision` | the destination directory already holds a file of the same name, checked before moving; or the post-move confirmation finds a collision |
| `422` | `group` is empty or invalid (`template_group_invalid`), case-conflicting or unsafe (see "Creating group directories safely") |
| `500` | the move failed or the confirmation failed otherwise |

Moving a template to the group it is in SHALL answer `200` and change nothing. A move refused before relocating SHALL remove the directories it created; the source directory SHALL remain after a move. The move SHALL never overwrite an existing file.

#### Scenario: Moving a template into a new nested group

- **WHEN** `PUT /api/templates/bin-tag/group` is called with `{ "group": "Shipping/Pallets" }` for `templates/bin-tag.yaml` and neither directory exists
- **THEN** the response is `200` with `group` `Shipping/Pallets`
- **AND** the file is at `templates/Shipping/Pallets/bin-tag.yaml` with its bytes unchanged

#### Scenario: A body omitting the key is rejected

- **WHEN** the body is `{}`
- **THEN** the response is `400` and the template does not move

#### Scenario: An occupied destination is refused

- **WHEN** `pallet.yaml` in `Shipping` is moved to `Warehouse`, which already holds `pallet.yaml`
- **THEN** the response is `409` and both files are unchanged

### Requirement: List groups

`GET /api/template-groups` SHALL answer `200` with a JSON array of every group path, sorted by Unicode code point. It SHALL include empty groups and every intermediate directory, and exclude dot-directories, directories with an invalid name or path, and everything beneath them. A group whose only file is refused SHALL still be listed. An unreadable tree SHALL answer `500` with reason `template_registry_io`.

#### Scenario: Groups are listed in order

- **WHEN** `templates/` holds `Warehouse/`, an empty `Archive/`, `Shipping/Pallets/euro.yaml` and a root-level template
- **THEN** the response is `["Archive", "Shipping", "Shipping/Pallets", "Warehouse"]`

### Requirement: Addressing a group in the path

`PUT` and `DELETE /api/template-groups/{path}` SHALL carry the whole group path. Every `%` in the raw path SHALL introduce two hexadecimal digits, or the response is `400` with reason `path_param_invalid`; the path SHALL then be percent-decoded once (`%2F` is a separator) and validated, an invalid path answering `400` with reason `template_group_invalid`. Every segment SHALL be resolved by an entry whose name matches byte for byte, so a case-variant SHALL answer `404` even where the filesystem would open it.

#### Scenario: A malformed percent sequence is refused

- **WHEN** `DELETE /api/template-groups/%ZZ` is called
- **THEN** the response is `400` and no group is looked up

#### Scenario: A case-mismatched path is not found

- **WHEN** `templates/Warehouse/` exists on a case-folding filesystem and `DELETE /api/template-groups/warehouse` is called
- **THEN** the response is `404` and the directory remains

### Requirement: Rename a group

`PUT /api/template-groups/{path}` with body `{ "name": "<segment>" }` SHALL rename the group's own directory, keeping its parent, and answer `200 { "group": "<new path>" }`. Everything beneath follows; no template id or file byte changes. The response SHALL be:

| Status | When |
|---|---|
| `400` | the path is malformed or invalid (see "Addressing a group in the path"), or the body is not `{ "name": string }` |
| `404` | no such directory |
| `409 Conflict` | an entry of the new name already exists, including an empty directory or a case-folding alias of the old name |
| `422` | `name` fails segment validation, or the new path or any existing subgroup's new path fails whole-path validation (reason `template_group_invalid`); a source segment is a symbolic link or not a directory (reason `template_group_unsafe_path`) |
| `500` | the rename failed, the platform has no no-replace rename, or the re-read after renaming finds a subgroup over a whole-path limit or the new group not listed |

The rename SHALL be a single no-replace operation and SHALL never merge two groups. Renaming a group to its own name SHALL answer `200` and change nothing.

#### Scenario: Descendants follow the renamed group

- **WHEN** `templates/Shipping/Pallets/euro.yaml` exists and `PUT /api/template-groups/Shipping` is called with `{ "name": "Freight" }`
- **THEN** the response is `200 { "group": "Freight" }`
- **AND** `Freight/Pallets` holds `euro.yaml`, and neither `Shipping` nor `Shipping/Pallets` is listed

#### Scenario: A nested rename keeps the parent

- **WHEN** `PUT /api/template-groups/Shipping/Pallets` is called with `{ "name": "Euro" }`
- **THEN** the response is `200 { "group": "Shipping/Euro" }`

#### Scenario: An occupied name is refused

- **WHEN** `Shipping` and an empty `Warehouse` exist and `Shipping` is renamed to `Warehouse`
- **THEN** the response is `409` and both directories are unchanged

#### Scenario: Recasing works where case is distinguished

- **WHEN** `templates/shipping/` exists on a case-sensitive filesystem and is renamed to `Shipping`
- **THEN** the response is `200 { "group": "Shipping" }`

#### Scenario: A name with a slash is refused

- **WHEN** the body is `{ "name": "Warehouse/Pallets" }`
- **THEN** the response is `422` with reason `template_group_invalid` and nothing is renamed

### Requirement: Delete an empty group

`DELETE /api/template-groups/{path}` SHALL remove the group's directory only when it holds no entry at all, answering `204`. A directory holding anything SHALL answer `409 Conflict` with the message `group '<path>' is not empty`. A symbolic link or non-directory on the path SHALL answer `400` with reason `template_group_unsafe_path`; a missing directory `404`; another failure `500`. The route SHALL never delete recursively.

#### Scenario: A group holding only a subgroup is refused

- **WHEN** `templates/Shipping/` holds only an empty `Pallets/` and `Shipping` is deleted
- **THEN** the response is `409` and both directories remain

### Requirement: Thumbnail

`GET /api/templates/{id}/thumbnail` SHALL render one label to PNG at the template's `dpi` (a `sheet` renders one slot at `label_width` × `label_height`), resolving variables from the store and parameter defaults as a render does. Each required parameter that appears in an interpolated string SHALL receive a placeholder; every other parameter takes its default:

| Input control | Placeholder |
|---|---|
| text, textarea | the parameter name |
| list | a one-element list holding the parameter name |
| integer, number | its `min`, else `1` |
| checkbox | `false` |
| date, datetime | the current local time as `YYYY-MM-DDTHH:MM:SS` |
| select | its first value, or nothing when its default failed to resolve |
| image | a built-in sample PNG |

The response SHALL carry `Content-Type: image/png`, `Cache-Control: no-cache` and an `ETag` that is the quoted lowercase hex SHA-256 of the PNG bytes. A request whose `If-None-Match` equals that ETag or is `*` SHALL answer `304` with the ETag and no body. An unknown id SHALL answer `404 TemplateNotFound`; a render failure fails as the render does (see `rendering`).

#### Scenario: A matching ETag revalidates

- **WHEN** the thumbnail is fetched and then fetched again with `If-None-Match` set to the returned `ETag`, nothing having changed
- **THEN** the second response is `304`

#### Scenario: A sheet thumbnail is one label

- **WHEN** the thumbnail of a `sheet` template is requested
- **THEN** the PNG is the size of one slot, not the page

### Requirement: Template catalog index

`catalog/` in the repository SHALL hold installable templates at `catalog/<category>/<file>.yaml` or `catalog/<category>/<vendor>/<file>.yaml`. `cargo run --bin catalog-index` SHALL parse and validate each with the server's rules, failing on an invalid template or any other path shape, and write `catalog/index.json`: a JSON array, in ascending path order, of entries `{ id, name, description, path, category, vendor, format, media_width_mm, fields }`, where `id` is the filename stem, `path` is relative to `catalog/`, `vendor` is `null` for the two-level shape, `format` is `single` or `sheet`, `media_width_mm` is `format.media_width` or `null`, and `fields` lists the names of the template's required inputs. CI SHALL fail when the committed index differs from the regenerated one.

#### Scenario: A stale index fails CI

- **WHEN** a catalog template changes and `catalog/index.json` is not regenerated
- **THEN** the CI catalog step fails

### Requirement: Catalog page

The UI catalog page SHALL fetch `index.json` and each template's YAML from the repository's raw-content URL in the browser; the server SHALL make no outbound request for the catalog. Entries SHALL be grouped under `category · vendor` (or `category`), headings sorted, each card showing name, description, format, media width in mm when present, and fields (`none` when empty), marked `installed` when the id is served. **Install** SHALL send `PUT /api/templates/{id}` with `If-None-Match: *`. A `412` SHALL open a dialog showing the installed source beside the catalog YAML, offering **Replace** (an unconditional `PUT`) or **Cancel**; a `422` SHALL report that the template needs a newer version of labeler. When the catalog cannot be fetched, the page SHALL say so and link to pasting YAML at `/templates/new`.

#### Scenario: Installing an already installed template offers a diff

- **WHEN** the user installs an entry whose id is already served
- **THEN** the request carries `If-None-Match: *` and answers `412`
- **AND** the page shows both versions and replaces only when the user confirms

#### Scenario: An unreachable catalog falls back to paste

- **WHEN** the browser cannot fetch `index.json`
- **THEN** the page says the catalog could not be reached and links to `/templates/new`

### Requirement: Labels view groups

The Labels view SHALL show a group filter of `All`, one button per group (the union of `GET /api/template-groups` and the groups of served templates, labelled by full path, sorted by code point, identified by path so a group named `All` or `Ungrouped` filters to itself), and `Ungrouped` only while an ungrouped template exists. Choosing a group SHALL narrow the grid to it; an **Include nested** checkbox, off by default and enabled only for a real group, SHALL widen it by whole segments. The filter SHALL compose with the search box, which matches id or name case-insensitively. While a group or a search is active the Favorites and Recents rows SHALL be hidden; an empty result SHALL say `No templates match your search.` or `No templates in this group.`.

Each card SHALL show its group and offer **Move to…**; selecting several cards SHALL offer moving them together, reporting which moves failed. The move dialog SHALL offer existing groups, accept a new path (creating the group), and offer moving to ungrouped, reporting server refusals. For the selected group the view SHALL offer **Rename group**, taking a single name, and **Delete group** only when it holds no template and no subgroup. After a rename the filter SHALL follow the group (or its renamed ancestor) and keep showing the same templates, rendering the pre-rename list until refreshed data carries the new paths; a failed refresh SHALL be reported with a **Retry refresh** action. Moves, renames and deletes SHALL update the view without a manual reload.

#### Scenario: The filter follows a renamed ancestor

- **WHEN** `Shipping/Pallets` is selected and the user renames `Shipping` to `Freight`
- **THEN** the selected filter becomes `Freight/Pallets` and the grid shows the same templates
- **AND** no intermediate render shows an empty grid

#### Scenario: A group filter hides Favorites and Recents

- **WHEN** the user chooses `Warehouse` while favorites exist
- **THEN** the Favorites and Recents rows are hidden, and choosing `All` with an empty search shows them again
