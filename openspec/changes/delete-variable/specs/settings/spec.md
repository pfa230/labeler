## MODIFIED Requirements

### Requirement: Variables store

`GET /api/variables` SHALL return all variables as a JSON object of strings ordered by key. `PUT /api/variables/{key}` with `{"value": "<string>"}` SHALL upsert one and echo the body with `200`; a key that is empty or outside `[A-Za-z0-9_.-]` SHALL be `400` reason `variable_key_invalid`. `DELETE /api/variables/{key}` SHALL remove one with `204` whether or not it was stored. Templates read them as `{vars.<key>}` (see `interpolation`); settings are never interpolated.

#### Scenario: Bad key

- **WHEN** a client sends `PUT /api/variables/base%20url`
- **THEN** the response is `400` with reason `variable_key_invalid`

#### Scenario: Delete a variable

- **WHEN** the store holds `site` and a client sends `DELETE /api/variables/site`, then sends it again
- **THEN** both answer `204` and `GET /api/variables` no longer includes `site`

### Requirement: Settings UI

The Settings page SHALL show Variables, Datetime formats, then Printers (`printing`), Users and API tokens (`auth`). Variables lists stored keys plus `qr_base_url` marked "(suggested)" when unset, sorted, each row saving separately; each stored row offers Remove, which deletes the variable only after an inline Confirm; adding refuses an invalid key or one already shown. Datetime formats lets the user add, edit and remove formats, shows a sample of each pattern or why it is invalid, and restores the defaults when overridden.

#### Scenario: Suggested variable

- **WHEN** no variables are stored and the user opens Settings
- **THEN** a `qr_base_url` row marked "(suggested)" is shown

#### Scenario: Remove a variable

- **WHEN** the user presses Remove on the stored `site` row
- **THEN** nothing is deleted until the user presses Confirm on that row, after which `site` is deleted and its row disappears
