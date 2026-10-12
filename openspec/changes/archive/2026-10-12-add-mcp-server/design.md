## Context

See `proposal.md` for motivation and `specs/mcp/spec.md` for the new contract. Existing REST endpoints already supply template summaries/details, filled single-label PNGs, configured printers and batch dispatch. `reqwest`, `serde_json` and `base64` are dependencies today. Personal tokens act as their owning user; token scopes do not exist (`auth`, API tokens). Printer response models omit passwords (`src/models.rs`, `Printer`).

The existing single-label endpoint refuses sheet templates. This proposal preserves that boundary rather than adding a rendering capability to an MCP integration issue. Sheets can still be printed through the existing batch endpoint.

## Goals / Non-Goals

**Goals:** Keep the MCP layer a protocol and HTTP adapter; let the REST service own validation, rendering, printer connections and actor attribution. Make the issue's local Claude Code workflow demonstrable with a real client and a controlled printer.

**Non-Goals:** In-process template loading, direct IPP, another auth store, scoped tokens, template mutation, generic REST proxy tools, resources/prompts, file persistence and another deployment daemon.

## Decisions

### Separate stdio binary

Add `src/bin/labeler-mcp.rs` with a small adapter module only as needed for tests. Use the official Rust MCP SDK (`rmcp`) for initialization, stdio framing, tool routing and schema generation. Choose a release compatible with the repository's pinned Rust compiler and commit its resolved lockfile; enable only server, stdio and the tool/schema features used.

An embedded Streamable HTTP endpoint could share axum's existing port and bearer authentication. The separate stdio binary instead leaves the service, Docker image and middleware intact and avoids adding HTTP MCP Origin handling and transport-session behavior. Claude Code already launches stdio servers, and the wrapper can reach either a local or remote REST service. These are the reasons to choose stdio for the acceptance client. Hand-written JSON-RPC would replace one dependency with protocol code the project would need to maintain.

Use `LABELER_URL` as an HTTP(S) service origin, defaulting to the existing local origin. Allow only the root path (empty or `/`), rejecting userinfo, query and fragment. This deliberately avoids path-prefix configuration nobody has requested. Parse the origin once and construct fixed `/api` URLs. Use `LABELER_TOKEN` only to set the bearer header; do not print environment values or include authorization headers in diagnostics. No cookie jar or login operation is needed. An absent token delegates the decision to REST and works with its no-auth mode.

### Four explicit tools

The spec's operation table is the entire surface. `list_templates` already returns each template's published `params`, so it supplies both discovery and parameter reads. A separate detail tool would only add `variables`, which the acceptance workflow does not need. The adapter does not derive schema or defaults from layouts. `data` is an arbitrary JSON object so list values, booleans, numbers and names such as `__proto__` survive forwarding. Typed tool argument schemas describe the REST wire fields; do not duplicate REST's domain validation. Keep optional `start_slot` absent when omitted and reject explicit null, matching its REST spelling.

Template mutation is excluded because discovery, preview and printing satisfy the acceptance workflow; the existing REST API and UI already cover authoring. Adding mutation tools would expand the agent's surface to rewriting shared templates without an acceptance need.

Use JSON text for catalog and summary responses. It works without output-schema duplication or wrapping arrays in invented result envelopes. Inline image content makes the preview directly visible to a client, without writing files or serving resource URLs. Check the successful preview's PNG signature before constructing image content; wrong-content responses must fail visibly. A second check of the HTTP content-type header adds no needed behavior.

A separate `copies` argument is unnecessary: repeated entries in `labels` already express identical labels. A preview is a separate read operation; printing does not require a stored preview or confirmation receipt. Client approval policy owns the decision to invoke the print tool. Its annotations and description explicitly identify the physical effect.

### Preserve operational errors

