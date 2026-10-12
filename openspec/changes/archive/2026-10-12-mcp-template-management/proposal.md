## Why

Agents connected to labeler's MCP server can use existing templates but cannot author, edit or repair them. Full access to the existing template API lets an agent complete the create, inspect, edit, preview and delete workflow through the same connection.

Fixes #427

## What Changes

- Add seven tools: `get_template`, `get_template_source`, `get_template_thumbnail`, `create_template`, `replace_template`, `delete_template` and `reload_templates`.
- Cover every operation of the template REST API, alongside the existing template listing, filled preview and print tools.
- Read stored YAML without losing comments; create and edit by sending complete raw YAML bodies, preserving the service's validation and write semantics.
- Return thumbnails as inline PNG images, including one slot for sheet templates; expose reload counts and successful deletion.
- Publish mutation annotations, preserve REST errors and avoid automatic replay of writes.
- Document an end-to-end authoring and broken-template repair workflow.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `mcp`: Extend the tool surface from four to eleven tools and define template detail, source, thumbnail, mutation and reload results.

## Impact

- Extend `src/bin/labeler-mcp.rs` using the existing SDK, credentials, HTTP client and REST error handling.
- Extend spawned-process MCP tests and real-service integration tests; update `docs/MCP.md`.
- Reference the existing `templates`, `auth` and `errors` contracts. Template parsing, storage, rendering and the REST API retain their existing rules.
- No new dependencies or deployment settings are needed. Repository catalog browsing remains a browser workflow; catalog YAML can be installed with the create tool after the client obtains it.
