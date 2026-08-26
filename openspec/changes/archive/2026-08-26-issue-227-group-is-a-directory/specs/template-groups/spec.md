## ADDED Requirements

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
two groups and `?group=warehouse` does not match `Warehouse`. To keep that true on a filesystem that
folds case, the service SHALL refuse to create a directory whose name differs only by case from an
existing sibling, with `422` and `details.reason` `template_group_case_conflict`, naming the existing
group. "Differs only by case" SHALL mean that the two names are equal after Unicode lowercase mapping, and that is the whole of the rule: it is deterministic, it is the same on every platform,
and it is testable.

It SHALL NOT be claimed to agree with any particular filesystem. Case-insensitive filesystems differ
from one another in both folding and normalization, and simple lowercase mapping does not implement
full case folding: `Größe` and `GRÖSSE` lowercase to `größe` and `grösse`, stay unequal, and are
therefore *not* caught by this rule. The rule refuses the clashes it can predict — every ASCII pair
and every single-character case pair — and does not pretend to predict the rest.

**What contains the rest.** A filesystem may alias two names this rule considers distinct, whether by
a folding the lowercase mapping does not perform or by Unicode normalization. The service SHALL NOT
absorb such an alias silently. When it has found no exact existing sibling and creating the directory
nonetheless fails because the name already exists, that is an alias, and the service SHALL refuse the
request with `422` and `details.reason` `template_group_case_conflict`, naming the requested path. It
SHALL NOT open and reuse the existing directory: the caller asked for a group of an exact name, and
serving a differently spelled one would silently merge two groups, which is precisely what this rule
exists to prevent.

The service therefore never depends on the filesystem's case behaviour in either direction: it
predicts what it can, and refuses what it cannot predict rather than guessing.

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

#### Scenario: A case-only clash is refused rather than merged

- **WHEN** the group `Shipping/Warehouse` exists and the service is asked to create
  `Shipping/warehouse`
- **THEN** the response is `422` with `details.reason` `template_group_case_conflict`
- **AND** the message names `Shipping/Warehouse`
- **AND** no directory is created

#### Scenario: An ASCII case clash is caught

- **WHEN** the group `Warehouse` exists and the service is asked to create `WAREHOUSE`
- **THEN** the response is `422` with `details.reason` `template_group_case_conflict`

#### Scenario: A folding the rule does not cover is not claimed to be caught

- **WHEN** the group `Größe` exists and the service is asked to create `GRÖSSE`
- **THEN** the two names are not equal under lowercase mapping
- **AND** the service does not refuse the request on that check

#### Scenario: A filesystem alias is refused, never silently reused

- **WHEN** no existing sibling matches the requested name exactly or under lowercase mapping, and
  creating the directory fails because the filesystem says the name already exists
- **THEN** the response is `422` with `details.reason` `template_group_case_conflict`
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

### Requirement: The group tree is served as its own resource

`GET /api/template-groups` SHALL list every group in the templates tree, as
`{ "groups": [ "<path>", ... ] }`, in ascending Unicode code-point order of the path.

The list SHALL include a group holding no templates, and SHALL include every intermediate directory
on the path to a nested group, whether or not that directory holds templates of its own. It SHALL
exclude every dot-directory and its descendants, and every directory whose name fails group-name
validation, together with everything beneath it.

This resource exists because a group can no longer be derived from the loaded templates: a group
outlives its members, so scanning the served set would omit every empty group.

Broken template files SHALL NOT affect the list: a group whose only template is refused is still a
group.

`GET /api/openapi.json` SHALL describe this route and its response schema.

This requirement supersedes the `docs/SPEC.md` §2 endpoint table and §2.0 to the extent of adding
this route: both enumerate the template surface without it, since a group had no independent
existence to list.

#### Scenario: Groups are listed in order

- **WHEN** `templates/` holds `Warehouse/`, `Shipping/Pallets/` and a root-level template
- **THEN** `GET /api/template-groups` returns `["Shipping", "Shipping/Pallets", "Warehouse"]`

