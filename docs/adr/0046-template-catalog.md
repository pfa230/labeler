# 46. A template catalog replaces first-run seeding

Date: 2026-08-08

## Status

Accepted. Supersedes the first-run seeding decision in
[ADR-0034](0034-single-config-dir.md) §2 (the single-config-dir decision itself stands).
Issue [#137](https://github.com/pfa230/labeler/issues/137); follow-ups
[#135](https://github.com/pfa230/labeler/issues/135) (what the catalog should contain) and
[#138](https://github.com/pfa230/labeler/issues/138) (moving it to its own repo).

## Context

`include_dir!` embedded `templates/` in the binary and `seed_templates_once` wrote all of it into
`{config}/templates/` on first run. Everyone got four Brother tape templates whether or not they
owned a Brother printer, deleting one had no route back, and adding a template needed a release.

Surveying how other self-hosted apps ship optional content separated two models. **Content imports**
— Home Assistant blueprints, Grafana dashboards, Zabbix templates, n8n workflows — become ordinary
editable objects owned by the user. **Package installs** — Obsidian, VS Code, Jellyfin, Homebridge
plugins — keep a package identity and version so the app can update and roll back code it owns, with
user settings stored separately.

Labeler templates are content imports: the installed artifact *is* the user's editable YAML, and
there is no code/settings split to exploit.

## Decision

**A catalog in this repo, installed from, seeded never.** `catalog/<media-class>/<vendor>/*.yaml`
(`tape/brother`, `sheet/avery`, plus `examples/` for templates that demonstrate an engine feature
rather than being printed as-is), with a CI-generated `catalog/index.json`.

**The browser downloads; the server validates and stores.** The UI fetches the index and the YAML
from `raw.githubusercontent.com` (which sends `access-control-allow-origin: *`) and POSTs to the
existing `POST /api/templates`. **No new backend endpoints, no server-side fetch.** Rejected
alternatives: a server-side fetch centralises checksums and audit but adds outbound requests for
user-selected content — the SSRF shape `egress.rs` exists to screen — and still needs an upload path
for air-gapped installs, two code paths where this needs zero; embedding the catalog in the image
keeps air-gap identical but ships every template to everyone and couples catalog fixes to the release
cadence, which is the complaint #137 opens with.

**No provenance is recorded.** Nothing tracks where a template came from. Installed is "the id
appears in `GET /api/templates`"; changed is "compare with `GET /api/templates/{id}/source`". This
deletes a table, a migration story for already-seeded installs, and upkeep in every write path — the
package-install bookkeeping does not apply to a content import.

**Compatibility is validate-before-write.** A template using syntax this server does not understand
fails `422` and installs nothing. Accepted limitation: on an older server every catalog entry still
*looks* installable and fails per click, so the UI reports it as "needs a newer version of labeler"
rather than a raw validation error. `min_server` metadata is deliberately not built — it is the
Jellyfin `targetAbi` shape, a floor only as good as whoever remembers to bump it.

**Ids stay globally unique across the tree and equal their filename stem**, both CI-enforced. An id
is the API key, the `/print/{id}` route and what print webhooks hardcode, and installs land flat in
`{config}/templates/{id}.yaml`, so nesting cannot namespace them.
`TemplateRegistry::load_from_dir` stays flat; recursion belongs only to catalog tooling.

## Consequences

- **A new deployment starts with zero templates.** That is the point, and it is why the empty state on
  Labels, Import and Connect is part of this change rather than a follow-up — Import and Connect
  previously rendered an empty `<select>` with nowhere to go.
- Existing installs are untouched: their templates were seeded already and are user-owned. The
  `templates_seeded` flag row stays in their databases, simply unread.
- `src/bundled.rs`, `seed_templates_once`, the `include_dir` dependency and the Dockerfile's
  `COPY templates/` are gone. `catalog/` is deliberately **not** copied into the image.
- `useCreateTemplate` had to start throwing `ApiError`: it discarded `res.status`, so `409` and `422`
  — the two outcomes the install flow branches on — were indistinguishable.
- CI gains three gates: every catalog entry parses, validates and renders; ids are unique and match
  filenames; and `catalog/index.json` is regenerated and diffed, so a template added without running
  the generator fails there rather than shipping an index that omits it.
- Adding a template still needs a release *of the catalog directory in this repo*, which is only an
  improvement once #138 moves it out. This decision is deliberately the smaller step.
