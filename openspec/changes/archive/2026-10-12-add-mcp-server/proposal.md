## Why

Agents must currently construct REST requests themselves to discover label templates, fill their parameters, preview output and print it. A small MCP adapter makes that existing workflow available directly to Claude Code and other MCP clients.

Fixes #421

## What Changes

- Add a separate `labeler-mcp` stdio binary that calls an existing labeler REST service.
- Publish exactly four tools: `list_templates`, `render_label`, `list_printers` and `print_labels`; template listings already include published parameters.
- Return filled single-label previews as inline MCP PNG images and batch print results as the existing print summary.
- Reuse personal bearer tokens, their user identity and the service's no-auth mode. Introduce no token scopes.
- Document building the binary and connecting Claude Code, with the real-printer demonstration assigned to human acceptance before integration.

## Capabilities

### New Capabilities

- `mcp`: Stdio connection, REST credentials, tool schemas, catalog reads, PNG previews, batch printing and MCP error results.

### Modified Capabilities

None. Existing REST authentication, template, rendering, printing and error contracts remain authoritative.

## Impact

- A new Rust binary and its adapter tests; `Cargo.toml` and `Cargo.lock` gain the official Rust MCP SDK with stdio server support and schema generation.
- Existing `reqwest`, JSON and base64 support handle REST calls and PNG content. The adapter uses the running service rather than loading templates or opening its database.
- Add setup documentation, update the domain lists in `AGENTS.md` and `openspec/config.yaml`, and create `openspec/specs/mcp/spec.md` when this change is archived.
- No UI changes, REST route changes, template editing tools, new printer drivers or new rendering behavior. Filled sheet previews keep the REST endpoint's existing refusal; sheet batches remain printable.