#### Scenario: An intermediate directory holding no templates is still listed

- **WHEN** the only template beneath `Shipping` is at `templates/Shipping/Pallets/euro.yaml`
- **THEN** the list carries both `Shipping` and `Shipping/Pallets`

#### Scenario: An empty group is listed

- **WHEN** `templates/Archive/` holds nothing
- **THEN** the list carries `Archive`

#### Scenario: A group whose only template is broken is still a group

- **WHEN** `templates/Warehouse/` holds one file that fails to parse
- **THEN** the list carries `Warehouse`
- **AND** the file is reported under `broken` by `GET /api/templates`

### Requirement: A template is moved between groups by moving its file

`PUT /api/templates/{id}/group` SHALL move one template's file between directories. The request body
is `{ "group": "<path>" }` to move it into a group, or `{ "group": null }` to move it to the root of
`templates/`, which makes it ungrouped. The body SHALL carry the `group` key: a body omitting it
entirely is malformed and SHALL be rejected with `400`, since treating an absent key as null would
let an empty object silently ungroup a template.

The move SHALL relocate the file from its current path to `{group}/{id}.yaml`, creating any missing
directory on that path. The file's content SHALL NOT be rewritten or reformatted: a move preserves
comments, key order and every other byte by construction, which is what makes the single-key YAML
patch this replaces unnecessary. The service still reads the file afterwards, as the post-move
confirmation below and every registry load require; what it never does is write a byte of it. The template's id SHALL NOT change, so favorites and
job history keep pointing at the same template.

The move SHALL NOT overwrite anything. The service SHALL refuse, with nothing moved and no directory
created, when the destination directory already holds a file whose stem is the id, under either the
`.yaml` or the `.yml` extension.

**Directories the request created are removed only while nothing has been relocated.** A move refused
before it relocates the file SHALL remove every directory it created, innermost first, stopping at
the first that is no longer empty, so such a refusal leaves no group that did not exist before it.
Once the file has been relocated the request has published its result: a later failure keeps the file
where the move put it, per the `template-registry` capability's rule that no endpoint undoes its own
write, so the directory holding it is neither empty nor removed. The two rules cannot both apply to
one directory.

The move SHALL resolve the destination path under the no-symlink rule of the `template-registry`
capability: a group path any of whose components is a symbolic link SHALL be refused with `422` and
`details.reason` `template_group_unsafe_path`, and nothing SHALL be created or moved. Lexical
validation alone does not contain the write: an operator can plant
`templates/Outside -> /somewhere/else`, after which the perfectly valid group path `Outside` would
otherwise resolve outside the templates tree.

Responses:

| Status | Meaning |
| --- | --- |
| `200` | Moved. Body is the updated `TemplateDetail`. |
| `400` | The path id is invalid, or the body is not `{ "group": string \| null }`. |
| `404` | The registry holds no template with that id. |
| `409` | The destination already holds a file for that id, or, after the move, the id is served from a different file. |
| `422` | The group path fails validation, clashes with an existing sibling only by case, or crosses a symbolic link. |
| `500` | The move failed, the directory could not be re-read afterwards, or the moved file is missing afterwards. |

The operation SHALL be idempotent: moving a template into the group it is already in SHALL return
`200`, move nothing, and create no directory.

Before resolving the id to a file, the service SHALL re-read the templates tree, so the file it moves
is chosen against the tree as it is rather than against a registry that may predate it. After the
move it SHALL re-read again and confirm that the id is served from the destination path and that the
served content is byte-identical to the file it moved, on the same terms the `template-registry`
capability sets for every template write. A failed confirmation is `409` or `500` by that
capability's rules.

Moving the last template out of a directory SHALL leave that directory in place. The group survives
its members, and is removed only by the delete route.

The three `422` cases SHALL be told apart by `details.reason`: `template_group_invalid` for a path
that fails validation, `template_group_case_conflict` for one clashing with an existing sibling by
case alone, and `template_group_unsafe_path` for one crossing a symbolic link. `template_group_unpatchable` no longer exists: there is no file the service can fail to patch,
because it patches nothing.

