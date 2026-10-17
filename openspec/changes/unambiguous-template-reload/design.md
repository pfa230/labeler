## Context

See `proposal.md` for the problem and scope. The same router registers the static `/templates/reload` route and dynamic `/templates/{id}` handlers. Axum selects the static path first; changing registration order cannot make literal template-id requests reach the dynamic route ([Router documentation](https://docs.rs/axum/latest/axum/struct.Router.html#method.route)). The MCP adapter avoids the collision by validating ids and escaping each id's first byte; its real-service lifecycle test already exercises the id `reload`, but does not establish that literal REST paths work.

This design is needed because route registration, published REST paths, the MCP adapter and operator instructions must change together. The current source/spec audit establishes the registrations and workaround; it does not repeat the issue's historical runtime probe.

## Goals / Non-Goals

**Goals:** Separate registry reload from template identity with one route move and make ordinary literal-id requests sufficient for REST and MCP callers.

**Non-Goals:** See the proposal. Retain the existing reload handler, registry publication and error handling; there is no need for a new registry service or module.

## Decisions

### 1. Move the reload operation to the approved path

User decision, 2026-10-16: use `POST /api/template-registry/reload`. Move the route and its OpenAPI path annotation together. The existing handler supplies the same success counts, quarantines and failure preservation. The route remains under the existing authenticated API router.

Remove the static template-path registration. An alias at that path would continue shadowing the template id. Reserving `reload` would violate #428's valid-id acceptance. Dispatching on request bodies would leave a single path with two operations and would still need special method handling. Moving template CRUD would change every template client instead of one registry endpoint.

### 2. Update MCP using ordinary validated template ids

Routine default: keep the MCP tool names, arguments and result conversion. Change only `reload_templates`' REST target and remove the first-byte escaping that exists solely for the retired collision. Keep validation in the existing shared template-id helper; every permitted character is already valid in an ordinary URL path segment. Do not add a second encoder or a `reload` special case.

Keeping the escape is a smaller patch but retains an unnecessary mechanism. The simpler validated literal segment addresses every accepted id after the route move. Update affected mock expectations and reuse the existing real-service MCP `reload` lifecycle plus independent reload test.

### 3. Prove both operations through their public paths

Routine default: use the existing HTTP test harness in `src/lib.rs` for a literal-path lifecycle. Assert create status `201`, returned id and exact stored YAML; a repeated create returns `409` without replacing it; GET reads its detail; PUT updates the file and detail; DELETE removes it and later GET is `404`. This fails if the static route remains, a method cannot reach the template handler or a create merely returns reload success without writing the file.

Independently add a valid template file directly to the test directory and call the registry endpoint; the served set must include that file and the response must contain the existing reload counts. Point the existing refused-file and unreadable-directory reload tests at the new path rather than duplicating them. Update the existing OpenAPI assertions to publish the registry operation and the generic template CRUD paths without a static `/templates/reload` operation. Those checks fail if routing and published paths disagree.

Adapt existing MCP wire mocks to require the new reload target and ordinary validated template paths. Reuse their authentication, exact-YAML and mutation-delivery checks and the real-service integration test. A stubbed reload response alone cannot prove the service republishes a registry, which is why the direct HTTP and existing real-service checks remain.

### 4. Update current operator instructions with the endpoint

Routine default: replace the reload call in `docs/AUTHORING.md`, `docs/DEPLOY.md`, `docs/MCP.md`, `AGENTS.md` and the startup quarantine diagnostic in `src/main.rs`; update any other active call site found during implementation. Leave archived change records intact. The browser already makes literal template CRUD requests and has no registry-reload call to change.

## Risks / Trade-offs

- Existing REST reload callers must use the new endpoint when upgrading. Deploy the server and bundled MCP adapter from the same revision and publish the current endpoint in operator instructions; no compatibility route is retained.
- Reverting requires reverting server, adapter and documentation together. There is no stored-data migration.
- A lifecycle test can pass through encoded paths while the public literal path is still broken. The HTTP regression must spell `reload` literally.

## Validation

The scoping gate is strict OpenSpec validation and artifact review. Implementation runs `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` and strict OpenSpec validation. No UI source edit or UI gate is planned. Tests must demonstrate the concrete failure modes above before they count as acceptance evidence.

## Cut pass

| Kept item | Requirement or explicit request | Simpler version and concrete failure |
|---|---|---|
| One moved route and OpenAPI annotation (task 1.1) | `templates`: Reload; #428's matching public API | Leaving the static route shadows a valid template; retaining an alias has the same failure. Reuse the handler rather than introduce a registry layer. |
| Literal-id lifecycle regression (task 1.2) | `templates`: Template id is the filename, Create with POST, replace with PUT, Template detail and source, Delete a template | Reusing only encoded MCP coverage misses the observed literal-path collision. Assert stored bytes as well as status to detect false creation success. |
| Existing reload tests and OpenAPI assertions at the new path (task 1.3) | `templates`: Reload; #428's independent reload and matching public API | A create/read lifecycle alone cannot prove disk reload works; a route-only change can leave the published operation stale. Adapt existing tests instead of duplicating failure suites. |
| MCP target change and simpler validated id paths (task 2.1) | `mcp`: Tool surface; #428's matching client update | Leaving the target attempts template creation instead of registry reload. Remove forced escaping; validation remains required by Tool surface. |
| Existing MCP mocks and live-service coverage (task 2.2) | `mcp`: Tool surface, Registry reload results, Template mutation results | REST tests alone cannot prove the adapter calls the new path. Reuse existing cases rather than build a second harness. |
| Current docs and startup diagnostic (task 3.1) | #428's matching client updates and unambiguous public API | Updating only code leaves operator commands targeting the template `reload`. Change endpoint references without rewriting unrelated guidance. |
| Required Rust gates (task 4.1) | Repository gates for the planned Rust diff | Compilation alone does not check the literal-path and MCP behavior; use the existing suites. |
| Strict spec validation (task 4.2) | Repository gate for `templates` and `mcp` deltas | Unvalidated deltas can fail synchronization or omit existing scenarios. Use the pinned command with no custom content checker. |

Cut: first-byte escaping and its collision-specific commentary; compatibility aliases, redirects, reserved ids, body-based dispatch, API versioning, a new registry abstraction, route/OpenAPI coupling refactors, duplicate MCP/reload failure suites and UI changes. No new response field, setting or dependency is needed. The existing twelve-tool surface and reload JSON fields remain the smallest contract that meets the request.
