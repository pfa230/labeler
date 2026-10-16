## MODIFIED Requirements

### Requirement: Generated links use the public URL

Every URL Labeler builds for a person to open (a browsed row's `url` and the `item_url` and `location_url` fields) SHALL be `<origin>/item/<url-encoded id>` for a row of the `entities` resource and `<origin>/location/<url-encoded id>` for a row of the `locations` resource, where `<origin>` is the connection's `public_url` when it is non-blank and its `base_url` otherwise. Requests to the upstream SHALL always use `base_url`.

#### Scenario: Public URL set

- **WHEN** a connection has `base_url` `http://homebox:7745` and `public_url` `https://homebox.example.com`, and entity `e1` is browsed or materialized
- **THEN** its `url` and `item_url` are `https://homebox.example.com/item/e1`, and the upstream request went to `http://homebox:7745`

#### Scenario: Location link

- **WHEN** a connection has no `public_url` and `base_url` `http://homebox:7745`, and location `l1` is browsed or materialized
- **THEN** its `url` and `location_url` are `http://homebox:7745/location/l1`
