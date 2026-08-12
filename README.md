# Labeler

A REST service that renders labels from declarative YAML templates. It produces a single label as PNG
(for continuous-roll printers) or a sheet of labels as PDF (for pre-cut label sheets), by generating
[Typst](https://typst.app/) source on the fly and compiling it in-process.

## Quickstart (Docker)

```bash
docker run -p 8080:8080 ghcr.io/pfa230/labeler:edge
```

## Run

```bash
cargo run            # serves on 0.0.0.0:$PORT (default 8080)
```

## Web UI

A React + TypeScript SPA in `ui/` (Vite, Tailwind). The backend serves its build at `/`.

```bash
npm --prefix ui install            # once
npm --prefix ui run dev            # Vite dev server (proxies /api to cargo run on :8080)
npm --prefix ui run build          # build to ui/dist (then `cargo run` serves it at /)
```

In production the binary serves `ui/dist`; the Docker multi-stage build bundles the UI (see Deployment
below).

## Deployment

Run the whole thing with Docker:

```bash
docker compose up -d --build      # serves on http://localhost:${HOST_PORT:-8080}
```

See [`docs/DEPLOY.md`](docs/DEPLOY.md) for configuration, persistent volumes and backups, and CUPS/IPP
printing setup.

YAML templates are loaded from `{config}/templates/` at startup; an invalid template stops the
service from starting.

**A new install starts with no templates.** Install what you need from the catalog in the UI
(Labels → Browse the catalog), or paste YAML. The catalog lives in this repo under `catalog/`,
organised by media class and vendor: the Brother continuous-tape set `brother_9mm` / `brother_12mm` /
`brother_18mm` / `brother_24mm`, and `avery5163`, ten 2x4 inch labels per US Letter sheet. Every one
takes a single text field named `message`, so an import maps one column and works against any of
them. Your browser downloads the entry and the server validates and stores it — the server itself
never reaches out, so air-gapped deployments paste YAML instead.

**Writing your own.** [`docs/AUTHORING.md`](docs/AUTHORING.md) walks the layout model through worked
examples: coordinates, auto-length tape widths, `auto` sizing, edge-relative placement, containers,
options, and a troubleshooting table.

**On other tape widths.** Copy the closest tape template and change three things: `format.height`
(the printable height, narrower than the nominal tape), `format.media_width` (the nominal width, used
for print preflight), and the `font_size` range. Templates demonstrating engine features — QR
layouts, multiline, sheet options and rotation, variable interpolation — are not in the catalog; they
live in `tests/fixtures/templates/` and are worth reading when authoring your own.

## Endpoints

All routes are under `/api` (the root is reserved for the web UI); unknown `/api/*` → `404 NotFound`.

- `GET /api/health` → `{ "status": "ok" }`
- `GET /api/templates` → list of template summaries
- `GET /api/templates/{id}` → detailed template schema
- `GET /api/templates/{id}/source` → raw stored template YAML
- `POST /api/render/label` → rendered PNG/PDF for a single template (preview / one-off)
- `POST /api/batch` → render/print a batch (single → ZIP or per-label jobs, sheet → paginated PDF or job)
- `GET /api/openapi.json` → OpenAPI document
- `GET /api/docs/` → Swagger UI

`scripts/render_avery_sheet.sh` posts a sample request to a running server and writes a PDF. All
`/api` routes require authentication (ADR-0017), so export `LABELER_API_TOKEN` (create one in the UI
under Settings) before running it; the script sends it as `Authorization: Bearer $LABELER_API_TOKEN`.

## Development

```bash
cargo fmt
cargo clippy --all-targets --all-features
cargo test
```

[`CONTRIBUTING.md`](CONTRIBUTING.md) has the contributor workflow. [`docs/AUTHORING.md`](docs/AUTHORING.md)
is the guide to writing templates; the full API and template spec is in [`docs/SPEC.md`](docs/SPEC.md);
design decisions are recorded as [ADRs](docs/adr/).
