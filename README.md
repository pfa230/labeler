# Labeler

A self-hosted service that renders labels from declarative YAML templates: a single label as PNG for continuous-roll printers, or a sheet of labels as PDF for pre-cut label sheets. It generates [Typst](https://typst.app/) source on the fly and compiles it in-process, and serves a web UI for printing, CSV import and inventory integrations.

## Run

```bash
docker run -p 8080:8080 ghcr.io/pfa230/labeler:edge     # or: docker compose up -d --build
```

Open http://localhost:8080. [`docs/DEPLOY.md`](docs/DEPLOY.md) covers configuration, volumes and backups, authentication and CUPS/IPP printing.

A new install has no templates. Install them from the catalog in the UI (Labels → Browse the catalog) or paste YAML. The catalog lives in this repo under `catalog/`: the Brother continuous-tape set (`brother_9mm` to `brother_24mm`) and `avery5163`, ten 2x4 inch labels per US Letter sheet. Your browser downloads the entry and the server stores it, so an air-gapped install pastes YAML instead.

## Write templates

[`docs/AUTHORING.md`](docs/AUTHORING.md) walks through writing templates by worked example. Templates demonstrating engine features (QR layouts, wrapping, `when:` branches, rotation, interpolation) live in `tests/fixtures/templates/`.

## API

Routes live under `/api`; all but health, login, setup and the API docs need authentication (see the [`auth` spec](openspec/specs/auth/spec.md)). The OpenAPI document is at `/api/openapi.json` and Swagger UI at `/api/docs/`. The full contract for the API, template schema, layout and errors is in [`openspec/specs/`](openspec/specs/). `scripts/render_avery_sheet.sh` posts a sample batch to a running server; export `LABELER_API_TOKEN` (create one under Settings) first.

## Develop

```bash
LABELER_CONFIG_DIR=./config-dev cargo run     # API and the built UI on :8080
npm --prefix ui install && npm --prefix ui run dev   # Vite dev server, proxies /api to :8080
```

[`CONTRIBUTING.md`](CONTRIBUTING.md) lists the checks to run before submitting a change.
