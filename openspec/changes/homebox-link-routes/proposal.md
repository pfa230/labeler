Fixes #447

## Why

Labeler links every Homebox row to `<url>/entity/<id>`, a page Homebox does not serve. Homebox's frontend routes are `/item/<id>` and `/location/<id>` (deployed v0.26.2 and v0.27.0-rc.1), and its own label maker encodes the same. Every printed QR code and every browse-row link therefore opens Homebox's not-found page.

## What Changes

- A row of the `entities` resource links to `<public_url|base_url>/item/<url-encoded id>`; its `url` and `item_url` carry that link.
- A row of the `locations` resource links to `<public_url|base_url>/location/<url-encoded id>`; its `url` and `location_url` carry that link.
- **BREAKING**: `/entity/<id>` links are no longer produced. Labels already printed keep their broken links.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `connections`: "Generated links use the public URL" names the per-resource Homebox route instead of `/entity/`.

## Impact

`src/connector/homebox.rs` (link construction for browse rows and materialized `item_url`/`location_url`), its unit tests, and the integration tests in `src/lib.rs` that assert `/entity/e1`.
