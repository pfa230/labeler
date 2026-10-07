# Templates

## Purpose

Defines the template file format (top-level keys, `unit`, `dpi`, `format`, `categories`), the template registry built from the templates folder, the template endpoints, the thumbnail and source endpoints, the template catalog, and the Labels view's category filter.

## Requirements

### Requirement: Template file keys

A template SHALL be a YAML mapping with exactly these top-level keys. Every mapping in a template, at any depth, SHALL reject a key it does not define, and every key written as `null`, with an error naming the key's path; a template carrying one is invalid.

| Key | Type | Required | Constraint |
|---|---|---|---|
| `name` | string | yes | not empty after trimming |
| `description` | string | no | reported as `""` when absent |
| `categories` | sequence of strings | no | the categories the template is listed under; reported as `[]` when absent |
| `unit` | `mm` \| `in` | yes | the length unit of every coordinate and size in the template |
| `dpi` | integer | yes | > 0; raster resolution of PNG output |
| `format` | mapping | yes | see "Label format" |
| `params` | sequence | no | see `parameters` |
| `layout` | sequence | yes | see `layout` |

`id` SHALL NOT be a key: it comes from the filename.

#### Scenario: An unknown or null key is refused

- **WHEN** a template declares `options:` or `id:` alongside the keys above, or writes `description: null`
- **THEN** it is invalid, with an error naming that key

### Requirement: Label format

`format` SHALL be tagged by `type`: `single` (one label) or `sheet` (a grid of identical slots on a page). Lengths are in the template `unit`.

| `type` | Key | Type | Required | Constraint |
|---|---|---|---|---|
| `single` | `width` | Dimension or range | yes | |
| `single` | `height` | Dimension | yes | |
| `single` | `media_width` | number | no | > 0; nominal media width, read only by `printing`, no effect on rendering |
| `sheet` | `paper_width`, `paper_height` | number | yes | > 0 |
| `sheet` | `label_width`, `label_height` | number | yes | > 0; the size of every slot |
| `sheet` | `positions` | sequence of `[x, y]` | yes | not empty; `x`, `y` ≥ 0; each is a slot's bottom-left corner, page origin bottom-left |

A Dimension SHALL be a number or a parameter reference `"{name}"` resolved from that parameter's value (see `parameters`). `width` MAY instead be a range `{ min, max }` whose bounds are each a Dimension; a range needs both bounds, checked after instantiating parameter defaults, with the message `a dynamic-width single template must specify both width.min and width.max`. A literal fixed value or literal bound SHALL be > 0, and literal `min` SHALL be ≤ literal `max`. At load, parameter references in `format` and `layout` geometry SHALL be instantiated with their defaults and the result validated.

#### Scenario: A sheet with no positions is refused

- **WHEN** a `sheet` format declares `positions: []`
- **THEN** the template is invalid with the message `positions must not be empty`

#### Scenario: A range on height is refused

- **WHEN** a `single` format declares `height: { min: 10, max: 20 }`
- **THEN** the template is invalid, with an error naming `format.height`

### Requirement: Auto-length labels

A `single` template whose `width` is a range SHALL be auto-length: at render its width SHALL be the layout's measured content width, given `max` as the available width, clamped to `[min, max]`. How text fits within that budget is owned by `text`. A fixed-width `single` and every `sheet` SHALL use their declared size.

#### Scenario: Width follows content within the bounds

- **WHEN** an auto-length template with `width: { min: 10, max: 100 }` renders content 30 units wide, then content 300 units wide
- **THEN** the labels are 30 and 100 units wide

### Requirement: Invalid templates are refused on write and quarantined at load

A template body SHALL be parsed and validated by the same rules whether it comes from a file at load or from an HTTP write. On a write, a body that fails SHALL answer `422 TemplateInvalid` with reason `template_validation_failed` and a message naming the path inside the template, before anything is written. At load, the file SHALL be quarantined (see "Registry load").

#### Scenario: An invalid body changes nothing

- **WHEN** `PUT /api/templates/pallet` receives a body with a top-level `options:` key
- **THEN** the response is `422` with `error.code` `TemplateInvalid`, naming `options`
- **AND** no file is changed

### Requirement: Registry load

At startup and on every reload the service SHALL build the registry from `{LABELER_CONFIG_DIR}/templates/`, creating that folder at startup when missing and seeding nothing into it. It SHALL load every entry directly in that folder whose name ends in `.yaml`, skip without reporting every entry whose name begins with `.`, and report every other entry, file or folder, as broken with the message `not a template file (expected <id>.yaml)`. It SHALL never create, move, rename or rewrite anything while loading.

A `.yaml` file SHALL be refused (excluded from the served set and reported as broken) when its filename stem is not a valid id, it cannot be read, or it fails parsing or validation. Startup SHALL abort only when the templates folder cannot be read or enumerated, or an entry's metadata cannot be read.

