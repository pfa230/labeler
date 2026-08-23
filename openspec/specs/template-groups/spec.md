# template-groups Specification

## Purpose

Defines how a template declares the group it belongs to, what a valid group name is, how groups reach the API on template summaries and detail, how the template list is filtered by group, how a template is moved from one group to another without disturbing the rest of its file, and how groups are browsed in the Labels view.

## Requirements

### Requirement: A template declares its group as an optional top-level field

A template MAY carry a top-level `group` field naming the one group it belongs to. The field is optional; a template without it is *ungrouped*, and ungrouped is a first-class state, not an error.

A group name is one flat name. The service SHALL NOT assign meaning to any character inside it: a name containing `/` is a name, not a path, and produces no hierarchy.

A `group` value SHALL be valid when, after stripping leading and trailing whitespace, it is:

- a string;
- non-empty;
- at most 64 characters;
- free of control characters, including tab, carriage return, and line feed.

The stored group is the whitespace-stripped value. A `group` that is not a string, is empty or whitespace-only, exceeds 64 characters, or carries a control character SHALL make the template fail to load, with a parse or validation message naming the `group` field. That quarantines the file under the existing rules of the `template-registry` capability rather than aborting startup.

Group names SHALL be compared exactly, including case: `Warehouse` and `warehouse` are two groups.

The post-change set of top-level template fields is:

| Field | Type | Notes |
| --- | --- | --- |
| `id` | string | Required, non-empty. Uniqueness across the directory is enforced per `template-registry`. |
| `name` | string | Required, non-empty. |
| `description` | string | Optional. |
| `group` | string | Optional. One flat group name, validated as above. Absent means ungrouped. |
| `unit` | `"mm"` \| `"in"` | Length unit for all coordinates/sizes in the template. |
| `dpi` | integer > 0 | Raster resolution for PNG output. |
| `format` | object | See `docs/SPEC.md` §3.1. |
| `params` | map | Optional. Map of parameter name → `ParamSpec`. See `docs/SPEC.md` §3.0. |
| `options` | map | Optional, legacy. Map of option name → allowed values, desugared into an enum `params` entry. Still accepted; not for new templates (ADR-0055, ADR-0056). |
| `layout` | list | Tree of layout items. See `docs/SPEC.md` §4. |
| `version` | string | Optional, free-form. |

Parsing still rejects unknown fields. `options` is listed because this table is the complete set of accepted top-level fields: it parses today and SHALL keep parsing, so omitting it here would make a working template read as invalid.

This requirement supersedes the `docs/SPEC.md` §3 top-level field table, and only that table. Every other rule in §3, and the frozen §3.0 and §3.1 subsections, stay authoritative.

#### Scenario: A template carries a group

- **WHEN** a template file declares `group: Warehouse` and is otherwise valid
- **THEN** the template loads
- **AND** its group is `Warehouse`

#### Scenario: A template without the field is ungrouped

- **WHEN** a valid template file declares no `group` key
- **THEN** the template loads
- **AND** it is reported as ungrouped, not as broken

#### Scenario: Surrounding whitespace is not part of the name

- **WHEN** a template declares `group: "  Warehouse  "`
- **THEN** its group is `Warehouse`

#### Scenario: An empty group name is a validation failure

- **WHEN** a template declares `group: ""` or a whitespace-only value
- **THEN** the template fails to load, with a message naming `group`
- **AND** the file is quarantined as broken while the service keeps running

#### Scenario: An over-long or control-character name is a validation failure

- **WHEN** a template declares a `group` longer than 64 characters, or one containing a line feed
- **THEN** the template fails to load, with a message naming `group`

#### Scenario: A present but valueless group is a failure, not an absent one

- **WHEN** a template declares `group:` with nothing after it, or `group: ~`, or `group: null`
- **THEN** the template fails to load, with a message naming `group`
- **AND** it is not served as ungrouped

#### Scenario: A non-string group is a failure

- **WHEN** a template declares `group: 42` or `group: true`
- **THEN** the template fails to load, with a message naming `group`

#### Scenario: A slash produces no hierarchy

- **WHEN** two templates declare `group: Shipping/Pallets` and `group: Shipping`
- **THEN** they belong to two unrelated groups
- **AND** neither is nested inside the other

### Requirement: Group is exposed on template summaries and detail

`GET /api/templates` SHALL carry each template's `group` in its summary, and `GET /api/templates/{id}` SHALL carry it in the detail. The field SHALL be omitted from the response body when the template is ungrouped, so an existing consumer sees exactly the payload it saw before this change.

The `broken` list of `GET /api/templates` SHALL be unchanged: a refused file is reported as `{ filename, error }` whether or not it declares a group, since a file that failed to load has no group to report.