`GET /api/openapi.json` SHALL document this route: its path parameter, its request body as
`{ "group": string | null }`, and every status in the table above, alongside the error reasons named
here.

This requirement supersedes the `docs/SPEC.md` §2 endpoint table and §2.0 template management to the
extent of this route and its semantics: both enumerate the template write surface without it. Every
other route in that table, and every other rule in §2.0, is unchanged, as is the `template-registry`
requirement that a `422` from a template write means nothing was written, which this endpoint
follows.

#### Scenario: Moving a template into a group

- **WHEN** `PUT /api/templates/bin-tag/group` is called with `{ "group": "Warehouse" }` for a
  template stored at `templates/bin-tag.yaml`
- **THEN** the response is `200` and the detail carries group `Warehouse`
- **AND** the file is at `templates/Warehouse/bin-tag.yaml`
- **AND** its bytes are unchanged, comments included

#### Scenario: Moving into a nested group creates the missing directories

- **WHEN** a template is moved to `Shipping/Pallets` and neither directory exists
- **THEN** the response is `200`
- **AND** both directories exist
- **AND** the file is at `templates/Shipping/Pallets/{id}.yaml`

#### Scenario: Clearing a group moves the file to the root

- **WHEN** `PUT /api/templates/bin-tag/group` is called with `{ "group": null }`
- **THEN** the response is `200` and the detail carries no group
- **AND** the file is at `templates/bin-tag.yaml`

#### Scenario: The id survives a move

- **WHEN** a favorited template is moved between groups
- **THEN** its id is unchanged
- **AND** it is still favorited

#### Scenario: A move never overwrites

- **WHEN** a template `pallet` in `Shipping` is moved to `Warehouse`, which already holds a file
  named `pallet.yml`
- **THEN** the response is `409`
- **AND** both files are unchanged on disk

#### Scenario: Moving to the group it already has changes nothing

- **WHEN** a template in `Warehouse` is moved to `Warehouse`
- **THEN** the response is `200`
- **AND** no file is moved and no directory is created

#### Scenario: A move refused before it relocates leaves no new group behind

- **WHEN** a move creates one or more directories on the way to its destination and is then refused
  before the file is relocated
- **THEN** every directory the request created is removed
- **AND** no group exists that did not exist before the request

#### Scenario: A move that relocated the file keeps its destination group

- **WHEN** a move relocates the file into a directory it created, and the post-move confirmation then
  fails with `409` or `500`
- **THEN** the file stays where the move put it, per the `template-registry` capability
- **AND** the directory holding it is not removed

#### Scenario: A symlinked group directory is refused

- **WHEN** `templates/Outside` is a symbolic link to a directory elsewhere and a template is moved to
  the group `Outside`
- **THEN** the response is `422` with `details.reason` `template_group_unsafe_path`
- **AND** nothing is written through the link
- **AND** the template is where it was

#### Scenario: A symlink deeper in the path is refused

- **WHEN** `templates/Shipping/Pallets` is a symbolic link and a template is moved to
  `Shipping/Pallets/Euro`
- **THEN** the response is `422` with `details.reason` `template_group_unsafe_path`
- **AND** nothing is created

#### Scenario: An invalid path is rejected and nothing moves

- **WHEN** the endpoint is called with `{ "group": "../escape" }`
- **THEN** the response is `422` with `details.reason` `template_group_invalid`
- **AND** the file is where it was
- **AND** nothing is written outside `templates/`

#### Scenario: A body omitting the key is rejected

- **WHEN** `PUT /api/templates/bin-tag/group` is called with the body `{}`
- **THEN** the response is `400`
- **AND** the template's group is unchanged

#### Scenario: An unknown template is a 404

- **WHEN** the endpoint is called for an id the registry does not hold
- **THEN** the response is `404`

#### Scenario: An emptied group stays a group

- **WHEN** the last template in `Shipping` is moved to `Warehouse`
- **THEN** `Shipping` is still listed by `GET /api/template-groups`

