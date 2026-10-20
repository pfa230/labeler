# Spec Delta

## MODIFIED Requirements

### Requirement: Template catalog index

`catalog/` in the repository SHALL hold installable templates at `catalog/<category>/<file>.yaml` or `catalog/<category>/<vendor>/<file>.yaml`. `cargo run --bin catalog-index` SHALL parse and validate each with the server's rules, failing on an invalid template or any other path shape, and write `catalog/index.json`: a JSON array, in ascending path order, of entries `{ id, name, description, path, category, vendor, format, position_count?, media_width_mm, fields }`, where `id` is the filename stem, `path` is relative to `catalog/`, `vendor` is `null` for the two-level shape, `format` is `single` or `sheet`, `position_count` is present only for sheets and equals the validated template's `positions` count, `media_width_mm` is `format.media_width` or `null`, and `fields` lists the names of the template's declared parameters in declaration order. CI SHALL fail when the committed index differs from the regenerated one.

#### Scenario: A stale index fails CI

- **WHEN** a catalog template changes and `catalog/index.json` is not regenerated
- **THEN** the CI catalog step fails

#### Scenario: Sheet counts come from validated positions

- **WHEN** the generator indexes a validated sheet declaring 10 positions
- **THEN** its entry carries `position_count: 10`

### Requirement: Catalog page

The UI catalog page SHALL fetch `index.json` and each template's YAML from the repository's raw-content URL in the browser; the server SHALL make no outbound request for the catalog. Entries SHALL be grouped under `category · vendor` (or `category`), headings sorted, each card showing name, description, a format badge ("Template format is shown" in `ui`), media width in mm when present, and fields (`none` when empty), marked `installed` when the id is served. **Install** SHALL send `POST /api/templates/{id}`. A `409` SHALL open a dialog showing the installed source beside the catalog YAML, offering **Replace** (a `PUT`) or **Cancel**; when the installed source cannot be fetched, the page SHALL instead report that it could not be read and open no dialog. A `422` SHALL report that the template needs a newer version of labeler. When the catalog cannot be fetched, the page SHALL say so and link to pasting YAML at `/templates/new`.

#### Scenario: Installing an already installed template offers a diff

- **WHEN** the user installs an entry whose id is already served
- **THEN** the `POST` answers `409`
- **AND** the page shows both versions and replaces only when the user confirms

#### Scenario: An unreadable installed file is reported

- **WHEN** the `POST` answers `409` and `GET /api/templates/{id}/source` answers `404`
- **THEN** the page reports that the installed template could not be read and sends no `PUT`

#### Scenario: An unreachable catalog falls back to paste

- **WHEN** the browser cannot fetch `index.json`
- **THEN** the page says the catalog could not be reached and links to `/templates/new`