The OpenAPI document at `GET /api/openapi.json` SHALL describe `group` on both schemas.

This requirement supersedes the `docs/SPEC.md` §2 endpoint-table success column for `GET /api/templates` and `GET /api/templates/{id}` as to response content, and adds `group` to the template payloads described in §2.0. It changes nothing else about those endpoints: the list stays sorted by id, and detail still carries the full layout.

#### Scenario: A grouped template reports its group

- **WHEN** `GET /api/templates` is called and one template declares `group: Warehouse`
- **THEN** that template's summary carries `group` with the value `Warehouse`

#### Scenario: An ungrouped template omits the field

- **WHEN** `GET /api/templates/{id}` is called for a template that declares no group
- **THEN** the response body contains no `group` key

### Requirement: The template list filters by group

`GET /api/templates` SHALL accept an optional `group` query parameter:

- **absent** — every template is listed, grouped and ungrouped alike;
- **`?group=<name>`** — only templates whose group equals `<name>` exactly, after the same whitespace stripping applied to the stored value;
- **`?group=`**, present with an empty value — only ungrouped templates.

A `group` naming no existing group SHALL return `200` with an empty `templates` list, never `404`.

Filtering SHALL NOT change the order of the results, which stays ascending by id, and SHALL NOT change the `broken` list, which continues to report every refused file regardless of the filter, because a refused file has no group to filter on.

`GET /api/openapi.json` SHALL document the `group` query parameter on `GET /api/templates`, including that it is optional and that an empty value selects the ungrouped templates.

This requirement supersedes the `docs/SPEC.md` §2 endpoint-table row for `GET /api/templates` ("List template summaries (sorted by id)") as to the accepted query parameters.

#### Scenario: Filtering to one group

- **WHEN** `GET /api/templates?group=Warehouse` is called
- **THEN** every returned summary has group `Warehouse`
- **AND** templates in other groups and ungrouped templates are absent

#### Scenario: Filtering to the ungrouped templates

- **WHEN** `GET /api/templates?group=` is called
- **THEN** only templates declaring no group are returned

#### Scenario: An unknown group is empty, not an error

- **WHEN** `GET /api/templates?group=Nonexistent` is called
- **THEN** the response is `200`
- **AND** `templates` is empty

#### Scenario: Case is significant

- **WHEN** templates exist in `Warehouse` and `GET /api/templates?group=warehouse` is called
- **THEN** the response is `200` with an empty `templates` list

#### Scenario: Broken files are reported whatever the filter

- **WHEN** one file in the directory is refused and `GET /api/templates?group=Warehouse` is called
- **THEN** the refused file still appears under `broken`

### Requirement: A template is moved between groups without rewriting the rest of its file

`PUT /api/templates/{id}/group` SHALL set or clear one template's group. The request body is `{ "group": "<name>" }` to move it into a group, or `{ "group": null }` to make it ungrouped.

The service SHALL apply the change as a targeted edit of the stored file: it replaces the value of the existing top-level `group:` line, or, when there is none, inserts one. Every other byte of the file SHALL be preserved exactly, including comments, key order, indentation style, quoting of other values, and blank lines. Clearing a group SHALL remove the whole `group:` line.

The patched text SHALL be parsed and validated before anything is written, and SHALL be written only if it is a valid template whose group reads back as the requested value. The file SHALL be replaced atomically, and the registry reloaded, exactly as the other template writes do.

The service SHALL refuse, with nothing written, any file it cannot patch unambiguously:

- one holding more than one YAML document;
- one whose top-level `group` value is not a scalar the patch can replace in place. Plain and quoted scalars are patchable; a block scalar, a flow collection, an anchor or alias, and a value introducing a nested block are not;
- one the template parser reads as already having a group while the patch cannot identify exactly one top-level `group:` line to replace, which is what a quoted key (`"group": Shipping`) or any other spelling the parser accepts and the patch does not recognize produces. Inserting a second line in that case is forbidden: a file SHALL never come back with two top-level `group` keys;
- one whose root is not a block mapping written one key per line, a top-level flow mapping (`{id: bin-tag, name: Bin Tag, ...}`) being the case that reaches the parser intact. Such a file offers no line to replace and no line to insert, so it is refused rather than reflowed.

A `group:` key that is not top-level, such as one inside `params` or a layout item, SHALL be left alone and SHALL NOT be mistaken for the template's group.

Responses:

