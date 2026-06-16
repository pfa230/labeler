# Labeler

A REST service that renders labels from declarative YAML templates. It produces a single label as PNG
(for continuous-roll printers) or a sheet of labels as PDF (for pre-cut label sheets), by generating
[Typst](https://typst.app/) source on the fly and compiling it in-process.

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
multi-stage build that bundles the UI is M6.

YAML templates are loaded from `templates/` at startup; an invalid template stops the service from
starting. Starter templates: `avery5163` (US Letter sheet) and `brother12mm` / `brother18mm` /
`brother24mm` (continuous tape).

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
server and write a PDF.

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

## Documentation

- [`docs/SPEC.md`](docs/SPEC.md) — full, living specification (API, template schema, layout model,
  coordinate system, options, errors).
- [`docs/adr/`](docs/adr/) — architecture decision records.

Work items are tracked as GitHub issues.

## Development

```bash
cargo fmt
cargo clippy --all-targets --all-features
cargo test
```

See [`CLAUDE.md`](CLAUDE.md) for architecture notes and contribution conventions.