### Requirement: An empty group is deleted by name

`DELETE /api/template-groups/{path}` SHALL remove one group's directory. The path segment carries the
whole group path, so `DELETE /api/template-groups/Shipping/Pallets` addresses `Shipping/Pallets`.

The path SHALL be percent-decoded once, per the URI syntax, and the decoded value SHALL then be
validated as a group path like any other. A `%2F` therefore decodes to `/` and is read as a segment
separator, exactly as a literal `/` is: there is no way to address a group whose name contains a
slash, because no such group can exist. A decoded value that fails validation is a `400`.

A malformed percent sequence such as `%ZZ` SHALL be rejected with `400` before decoding. The decoder
this route sits behind passes an invalid sequence through literally rather than failing, and `%` is a
legal character in a group name, so `%ZZ` would otherwise decode to itself and address a group named
`%ZZ`. The check is therefore on the raw encoded path, not on the decoded value: every `%` in it must
introduce two hexadecimal digits.

The service SHALL resolve every component of the path, the final directory included, by exact entry
name: it lists the parent and requires an entry whose name matches the segment byte for byte. A
segment that matches no entry exactly is a `404`, even where the filesystem would have opened a
case-variant or otherwise aliased name. Deleting a group is not the place to discover that
`warehouse` and `Warehouse` are one directory here: the caller named an exact group, and a delete
that removed a differently spelled one would destroy a group nobody asked to destroy.

The service SHALL remove the directory only when it holds no entries at all. A directory holding a
template, a subdirectory, or any other file SHALL be refused with `409` and nothing removed. That is
what keeps this route incapable of destroying a template: it never deletes recursively, and the
caller empties a group by moving its templates out first.

Responses:

| Status | Meaning |
| --- | --- |
| `204` | The directory was removed. |
| `400` | The group path fails validation, or crosses a symbolic link. |
| `404` | No such directory under `templates/`. |
| `409` | The directory is not empty. Message names what it still holds. |
| `500` | The directory could not be removed, or the tree could not be re-read afterwards. |

There is no create-group route. A group is created by naming it in a request that puts a template
into it — a move, or a create with `?group=` — exactly as it was under the field model, and this
route is what balances that: it exists because a directory now outlives its
last template, which a group name in a YAML field never did.

`GET /api/openapi.json` SHALL document this route and every status in the table above.

This requirement supersedes the `docs/SPEC.md` §2 endpoint table and §2.0 to the extent of adding
this route. Every other route in that table is unchanged.

#### Scenario: Deleting an empty group

- **WHEN** `templates/Archive/` is empty and `DELETE /api/template-groups/Archive` is called
- **THEN** the response is `204`
- **AND** the directory is gone
- **AND** `Archive` is no longer listed

#### Scenario: A group holding a template is refused

- **WHEN** `templates/Warehouse/` holds `bin-tag.yaml` and the endpoint is called for `Warehouse`
- **THEN** the response is `409`
- **AND** the directory and the template are untouched

#### Scenario: A group holding a subgroup is refused

- **WHEN** `templates/Shipping/` holds only the empty directory `Pallets/` and the endpoint is called
  for `Shipping`
- **THEN** the response is `409`
- **AND** both directories remain

#### Scenario: A nested group is addressed by its whole path

- **WHEN** `templates/Shipping/Pallets/` is empty and
  `DELETE /api/template-groups/Shipping/Pallets` is called
- **THEN** the response is `204`
- **AND** `Shipping` remains

#### Scenario: A case-mismatched name is not deleted

- **WHEN** `templates/Warehouse/` is empty and `DELETE /api/template-groups/warehouse` is called on a
  filesystem that folds case
- **THEN** the response is `404`
- **AND** `templates/Warehouse/` still exists

#### Scenario: A case-mismatched intermediate segment is not followed

- **WHEN** `templates/Shipping/Pallets/` is empty and
  `DELETE /api/template-groups/shipping/Pallets` is called