#### Scenario: A malformed file does not stop the service

- **WHEN** the folder holds one valid template and one file that is not valid template YAML
- **THEN** the service starts, serves the valid template, and reports the other as broken

#### Scenario: Only .yaml files are templates

- **WHEN** `templates/` holds a valid `pallet.yml`, a `notes.txt` and a folder `Shipping/`
- **THEN** none is served, and each is reported as broken with the message `not a template file (expected <id>.yaml)`

#### Scenario: A dot-entry is invisible

- **WHEN** `templates/.attic/` or `templates/.old.yaml` exists
- **THEN** it is neither served nor reported

#### Scenario: An unreadable templates folder is fatal

- **WHEN** the templates folder cannot be read at startup
- **THEN** the service exits with a fatal error naming the folder

### Requirement: Template id is the filename

A template's id SHALL be its filename stem: `templates/pallet.yaml` is the template `pallet`. An id SHALL be non-empty and match `^[a-zA-Z0-9_-]+$`.

#### Scenario: The filename names the template

- **WHEN** a valid template is stored at `templates/euro.yaml`
- **THEN** `GET /api/templates/euro` returns it

#### Scenario: A filename that cannot be an id is refused

- **WHEN** `templates/my template.yaml` holds an otherwise valid template
- **THEN** it is reported as broken with a message naming the stem and the rule, and every other template is served

### Requirement: Listing templates

`GET /api/templates` SHALL answer `200 { "templates": [TemplateSummary...], "broken": [{ "path", "error" }...] }`. `templates` SHALL be sorted ascending by id. A summary SHALL carry `id`, `name`, `description`, `categories`, `unit`, `dpi`, `params` and `format`. `broken` SHALL list every refused entry by its name in `templates/` with the refusal message, and SHALL be omitted when empty.

#### Scenario: A summary carries its categories

- **WHEN** `pallet.yaml` declares `categories: [Shipping, Warehouse]` and `bin.yaml` declares none
- **THEN** the summaries carry `categories` `["Shipping", "Warehouse"]` and `[]`

### Requirement: Template detail and source

`GET /api/templates/{id}` SHALL answer `200` with a TemplateDetail: `id`, `name`, `description`, `categories`, `unit`, `dpi`, `format`, `params`, and `variables` (the `{vars.<key>}` keys the template reads, without the prefix, ascending), or `404 NotFound`. `GET /api/templates/{id}/source` SHALL answer `200` with the stored file's bytes as `text/yaml; charset=utf-8`, `400` with reason `template_id_invalid` when the id does not match the id rule, and `404 NotFound` when the registry holds no such id or its file cannot be read. The source is the only way to read a template's layout.

#### Scenario: Source returns the file as stored

- **WHEN** `GET /api/templates/pallet/source` is called for a template whose file carries comments
- **THEN** the body is the file's bytes, comments included

#### Scenario: The detail carries no layout

- **WHEN** `GET /api/templates/pallet` is called
- **THEN** the body has no `layout` key

### Requirement: Reload

`POST /api/templates/reload` SHALL rebuild the registry from disk and swap it in, answering `200 { "count": <served>, "broken_count": <refused> }`, including when entries were refused. When the folder cannot be read it SHALL answer `500 Internal` and keep the previously loaded registry. Every refused entry SHALL also be logged as a warning.

#### Scenario: Reload reports refused files instead of failing

- **WHEN** an invalid file appears on disk and reload is called
- **THEN** the response is `200` with `broken_count` counting the refused file
- **AND** `GET /api/templates` lists it under `broken`

#### Scenario: An unreadable folder keeps the live set

- **WHEN** the templates folder cannot be read and reload is called
- **THEN** the response is `500 Internal` and the previous templates stay served

### Requirement: Create with POST, replace with PUT

`POST /api/templates/{id}` with a raw YAML body SHALL create the template, answering `201` with the TemplateDetail, or `409 Conflict` when `templates/{id}.yaml` exists; a create SHALL never overwrite a file. `PUT /api/templates/{id}` with a raw YAML body SHALL replace the template, answering `200` with the TemplateDetail, or `404 NotFound` when `templates/{id}.yaml` does not exist. Both SHALL answer `400` with reason `template_id_invalid` for an id not matching the id rule and `422` for an invalid body (see "Invalid templates are refused on write"), before anything is written. The service SHALL write `templates/{id}.yaml` atomically, then reload the registry.

#### Scenario: POST creates a template

- **WHEN** `POST /api/templates/pallet` is called with a valid body and no `pallet.yaml` exists
- **THEN** the response is `201` and the file is at `templates/pallet.yaml`

#### Scenario: POST refuses a taken id

