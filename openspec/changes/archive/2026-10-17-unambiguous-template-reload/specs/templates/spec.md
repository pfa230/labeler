## MODIFIED Requirements

### Requirement: Reload

`POST /api/template-registry/reload` SHALL rebuild the registry from disk and swap it in, answering `200 { "count": <served>, "broken_count": <refused> }`, including when entries were refused. When the folder cannot be read it SHALL answer `500 Internal` and keep the previously loaded registry. Every refused entry SHALL also be logged as a warning.

#### Scenario: Reload reports refused files instead of failing

- **WHEN** an invalid file appears on disk and reload is called
- **THEN** the response is `200` with `broken_count` counting the refused file
- **AND** `GET /api/templates` lists it under `broken`

#### Scenario: An unreadable folder keeps the live set

- **WHEN** the templates folder cannot be read and reload is called
- **THEN** the response is `500 Internal` and the previous templates stay served
