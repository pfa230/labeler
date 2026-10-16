## MODIFIED Requirements

### Requirement: Template id is the filename

A template's id SHALL be its filename stem: `templates/pallet.yaml` is the template `pallet`. An id SHALL be non-empty and match `^[a-zA-Z0-9_-]+$`. Every valid id SHALL be usable at its literal `/api/templates/{id}` path for the create, detail, replacement and deletion operations defined below.

#### Scenario: The filename names the template

- **WHEN** a valid template is stored at `templates/euro.yaml`
- **THEN** `GET /api/templates/euro` returns it

#### Scenario: A filename that cannot be an id is refused

- **WHEN** `templates/my template.yaml` holds an otherwise valid template
- **THEN** it is reported as broken with a message naming the stem and the rule, and every other template is served

#### Scenario: A valid id has a complete literal-path lifecycle

- **WHEN** a client creates the template `reload` by posting valid YAML to `/api/templates/reload`, reads its detail, replaces it with valid YAML and deletes it at that same literal path
- **THEN** the requests follow the Create with POST, replace with PUT, Template detail and source, and Delete a template requirements respectively
- **AND** the create returns `201` with template id `reload` and writes `templates/reload.yaml`

### Requirement: Reload

`POST /api/template-registry/reload` SHALL rebuild the registry from disk and swap it in, answering `200 { "count": <served>, "broken_count": <refused> }`, including when entries were refused. When the folder cannot be read it SHALL answer `500 Internal` and keep the previously loaded registry. Every refused entry SHALL also be logged as a warning.

#### Scenario: Reload reports refused files instead of failing

- **WHEN** an invalid file appears on disk and reload is called
- **THEN** the response is `200` with `broken_count` counting the refused file
- **AND** `GET /api/templates` lists it under `broken`

#### Scenario: An unreadable folder keeps the live set

- **WHEN** the templates folder cannot be read and reload is called
- **THEN** the response is `500 Internal` and the previous templates stay served
