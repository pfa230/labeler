# Labeler

A REST service that renders labels from declarative YAML templates. It produces a single label as PNG
(for continuous-roll printers) or a sheet of labels as PDF (for pre-cut label sheets), by generating
[Typst](https://typst.app/) source on the fly and compiling it in-process.

## Run

```bash
cargo run            # serves on 0.0.0.0:$PORT (default 8080)
```

YAML templates are loaded from `templates/` at startup; an invalid template stops the service from
starting. Starter templates: `avery5163` (US Letter sheet) and `brother12mm` / `brother18mm` /
`brother24mm` (continuous tape).

## Endpoints

- `GET /health` → `{ "status": "ok" }`
- `GET /templates` → list of template summaries
- `GET /templates/{id}` → detailed template schema
- `POST /render/label` → rendered PNG (templates with `format.type: single`)
- `POST /render/batch` → rendered PDF (templates with `format.type: sheet`)
- `GET /openapi.json` → OpenAPI document
- `GET /docs` → Swagger UI

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
