# Settings

## Purpose

State outside templates: the variables store, typed application settings and their editors, the datetime pattern preview, and per-user favorites and recent templates.

## Requirements

### Requirement: Variables store

`GET /api/variables` SHALL return all variables as a JSON object of strings ordered by key. `PUT /api/variables/{key}` with `{"value": "<string>"}` SHALL upsert one and echo the body with `200`; a key that is empty or outside `[A-Za-z0-9_.-]` SHALL be `400` reason `variable_key_invalid`. Templates read them as `{vars.<key>}` (see `interpolation`); settings are never interpolated.

#### Scenario: Bad key

- **WHEN** a client sends `PUT /api/variables/base%20url`
- **THEN** the response is `400` with reason `variable_key_invalid`

### Requirement: Settings endpoints

`GET /api/settings` SHALL return every known key as `{value, is_default}`, an unset key reporting its default with `is_default: true`, and SHALL fail `500 Internal` if a stored override no longer parses. `PUT /api/settings/{key}` with `{"value": <json>}` SHALL validate, store and answer `200 {value, is_default: false}`, or `400` reason `setting_value_invalid`. `DELETE /api/settings/{key}` SHALL drop the override with `204`, idempotently. An unknown key SHALL be `404 NotFound`.

| Key | Value | Default | Rule |
|---|---|---|---|
| `datetime_formats` | object name → strftime pattern | see below | a write replaces the whole map |
| `default_connection_id` | string | unset (`null`) | see `connections` |
| `default_printer_id` | string | unset (`null`) | see `printing` |

#### Scenario: Override and reset

- **WHEN** a client PUTs `{"value": {"day": "%d"}}` to `datetime_formats`, then DELETEs it
- **THEN** the PUT answers `{"value": {"day": "%d"}, "is_default": false}` and `GET /api/settings` then reports the five default formats with `is_default: true`

#### Scenario: Wrong type

- **WHEN** a client PUTs `{"value": "%d"}` to `datetime_formats`
- **THEN** the response is `400` with reason `setting_value_invalid`

### Requirement: Datetime formats

`datetime_formats` SHALL default to `iso_date` `%Y-%m-%d`, `iso_date_time` `%Y-%m-%d %H:%M`, `short_date` `%m/%d/%Y`, `long_date` `%B %-d, %Y` and `time` `%H:%M`. A write MUST use names matching `[A-Za-z0-9_]+` and string patterns chrono accepts as strftime, else `400 setting_value_invalid`. `POST /api/datetime-formats/preview` with `{"pattern"}` SHALL answer `200 {"sample"}`, the pattern applied to the server's local time now, or `400` reason `datetime_pattern_invalid`. Template use belongs to `interpolation`.

#### Scenario: Invalid pattern

- **WHEN** a client previews `{"pattern": "%!"}`
- **THEN** the response is `400` with reason `datetime_pattern_invalid`

### Requirement: Settings UI

The Settings page SHALL show Variables, Datetime formats, then Printers (`printing`), Users and API tokens (`auth`). Variables lists stored keys plus `qr_base_url` marked "(suggested)" when unset, sorted, each row saving separately; adding refuses an invalid key or one already shown. Datetime formats lets the user add, edit and remove formats, shows a sample of each pattern or why it is invalid, and restores the defaults when overridden.

#### Scenario: Suggested variable

- **WHEN** no variables are stored and the user opens Settings
- **THEN** a `qr_base_url` row marked "(suggested)" is shown

### Requirement: Favorites

Favorites and recents SHALL be per actor (see `auth`). `GET /api/favorites` SHALL return the caller's ids in favoriting order (same-second ties by id), omitting ids not in the registry. `PUT /api/favorites/{template_id}` SHALL add one idempotently with `204`, or `404 NotFound`. `DELETE` SHALL remove one with `204` whether or not it existed. Deleting a template unfavorites it for everyone (see `templates`).

#### Scenario: Favorites are per user

- **WHEN** user A favorites `pallet`
- **THEN** user B's `GET /api/favorites` does not include it

### Requirement: Recent templates

`GET /api/recent-templates` SHALL return at most 6 distinct templates from the caller's recorded print jobs, most recent first (ties to the later job); ids not in the registry SHALL be dropped after the limit, so fewer may return.

#### Scenario: Six most recent

- **WHEN** a caller has printed 8 distinct templates, all still served
- **THEN** the 6 most recently printed ids are returned, most recent first

### Requirement: Favorites and Recent rows

Each Labels grid card SHALL carry a star toggle (`aria-pressed` when favorited). A Favorites row (favoriting order) and a Recent row (recency order, favorites excluded) SHALL show above the grid when non-empty, and both SHALL hide while a search term or a category other than All is active.

#### Scenario: Searching hides the rows

- **WHEN** the user types in the search box
- **THEN** neither row is shown
