## 1. Separate registry reload from template identity

- [ ] 1.1 Move the existing reload route and its OpenAPI path annotation to `/api/template-registry/reload`, removing the static template-id route; verify the router exposes the new operation and generic CRUD remains registered.
- [ ] 1.2 Add a regression in the existing HTTP harness using the literal id `reload`: create with `201` and matching file bytes/detail, refuse duplicate create with `409` and unchanged bytes, read, replace, delete and observe `404`; demonstrate that the original static route makes the regression fail.
- [ ] 1.3 Point existing reload tests at the registry endpoint, cover an externally added file becoming served independently of template CRUD, and update OpenAPI assertions; verify the reload counts, existing failure-preservation cases and published paths.

## 2. Update the MCP client

- [ ] 2.1 Change `reload_templates` to call the registry endpoint and simplify the shared validated template-id helper to use ordinary literal segments; verify tool names, schemas and result handling remain unchanged and invalid ids are still refused before HTTP.
- [ ] 2.2 Update existing MCP path mocks and reuse the real-service `reload` lifecycle and registry-reload integration coverage; verify exact YAML delivery, the new reload target and successful live template editing/reload through the existing MCP test suites.

## 3. Update operator instructions

- [ ] 3.1 Update active reload examples in `AGENTS.md`, `docs/AUTHORING.md`, `docs/DEPLOY.md`, `docs/MCP.md` and the startup quarantine diagnostic, plus any other active call site found; review the diff and references so every registry-reload instruction uses the new endpoint while historical archives remain intact.

## 4. Run the required gates

- [ ] 4.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test`; all must pass, including the literal REST and MCP regressions above.
- [ ] 4.2 Run `npx --no-install openspec validate --all --strict --no-interactive`; verify all specs and this change pass without changing files during the check.