- **THEN** the response is `404`
- **AND** both directories still exist

#### Scenario: An unknown group is a 404

- **WHEN** the endpoint is called for a directory that does not exist
- **THEN** the response is `404`

#### Scenario: A symlinked group is not followed

- **WHEN** `templates/Outside` is a symbolic link and `DELETE /api/template-groups/Outside` is called
- **THEN** the response is `400`
- **AND** neither the link's target nor the link itself is removed by this route

#### Scenario: A malformed percent sequence is refused before decoding

- **WHEN** `DELETE /api/template-groups/%ZZ` is called
- **THEN** the response is `400`
- **AND** no group named `%ZZ` is looked up or removed

#### Scenario: A traversal path is refused

- **WHEN** the endpoint is called for a path whose segment is `..`
- **THEN** the response is `400`
- **AND** nothing outside `templates/` is touched

## MODIFIED Requirements

### Requirement: Group is exposed on template summaries and detail

`GET /api/templates` SHALL carry each template's `group` in its summary, and `GET /api/templates/{id}`
SHALL carry it in the detail. The value is the template's directory path relative to `templates/`,
with `/` between segments. The field SHALL be omitted from the response body when the template is
ungrouped, so an existing consumer sees exactly the payload it saw before this change.

The `broken` list of `GET /api/templates` SHALL report a refused file by its path relative to
`templates/`, per the `template-registry` capability: a refused file has no group to report, and a
bare filename no longer identifies it.

The OpenAPI document at `GET /api/openapi.json` SHALL describe `group` on both schemas.

This requirement supersedes the `docs/SPEC.md` §2 endpoint-table success column for
`GET /api/templates` and `GET /api/templates/{id}` as to response content, and adds `group` to the
template payloads described in §2.0. It changes nothing else about those endpoints: the list stays
sorted by id, and detail still carries the full layout.

#### Scenario: A grouped template reports its group

- **WHEN** `GET /api/templates` is called and one template is stored under `templates/Warehouse/`
- **THEN** that template's summary carries `group` with the value `Warehouse`

#### Scenario: A nested group reports its whole path

- **WHEN** a template is stored at `templates/Shipping/Pallets/euro.yaml`
- **THEN** its summary carries `group` with the value `Shipping/Pallets`

#### Scenario: An ungrouped template omits the field

- **WHEN** `GET /api/templates/{id}` is called for a template stored at the root of `templates/`
- **THEN** the response body contains no `group` key

### Requirement: The template list filters by group

`GET /api/templates` SHALL accept an optional `group` query parameter:

- **absent** — every template is listed, at every depth, grouped and ungrouped alike;
- **`?group=<path>`** — only templates whose group equals `<path>` exactly, after stripping leading
  and trailing whitespace from the parameter;
- **`?group=`**, present with an empty value — only ungrouped templates, meaning the files at the
  root of `templates/`.

It SHALL also accept an optional `nested` parameter, `true` or `false`, defaulting to `false`. With
`nested=true` the `group` filter widens from that one group to that group and every group beneath it.
"Beneath it" is measured in whole path segments, never in characters: `Shipping2` is not beneath
`Shipping`, so a bare string-prefix test does not implement this. Widening
makes `?group=Shipping&nested=true` return the templates of `Shipping`, `Shipping/Pallets`, and
anything deeper. `?group=&nested=true` therefore selects every template in the tree, the root's
descendants being all of them. `nested` without `group` SHALL be accepted and SHALL change nothing,
the unfiltered list already being every template.

A `group` naming no existing group SHALL return `200` with an empty `templates` list, never `404`.
So SHALL a group that exists but holds no templates.

Filtering SHALL NOT change the order of the results, which stays ascending by id, and SHALL NOT
change the `broken` list, which continues to report every refused file regardless of the filter.

`GET /api/openapi.json` SHALL document both parameters on `GET /api/templates`, including that
`group` is optional, that an empty value selects the ungrouped templates, and what `nested` widens.