Wrap REST failures with their status and unchanged service error object in an execution-error result. A `200` print summary with failures remains the original summary but sets `isError` so the agent sees partial failure. Missing required tool arguments, unlisted argument keys and invalid values also return execution errors with `isError: true` before any REST call. Use SDK protocol errors for unknown tools and malformed `CallToolRequest` envelopes. Handle tool-specific argument deserialization at a boundary where its failures can be converted into execution results while leaving envelope errors to the protocol layer. Network failures or unexpected bodies produce explanatory execution errors without adding service reason slugs.

Disable HTTP automatic retries for print submission; one invocation makes at most one print POST. Do not add an adapter-wide request deadline or retry configuration: batch runtime scales with label count, and a timeout cannot prove that printing did not occur. A lost response must say the dispatch outcome is unknown. This does not provide exactly-once printing across separate client invocations.

### Prior art and protocol references

Surveyed on 2026-10-11, with primary README/source confirmation:

| Project | Observed surface | Effect on this proposal |
|---|---|---|
| [label-printer-plugin server source](https://github.com/danielrosehill/label-printer-plugin/blob/main/mcp-server/server.py) | `preview_label`, `print_label`, `print_batch`, printer status/capabilities and image tools; configurable transport, default Streamable HTTP | Separate pure preview from physical print; reuse labeler's renderer rather than adopting its Brother/USB bridge. |
| [canon-mcp README](https://github.com/cwdcwd/canon-mcp#readme) | `printer_status`, `print_file`, `print_jobs`, `cancel_print_job`; stdio and Streamable HTTP entrypoints; inline image delivery and destructive print annotations | Stdio is an established local-client route; inline image results and explicit print effects fit this workflow. |

Neither project's observed interface supplies labeler's YAML parameter contract. The tool arguments therefore follow labeler's REST models rather than copying printer-specific text, file or copy options.

Protocol and setup sources: [MCP stdio transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports), [MCP tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools), [official Rust SDK](https://github.com/modelcontextprotocol/rust-sdk), and [Claude Code MCP setup](https://code.claude.com/docs/en/mcp). Negotiate versions supported by the selected SDK and client rather than assuming a particular latest version.

## Risks / Trade-offs

- A native client-side binary needs installation: document `cargo build --release --locked --bin labeler-mcp` and an absolute binary path in the Claude Code stdio setup. Building a binary in the existing package still compiles its Typst dependencies; a separate package would add build structure beyond this local acceptance need. Container distribution and release installers are outside this issue's local acceptance path.
- Filled sheet previews are unavailable: describe the limitation in `render_label` and propagate REST's refusal; exercise the acceptance preview with a single-label template.
- A token retains its owner's REST permissions: the exposed four tools constrain the adapter surface, not the credential's permission on REST. A separately named personal token makes revocation straightforward; introducing scopes would require a separate auth change.
- Large PNGs cost base64 overhead: return the existing PNG directly; do not add image scaling or file-storage fallback without an observed client limit.
- Lost print responses can lead to client-initiated duplicate printing: make no automatic retry, report uncertainty and retain the service's distinction between accepted jobs and physical output.

## Migration Plan

Build the additional binary and run the existing labeler service. Create a named personal API token through its existing UI, then configure Claude Code locally with `LABELER_URL` and `LABELER_TOKEN`, or omit the token for a local no-auth service. Documentation uses placeholders; credentials and machine configuration stay outside git. Removing that client registration and binary rolls back the integration without changing server state.

The loop owns automated protocol and service integration tests using a fake IPP receiver, plus documentation and build/registration command checks. As part of introducing the new spec domain, it also updates the domain lists in `AGENTS.md` and `openspec/config.yaml` to include `mcp`.

Human acceptance is separate from the loop's task checklist. Before approving the implementation PR for integration, the person reviewing it connects local Claude Code to labeler, discovers templates with their published parameters and configured printers, requests a filled single-label PNG and prints that label to a designated real printer. The person confirms the physical output; an accepted-job response alone cannot establish that a label emerged. Record the client results and the person's acceptance in the PR review, keeping redacted local transcripts and render artifacts under `.agent-runs/`. The fake IPP tests do not satisfy this part of #421's acceptance, and it must not be claimed complete before that review.
