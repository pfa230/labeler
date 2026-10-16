## MODIFIED Requirements

### Requirement: Settings endpoints

`GET /api/settings` SHALL return every known key as `{value, is_default}`, an unset key reporting its default with `is_default: true`, and SHALL fail `500 Internal` if a stored override no longer parses. `PUT /api/settings/{key}` with `{"value": <json>}` SHALL validate, store and answer `200 {value, is_default: false}`, or `400` reason `setting_value_invalid`. `DELETE /api/settings/{key}` SHALL drop the override with `204`, idempotently. An unknown key SHALL be `404 NotFound`.

| Key | Value | Default | Rule |
|---|---|---|---|
| `datetime_formats` | object name → strftime pattern | see below | a write replaces the whole map |
| `default_printer_id` | string | unset (`null`) | see `printing` |

#### Scenario: Override and reset

- **WHEN** a client PUTs `{"value": {"day": "%d"}}` to `datetime_formats`, then DELETEs it
- **THEN** the PUT answers `{"value": {"day": "%d"}, "is_default": false}` and `GET /api/settings` then reports the five default formats with `is_default: true`

#### Scenario: Wrong type

- **WHEN** a client PUTs `{"value": "%d"}` to `datetime_formats`
- **THEN** the response is `400` with reason `setting_value_invalid`

## ADDED Requirements

### Requirement: Persisted inventory connection cleanup

An upgrade SHALL remove stored inventory connection records, their credentials and the default inventory connection override, while preserving all other application state.

#### Scenario: Upgrade with configured inventory and printer

- **WHEN** an existing installation has stored inventory connections, a default inventory connection override, printers, a default printer, variables, datetime formats, users, API tokens, sessions, favorites and print history
- **THEN** the inventory connections, their credentials and the default inventory connection override are absent after the upgrade
- **AND** the printers, default printer and all other stored state retain their values
