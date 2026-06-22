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

In production the binary serves `ui/dist` (override the dir with `LABELER_UI_DIR`). The Docker
multi-stage build bundles the UI (see Deployment below).

## Deployment

Run the whole thing with Docker:

```bash
docker compose up -d --build      # serves on http://localhost:${HOST_PORT:-8080}
```

See [`docs/DEPLOY.md`](docs/DEPLOY.md) for configuration, persistent volumes and backups, and CUPS/IPP
printing setup.

YAML templates are loaded from `templates/` at startup; an invalid template stops the service from
starting. Starter templates: `avery5163` (US Letter sheet) and the Brother continuous-tape set
`brother_12mm` / `brother_18mm` / `brother_24mm` (text only) plus `brother_18mm_qr` / `brother_24mm_qr`
(QR + text).

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

`scripts/render_test.sh` and `scripts/render_avery_horizontal.sh` post sample requests to a running
server and write a PDF. All `/api` routes require authentication (ADR-0017), so export
`LABELER_API_TOKEN` (create one in the UI under Settings) before running them; the scripts send it as
`Authorization: Bearer $LABELER_API_TOKEN`.

## Error model

All errors are JSON with a stable schema:

```json
{
  "error": {
    "code": "TemplateNotFound",
    "message": "No template with id 'xyz' was found",
    "details": { "template": "xyz" }
  }
}
```

## Development

```bash
cargo fmt
cargo clippy --all-targets --all-features
cargo test
```

[`CONTRIBUTING.md`](CONTRIBUTING.md) has the contributor workflow. The full API and template spec is in
[`docs/SPEC.md`](docs/SPEC.md); design decisions are recorded as [ADRs](docs/adr/).