| Status | Meaning |
| --- | --- |
| `200` | Moved. Body is the updated `TemplateDetail`. |
| `400` | The path id is invalid, or the body is not `{ "group": string \| null }`. |
| `404` | The registry holds no template with that id. |
| `409` | After the write, the id is served from a different file. See the collision clause below. |
| `422` | The group name fails validation, or the file cannot be patched unambiguously. |
| `500` | The file could not be written, the directory could not be re-read afterwards, or the post-write confirmation found the patched file gone, renamed, re-identified, or replaced. |

The operation SHALL be idempotent: setting the group a template already has, or clearing an already ungrouped template, SHALL return `200` and leave the file byte-identical.

Before resolving the id to a file, the service SHALL re-read the templates directory, so the file it patches is chosen against the directory as it is rather than against a registry that may predate it.

On the branch that writes, the service SHALL confirm after the reload that the id is served from the file it patched and that the served content is byte-identical to the patched text, and SHALL NOT return `200` describing anything else. A file elsewhere in the directory declaring the same id but sorting after the patched file does not displace it: the patched file still serves the id, the confirmation passes, and the response is `200` while that other file is reported in `broken[]`. When the id is instead served from a different file, and the patched file survives intact and refused, the response SHALL be `409 TemplateIdCollision`, naming the id, the patched file and the file now serving the id, exactly as `PUT /api/templates/{id}` does; the patch stays in the file it was applied to, and that file is reported as broken by `GET /api/templates`. When instead the patched file is gone, renamed, re-identified, or holding content the service did not write, the response SHALL be `500`, reporting the template missing after the write rather than a collision, on the same precedence the `template-registry` capability specifies for every write endpoint. The idempotent branch writes nothing and performs no post-write reload, so it makes no such claim and can return neither.

This endpoint SHALL be the only path by which the service writes a group into a hand-authored file. `PUT /api/templates/{id}` continues to replace the whole file from a submitted body, and a `group:` key typed there is honoured exactly like any other field.

The two `422` cases SHALL be told apart by `details.reason`: `template_group_invalid` for a name that fails validation, and `template_group_unpatchable` for a file the patch will not touch. Both are additions to the reason registry of `docs/SPEC.md` §10.1, which is frozen and therefore does not list them; this requirement is their published home.

The request body SHALL carry the `group` key. `{ "group": null }` clears the group deliberately, while a body omitting the key entirely is malformed and SHALL be rejected with `400`, since treating an absent key as null would let an empty object silently ungroup a template.

`GET /api/openapi.json` SHALL document this route: its path parameter, its request body as `{ "group": string | null }`, and every status in the table above, alongside the two error reasons named here.

This requirement supersedes the `docs/SPEC.md` §2 endpoint table and §2.0 template management to the extent of adding this route and its semantics: both enumerate the template write surface without it. Every other route in that table, and every other rule in §2.0, is unchanged, as is the `template-registry` requirement that a `422` from a template write means nothing was written, which this endpoint follows.

#### Scenario: Moving a template into a group

- **WHEN** `PUT /api/templates/bin-tag/group` is called with `{ "group": "Warehouse" }`
- **THEN** the response is `200` and the detail carries group `Warehouse`
- **AND** `GET /api/templates/bin-tag/source` shows a `group: Warehouse` line
- **AND** every comment and every other line of the file is unchanged

#### Scenario: Changing an existing group replaces only that line

- **WHEN** a template already declaring `group: Shipping` is moved to `Warehouse`
- **THEN** the stored file differs from its previous content only in the value on the `group:` line

#### Scenario: A quoted value and a trailing comment both survive

- **WHEN** a template whose line reads `group: "A # B"  # keep me` is moved to `Warehouse`
- **THEN** the response is `200` and its group is `Warehouse`
- **AND** the `# keep me` comment is still on the `group:` line

#### Scenario: Clearing a group

- **WHEN** `PUT /api/templates/bin-tag/group` is called with `{ "group": null }`
- **THEN** the response is `200` and the detail carries no group
- **AND** the stored file no longer contains a top-level `group:` line

#### Scenario: An invalid name is rejected and nothing is written

- **WHEN** the endpoint is called with a group name of 200 characters
- **THEN** the response is `422`
- **AND** the stored file is unchanged

#### Scenario: A body omitting the key is rejected

- **WHEN** `PUT /api/templates/bin-tag/group` is called with the body `{}`
- **THEN** the response is `400`
- **AND** the template's group is unchanged

#### Scenario: An unknown template is a 404

- **WHEN** the endpoint is called for an id the registry does not hold
- **THEN** the response is `404`

#### Scenario: A nested `group:` key is not the template's group

- **WHEN** a template declaring no top-level group, but carrying a param named `group`, is moved into `Warehouse`
- **THEN** a top-level `group: Warehouse` line is added
- **AND** the param named `group` is untouched

#### Scenario: An unpatchable file is refused

