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

`GET /api/settings` SHALL return every known key as `{value, is_default}`, an unset key reporting its default with `is_default: true`, and SHALL fail `500 Internal` if a stored override no longer parses. `PUT /api/settings/{key}` with `{"value": <json>}` SHALL validate, store and answer `200 {value, is_default: false}`, or `400` reason `setting_value_invalid`. `DELETE /api/settings/{key}` SHALL drop the override with `204`, idempotently. An unknown key SHALL be `404 SettingNotFound` with `details.setting`.

| Key | Value | Default | Rule |
|---|---|---|---|
| `job_log_retention_days` | integer | `90` | 0 to 4294967295; `0` disables pruning |
| `datetime_formats` | object name → strftime pattern | see below | a write replaces the whole map |
| `max_label_dimension_mm` | number | `1000` | positive, finite; enforced as `layout` specifies |
| `default_connection_id` | string or `null` | `null` | see `connections` |

#### Scenario: Override and reset

- **WHEN** a client PUTs `{"value": 30}` to `job_log_retention_days`, then DELETEs it
- **THEN** the PUT answers `{"value": 30, "is_default": false}` and `GET /api/settings` then reports `{"value": 90, "is_default": true}`

#### Scenario: Wrong type

- **WHEN** a client PUTs `{"value": "30"}` to `job_log_retention_days`
- **THEN** the response is `400` with reason `setting_value_invalid`

### Requirement: Job log pruning

The server SHALL delete job-log rows older than `job_log_retention_days` days at startup and every 24 hours after, reading the live value each time.

#### Scenario: Shortened retention

- **WHEN** retention is set to `30` while 45-day-old jobs exist
- **THEN** the next prune deletes them without a restart

### Requirement: Datetime formats

`datetime_formats` SHALL default to `iso_date` `%Y-%m-%d`, `iso_date_time` `%Y-%m-%d %H:%M`, `short_date` `%m/%d/%Y`, `long_date` `%B %-d, %Y` and `time` `%H:%M`. A write MUST use names matching `[A-Za-z0-9_]+` and string patterns chrono accepts as strftime, else `400 setting_value_invalid`. `POST /api/datetime-formats/preview` with `{"pattern"}` SHALL answer `200 {"sample"}`, the pattern applied to the server's local time now, or `400` reason `datetime_pattern_invalid`. Template use belongs to `interpolation`.

#### Scenario: Invalid pattern

- **WHEN** a client previews `{"pattern": "%!"}`
- **THEN** the response is `400` with reason `datetime_pattern_invalid`

### Requirement: Settings UI

The Settings page SHALL show Variables, Job log, Datetime formats, then Printers (`printing`), Users and API tokens (`auth`). Variables lists stored keys plus `qr_base_url` marked "(suggested)" when unset, sorted, each row saving separately; adding refuses an invalid key or one already shown. Job log and Datetime formats mark an unset setting "(default)" and offer Reset to default only when overridden. Datetime formats edits name/pattern rows (Add format, Remove), shows each pattern's preview sample or error 400 ms after typing stops, and saves the whole map without rows whose name is empty.

#### Scenario: Suggested variable

- **WHEN** no variables are stored and the user opens Settings
- **THEN** a `qr_base_url` row marked "(suggested)" is shown

### Requirement: Favorites

Favorites and recents SHALL be per actor: the user id, `token:{id}` for an API token, or `local` in no-auth mode. `GET /api/favorites` SHALL return the caller's ids in favoriting order (same-second ties by id), omitting ids not in the registry. `PUT /api/favorites/{template_id}` SHALL add one idempotently with `204`, or `404 TemplateNotFound`. `DELETE` SHALL remove one with `204` whether or not it existed. Deleting a template unfavorites it for everyone (see `templates`).

#### Scenario: Favorites are per user

- **WHEN** user A favorites `pallet`
- **THEN** user B's `GET /api/favorites` does not include it

### Requirement: Recent templates

`GET /api/recent-templates?limit=N` SHALL return the distinct templates of the caller's recorded print jobs, successful or failed, most recent first (ties to the later job), `limit` defaulting to 6 and clamped to 1–20. Ids not in the registry SHALL be dropped after the limit, so fewer may return.

#### Scenario: Limit is clamped

- **WHEN** a caller who printed 25 templates asks for `limit=50`
- **THEN** 20 ids are returned

### Requirement: Favorites and Recent rows

Each Labels grid card SHALL carry a star toggle (`aria-pressed` when favorited). A Favorites row (favoriting order) and a Recent row (recency order, favorites excluded) SHALL show above the grid when non-empty, and both SHALL hide while a search term or a group other than All is active.

#### Scenario: Searching hides the rows

- **WHEN** the user types in the search box
- **THEN** neither row is shown