This requirement supersedes the `docs/SPEC.md` §2 endpoint-table row for `GET /api/templates`
("List template summaries (sorted by id)") as to the accepted query parameters.

#### Scenario: Filtering to one group

- **WHEN** `GET /api/templates?group=Warehouse` is called
- **THEN** every returned summary has group `Warehouse`
- **AND** templates in other groups and ungrouped templates are absent

#### Scenario: A parent group does not include its children by default

- **WHEN** templates exist in `Shipping` and in `Shipping/Pallets`, and
  `GET /api/templates?group=Shipping` is called
- **THEN** only the templates directly in `Shipping` are returned

#### Scenario: The nested switch includes the whole branch

- **WHEN** `GET /api/templates?group=Shipping&nested=true` is called
- **THEN** the templates of `Shipping` and of `Shipping/Pallets` are all returned

#### Scenario: The nested switch does not match a similarly named sibling

- **WHEN** groups `Shipping`, `Shipping/Pallets` and `Shipping2` all hold templates, and
  `GET /api/templates?group=Shipping&nested=true` is called
- **THEN** the templates of `Shipping` and `Shipping/Pallets` are returned
- **AND** no template of `Shipping2` is returned

#### Scenario: Filtering to the ungrouped templates

- **WHEN** `GET /api/templates?group=` is called
- **THEN** only templates at the root of `templates/` are returned

#### Scenario: The nested switch on the root selects everything

- **WHEN** `GET /api/templates?group=&nested=true` is called
- **THEN** every template in the tree is returned

#### Scenario: An unknown group is empty, not an error

- **WHEN** `GET /api/templates?group=Nonexistent` is called
- **THEN** the response is `200`
- **AND** `templates` is empty

#### Scenario: An empty group is empty, not an error

- **WHEN** `templates/Archive/` exists and holds nothing, and `GET /api/templates?group=Archive` is
  called
- **THEN** the response is `200` with an empty `templates` list

#### Scenario: Case is significant

- **WHEN** templates exist in `Warehouse` and `GET /api/templates?group=warehouse` is called
- **THEN** the response is `200` with an empty `templates` list

#### Scenario: Broken files are reported whatever the filter

- **WHEN** one file in the tree is refused and `GET /api/templates?group=Warehouse` is called
- **THEN** the refused file still appears under `broken`

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

The view SHALL support selecting several templates and moving them in one action, reporting per
template which moves succeeded when some fail.

After a successful move or delete the view SHALL reflect the new tree without a manual reload.

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

## REMOVED Requirements

### Requirement: A template declares its group as an optional top-level field

**Reason**: A group carried as a string in every member file has no identity: renaming one means
rewriting every member, and the group can end up split across two names mid-loop (#227). A directory
holds the name once, and renaming it is one `rename(2)`. This also retires the ADR-0061 decision that
a group is one flat name in which `/` carries no meaning: nested directories are nested groups.

**Migration**: The complete post-change contract is the ADDED requirement "A group is a directory
under the templates directory", which also carries the post-change top-level field table and keeps
superseding the `docs/SPEC.md` §3 table. A file still carrying a `group:` key is quarantined as an
unknown field, like any other invalid template. No migration ships with this change, and no repair
guidance is owed to such a file.

### Requirement: A template is moved between groups without rewriting the rest of its file

**Reason**: The single-key YAML patch existed only to move a template between groups without
disturbing its bytes. Moving the file does that by construction, so the patch, its four classes of
refused file, and the `template_group_unpatchable` reason all go away, and ADR-0062, which authorized
the service to rewrite one key of a hand-authored file, is superseded with nothing left to authorize.

**Migration**: The complete post-change contract is the ADDED requirement "A template is moved
between groups by moving its file". The route and request body are unchanged, so a caller sending
`{ "group": "Warehouse" }` or `{ "group": null }` needs no change. A caller matching on
`details.reason` `template_group_unpatchable` must stop: that reason no longer exists, and its 422 is
replaced by `template_group_invalid`, `template_group_case_conflict`, or a `409` for an occupied
destination.
