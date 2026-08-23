## MODIFIED Requirements

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
