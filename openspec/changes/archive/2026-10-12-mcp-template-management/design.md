## Context

See proposal.md for the requested workflow. The merged adapter already has SDK tool discovery, strict argument deserialization, bearer forwarding, JSON/error results, PNG content and a client with redirects and retries disabled. The template REST API already implements every required operation. Extend that adapter; `templates`, `auth` and `errors` remain authoritative.

## Goals / Non-Goals

**Goals:** An MCP client can use every existing template operation and combine them into authoring and repair workflows. Tool results preserve the service's source, metadata, images and errors.

**Non-Goals:** Additional template syntax, structured layout editors, server-side catalog downloads, non-template administration and new REST routes. Catalog installation is creation with client-supplied catalog YAML; cloning is source read followed by creation under another id.

## Decisions

### One tool per existing template operation

Add the seven tools in the delta's Tool surface table, for eleven tools in total. Names use the existing `template` argument spelling. Editing is `get_template_source` followed by `replace_template` with the entire edited YAML; no patch language or second edit spelling is needed.

`get_template` remains distinct from source: detail carries published parameters and referenced variables, while source carries layout and comments and works for quarantined files. A combined read would make source repair depend on a detail request that legitimately fails for broken templates. Thumbnail is distinct from filled preview because REST provides placeholder-based one-slot thumbnails for sheets, while filled PNG preview rejects sheets.

### Reuse REST validation and send raw YAML

Create and replace accept `{template, yaml}` and send exactly that string as `text/yaml`. Extend the existing HTTP request path only enough to support raw bodies and no-content success; reuse its credentials and error mapping. Parsing or serializing YAML in the adapter would discard comments or duplicate server validation. A typed template-editing model would also have to track every template-domain change.

Use the existing template-id validator for path-based arguments and describe the same rule in discovery. YAML content itself is validated by REST, including empty or malformed strings. Create never falls back to replace, and replace never falls back to create.

### Make identifiers unambiguously address templates

Escape the first byte of a validated ASCII identifier as `%HH` and retain its remaining allowed characters in the same path segment, leaving the fixed API path and suffix untouched. Standard interpolation, including ordinary encoders that leave unreserved letters literal, causes the valid id `reload` to hit the static registry-reload route rather than creation. One escaped byte prevents a literal static-path match; escaping the whole identifier adds no required behavior.

An isolated probe confirmed literal POST `/api/templates/reload` returns reload counts without creating a file, and literal detail/replace/delete return 405. Using `%72eload` makes those same handlers return the expected 201/200/204. Keep evidence in `.agent-runs/template-route-probe.json`; the underlying REST routing bug is tracked separately as [#428](https://github.com/pfa230/labeler/issues/428). Reuse one identifier builder for all six path-based template tools and verify the actual SDK/reqwest request against the real service, not only a mocked URI.

This adds no reserved-id rule, fallback request or dependency. The existing REST handler still receives the original identifier.

### Keep each response in its native MCP form

Detail, create, replace and reload preserve their original JSON in text content, including extra response fields. Source is one exact UTF-8 text block; reject invalid UTF-8 rather than replacing bytes. Thumbnail shares the existing PNG signature check and image encoding. Delete's 204 is successful empty content, avoiding an invented JSON acknowledgement or a JSON decode failure. Thumbnail GET is unconditional; HTTP cache headers and client-side image caching are unnecessary for this workflow.

### Advertise effects and retain one-request delivery

Read tools advertise read-only behavior. All mutation tools use the existing print tool's conservative annotation tuple: read-only false, destructive true and idempotent false. Even a create reloads the live registry, which can remove entries whose on-disk files became invalid; it cannot promise solely additive effects. Descriptions explain the particular operation and unknown outcomes after lost responses.

The annotation meanings come from the [MCP ToolAnnotations schema](https://modelcontextprotocol.io/specification/2025-11-25/schema#toolannotations); they inform clients rather than enforce authorization. Existing bearer authentication authorizes the request. Reuse the client's disabled redirects and retries and add mutation-specific transport tests; introduce no confirmation flag, token scope or compensating write.

## Risks / Trade-offs

- A replacement uses the service's last-writer-wins complete-source semantics. Document read/edit/replace clearly; a new merge or concurrency protocol would change the REST contract.
- A write or reload response can be lost after the effect. Report the unknown outcome and allow the agent to inspect source or listing before a deliberate next call; never automatically replay it.
- REST can publish a file before a subsequent reload fails. Preserve the service error rather than promising that every error rolls the mutation back.
- The existing literal `reload` collision can turn a create into another operation. An escaped identifier byte and a real-service lifecycle regression cover it without widening this issue into REST routing changes.

## Validation and acceptance

Spawn the actual adapter and test discovery, argument rejection, method/path/body fidelity, bearer headers, exact response content, error mapping and one-request delivery. Extend existing process helpers and fake REST services rather than adding another harness.

Against an isolated real service, exercise create → detail/source → edit/replace → filled preview → thumbnail → delete, for ordinary ids and the valid id `reload`; also repair a quarantined file, reload served/broken counts, and fetch a sheet thumbnail. Check actual files and registry outcomes so assertions fail on lost source bytes, missing writes or a misrouted operation. Test bearer-authenticated writes and no-auth writes, plus revoked-token refusal before changes.

Document all eleven tools and the authoring workflow in `docs/MCP.md`, with the template authoring guide linked. Human PR review can inspect the generated image through an MCP client; this visual check is not an autonomous task checkbox, and this change requires no additional physical print demonstration.