- **WHEN** the endpoint is called for a template whose file holds two YAML documents
- **THEN** the response is `422`
- **AND** the stored file is unchanged

#### Scenario: A flow-mapping root is refused

- **WHEN** the endpoint is called for a valid template whose whole document is one top-level flow mapping and which declares no group
- **THEN** the response is `422`
- **AND** the stored file is unchanged

#### Scenario: An unrecognized spelling of the key is refused, not duplicated

- **WHEN** the endpoint is called for a template whose group is written as `"group": Shipping`
- **THEN** the response is `422`
- **AND** the stored file is unchanged
- **AND** no second top-level `group` key is added

#### Scenario: Moving to the group it already has changes nothing

- **WHEN** a template in `Warehouse` is moved to `Warehouse`
- **THEN** the response is `200`
- **AND** the stored file is byte-identical to before the call

#### Scenario: A group update whose id moved to another file fails loudly

- **WHEN** the endpoint patches the file the registry held for an id, and the re-read then serves that id from a different file that appeared on disk meanwhile
- **THEN** the response is `409 TemplateIdCollision`
- **AND** the response does not describe the other file's template
- **AND** the patched file is reported as broken by `GET /api/templates`

#### Scenario: An idempotent call is unaffected by a collision elsewhere

- **WHEN** a template is moved to the group it already has while an unrelated file in the directory is refused
- **THEN** the response is `200`
- **AND** the stored file is byte-identical to before the call

### Requirement: The Labels view browses and edits groups

The Labels view SHALL let a user browse by group and move templates between groups without editing YAML.

It SHALL show a filter control listing `All`, every group in use in ascending Unicode code-point order, and `Ungrouped`. That is the ordering the API already applies to template ids, so `Warehouse` sorts before `warehouse`, and it holds for every valid group name, including one outside the Basic Multilingual Plane, where ordering by UTF-16 code unit would disagree. `Ungrouped` SHALL be offered only while at least one ungrouped template exists. Selecting an entry SHALL narrow the grid to that group, and SHALL compose with the existing text search so that both constraints apply together.

While a group other than `All` is selected, the view SHALL hide the Favorites and Recents rows, exactly as an active search already does, so that everything on screen belongs to the selected group. When the selection is `All` and the search box is empty, both rows return.

When a group and a search term together match nothing, the view SHALL say so rather than render an empty grid.

Each template card SHALL show its group, and SHALL offer a **Move to…** action. The move dialog SHALL offer the groups already in use and SHALL accept a name that is not yet in use, which creates that group by the act of moving a template into it. It SHALL offer a way to make the template ungrouped. A group therefore exists exactly as long as at least one template names it; there is no separate create or delete step.

The view SHALL support selecting several templates and moving them in one action, reporting per template which moves succeeded when some fail.

After a successful move the view SHALL reflect the template's new group without a manual reload.

Favorites and Recents SHALL otherwise keep working exactly as they do today: they are per-user shortcuts over the whole set, a move SHALL NOT change either, and neither is reordered or pruned by grouping.

#### Scenario: Filtering the grid to a group

- **WHEN** the user selects the `Warehouse` filter
- **THEN** only Warehouse templates are shown in the grid

#### Scenario: Group filter and search compose

- **WHEN** the user selects `Warehouse` and types `bin` in the search box
- **THEN** only Warehouse templates matching `bin` are shown

#### Scenario: Ungrouped is offered only when it is non-empty

- **WHEN** every template belongs to a group
- **THEN** the filter control offers no `Ungrouped` entry

#### Scenario: Moving one template from the grid

- **WHEN** the user picks **Move to…** on a card and chooses `Warehouse`
- **THEN** the card shows `Warehouse` without a manual page reload

#### Scenario: Creating a group by naming it

- **WHEN** the user types a group name that no template uses and confirms the move
- **THEN** the template is moved into that group
- **AND** the new group appears in the filter control

#### Scenario: Moving several templates at once

- **WHEN** the user selects three templates and moves them to `Shipping`
- **THEN** all three are moved
- **AND** a failure on one of them is reported without hiding the successes of the others

#### Scenario: A group filter hides the Favorites and Recents rows

- **WHEN** the user selects the `Warehouse` filter while favorites exist
- **THEN** the Favorites and Recents rows are not shown
- **AND** selecting `All` again brings both back

#### Scenario: A group and a search that match nothing say so

- **WHEN** the user selects `Warehouse` and types a term no Warehouse template matches
- **THEN** the view reports that nothing matches

#### Scenario: A move leaves favorites alone

- **WHEN** a favorited template is moved to another group
- **THEN** it is still favorited
- **AND** it appears in the Favorites row again once the filter is cleared
