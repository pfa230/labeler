# Labeler (scaffold)

Dummy REST service skeleton for the label rendering project. It exposes the planned API surface and
returns structured errors while the rendering pipeline is not yet implemented.

## Run

```bash
cargo run
```

## Endpoints

- `GET /health` → `{ "status": "ok" }`
- `GET /openapi.json` → OpenAPI document
- `GET /docs` → Swagger UI
- `GET /templates` → list of template summaries
- `GET /templates/{id}` → detailed template schema
- `POST /render/label` → returns `501 Not Implemented`
- `POST /render/batch` → returns `501 Not Implemented`

## Templates

YAML templates are loaded from the `templates/` directory on startup. Invalid templates will stop the
service from starting.

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