- **WHEN** `POST /api/templates/pallet` is called while `templates/pallet.yaml` exists, served or broken
- **THEN** the response is `409 Conflict` and the existing file is unchanged

#### Scenario: PUT replaces a template

- **WHEN** `pallet` is served and `PUT /api/templates/pallet` is called with a valid body
- **THEN** the response is `200` and `templates/pallet.yaml` holds the new body

#### Scenario: PUT refuses an absent template

- **WHEN** `PUT /api/templates/pallet` is called and no `pallet.yaml` exists
- **THEN** the response is `404 NotFound` and nothing is written

### Requirement: Delete a template

`DELETE /api/templates/{id}` SHALL remove `templates/{id}.yaml` and answer `204`, then drop every user's favorite for that id; recents and job history SHALL be left alone. It SHALL answer `400` with reason `template_id_invalid` for an id not matching the id rule and `404 NotFound` when the file does not exist.

#### Scenario: Delete drops favorites and keeps recents

- **WHEN** two users have favorited and printed `pallet` and `DELETE /api/templates/pallet` is called
- **THEN** the response is `204`, the file is gone, and neither user's favorites hold `pallet`
- **AND** the job history still records the prints

### Requirement: Thumbnail

`GET /api/templates/{id}/thumbnail` SHALL render one label to PNG at the template's `dpi` (a `sheet` renders one slot at `label_width` × `label_height`) from the placeholder values defined by `parameters`, as a render does. The response SHALL carry `Content-Type: image/png`, `Cache-Control: no-cache` and an `ETag` that is the quoted lowercase hex SHA-256 of the PNG bytes. A request whose `If-None-Match` equals that ETag or is `*` SHALL answer `304` with the ETag and no body. An unknown id SHALL answer `404 NotFound`; a render failure fails as the render does (see `rendering`).

#### Scenario: A matching ETag revalidates

- **WHEN** the thumbnail is fetched and then fetched again with `If-None-Match` set to the returned `ETag`, nothing having changed
- **THEN** the second response is `304`

#### Scenario: A sheet thumbnail is one label

- **WHEN** the thumbnail of a `sheet` template is requested
- **THEN** the PNG is the size of one slot, not the page

### Requirement: Template catalog index

`catalog/` in the repository SHALL hold installable templates at `catalog/<category>/<file>.yaml` or `catalog/<category>/<vendor>/<file>.yaml`. `cargo run --bin catalog-index` SHALL parse and validate each with the server's rules, failing on an invalid template or any other path shape, and write `catalog/index.json`: a JSON array, in ascending path order, of entries `{ id, name, description, path, category, vendor, format, media_width_mm, fields }`, where `id` is the filename stem, `path` is relative to `catalog/`, `vendor` is `null` for the two-level shape, `format` is `single` or `sheet`, `media_width_mm` is `format.media_width` or `null`, and `fields` lists the names of the template's declared parameters in declaration order. CI SHALL fail when the committed index differs from the regenerated one.

#### Scenario: A stale index fails CI

- **WHEN** a catalog template changes and `catalog/index.json` is not regenerated
- **THEN** the CI catalog step fails

### Requirement: Catalog page

The UI catalog page SHALL fetch `index.json` and each template's YAML from the repository's raw-content URL in the browser; the server SHALL make no outbound request for the catalog. Entries SHALL be grouped under `category · vendor` (or `category`), headings sorted, each card showing name, description, format, media width in mm when present, and fields (`none` when empty), marked `installed` when the id is served. **Install** SHALL send `POST /api/templates/{id}`. A `409` SHALL open a dialog showing the installed source beside the catalog YAML, offering **Replace** (a `PUT`) or **Cancel**; a `422` SHALL report that the template needs a newer version of labeler. When the catalog cannot be fetched, the page SHALL say so and link to pasting YAML at `/templates/new`.

#### Scenario: Installing an already installed template offers a diff

- **WHEN** the user installs an entry whose id is already served
- **THEN** the `POST` answers `409`
- **AND** the page shows both versions and replaces only when the user confirms

#### Scenario: An unreachable catalog falls back to paste

- **WHEN** the browser cannot fetch `index.json`
- **THEN** the page says the catalog could not be reached and links to `/templates/new`

### Requirement: Labels view categories

The Labels view SHALL offer a category filter of `All`, one choice per category declared by a served template, sorted, and `Uncategorized` while a served template declares none. Choosing a category SHALL narrow the grid to the templates listing it, and `Uncategorized` to those listing none. The filter SHALL compose with the search box, which matches id or name case-insensitively. An empty result SHALL say that nothing matches.

#### Scenario: A template appears under each of its categories

- **WHEN** `pallet` declares `categories: [Shipping, Warehouse]` and `bin` declares none
- **THEN** choosing `Shipping` or `Warehouse` shows `pallet`, and choosing `Uncategorized` shows only `bin`
