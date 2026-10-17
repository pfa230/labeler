## Why

The registry reload route shadows the valid template id `reload`: a create can answer successfully without creating a file, and literal detail, replacement and deletion requests cannot reach that template. Move registry reload outside the template-id namespace so all valid ids work through the public REST API.

Source issue: https://github.com/pfa230/labeler/issues/428

Fixes #428

## What Changes

- **BREAKING** Use `POST /api/template-registry/reload` for registry reload, preserving its response and failure behavior.
- Make the literal `/api/templates/reload` path follow the ordinary template create/read/replace/delete contract for id `reload`.
- Update OpenAPI, the MCP adapter's reload target and current operator documentation; remove the MCP template-id escaping used solely to bypass the collision.
- Cover the literal REST lifecycle and independent registry reload, reusing existing MCP coverage.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `templates`: Change the Reload endpoint.
- `mcp`: Point `reload_templates` at the registry endpoint; its tool name, arguments and results retain their contracts.

## Impact

Implementation touches route registration and the reload OpenAPI annotation in `src/api.rs`, the MCP adapter in `src/bin/labeler-mcp.rs`, HTTP and MCP tests, the startup diagnostic in `src/main.rs`, and current instructions in `AGENTS.md` and `docs/`. Existing browser template requests already use the ordinary id paths. No dependency, persistence, rendering or UI interaction change is needed.

## Non-Goals

No additional registry operations, route-registration refactor, new reserved identifiers, compatibility endpoint, redirect, body-based dispatch or API versioning. Historical archived changes stay historical. The complete request in #428 is included; no follow-up issue is needed.
