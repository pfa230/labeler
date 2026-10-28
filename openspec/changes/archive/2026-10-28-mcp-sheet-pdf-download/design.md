# Design

## Context

See proposal.md for motivation. `labeler-mcp` already forwards authenticated REST requests and returns PNG content. `POST /api/render` returns a PDF for sheets and a ZIP for single-label templates. `rmcp` 3.5.1 already supplies embedded blob resources.

## Goals / Non-Goals

**Goals:** Carry the existing sheet PDF from REST to the MCP client with one render request.

**Non-Goals:** Single-label PDF export, ZIP download, new rendering behavior, filesystem output, file hosting, printer dispatch or deployment configuration.

## Decisions

- Add `render_sheet` beside `render_label`, reusing label-entry decoding and the nonnullable optional start-slot convention. Describe `start_slot` as the number of leading sheet slots to skip. Template controls such as orientation and outline are already declared parameters (`data`), not a separate options object; preserve their values per label and expose that usage in the tool/schema descriptions. Extend the shared label entry with optional positive, nonnullable `copies`, forwarding it unchanged for both `render_sheet` and `print_labels` and preserving omission. This matches the existing REST batch inputs without expanding copies in the adapter. Forward the supplied JSON through `rest_bytes` to `api/render`; omit an absent start slot. No template lookup is needed because the service already resolves and validates templates.
- Return `ContentBlock::resource` with `ResourceContents::BlobResourceContents`, base64-encoded PDF bytes, `application/pdf` and `labeler:///renders/<template>.pdf`. The URI names the embedded file; it is not a separately hosted download endpoint. A text-only response would not give clients a binary file resource; hosting links or writing local files would require storage that this request does not need. Use the MCP SDK already in the dependency graph. The official protocol defines [embedded binary resource contents](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2025-06-18/schema.mdx).
- Check the `%PDF-` prefix before constructing the resource. This distinguishes the real REST behavior for a single-label template, which returns a ZIP, without a second template request or a REST contract change. Report a tool execution error for non-PDF output; do not fabricate a service error code.
- Cover tool discovery, request forwarding, omitted and supplied start slots, binary byte preservation and existing service-error handling in `tests/mcp.rs`. Add a real-service sheet download in `tests/mcp_integration.rs` using its existing sheet fixture and fake IPP counter to establish that rendering works and dispatch stays at zero. Reuse existing helpers; no PDF parser dependency is needed to test adapter transport.

## Risks / Trade-offs

- Clients decide how to expose embedded files to users. Returning a standard MCP resource carries the bytes but does not add a client download UI.
- Base64 adds protocol payload size; this follows the adapter's existing inline-image model and avoids a new artifact service.

## Cut pass

Kept the tool/schema change to make download discoverable; the PDF resource contract to deliver file bytes; the single-template scenario and signature check because the existing REST endpoint returns ZIPs for that input; and the mock/real-service checks to establish transport and absence of physical dispatch. Existing service validation, pagination, authentication and error-envelope rules are referenced rather than duplicated. No storage, resource-read endpoint, PDF parser, rendering-engine or UI change is proposed.
