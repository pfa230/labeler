## 1. Resource-local API modules

- [ ] 1.1 Move the five groups in design.md into child modules with their existing route declarations, handlers, API-local models and private helpers. Keep AppState and application composition in `src/api.rs`, merging the resource routers before the existing auth layer. Review the diff to verify paths/methods, handler bodies, custom extractors, locks, error construction, fallback and middleware placement are unchanged; use the current #229/#407 state rather than reverting concurrent work.
- [ ] 1.2 Update qualified OpenAPI references and retain the MCP binary's public `api::valid_printer_id` path. Review each visibility change against its actual consumer, leaving state fields and leaf-only helpers private; verify the existing OpenAPI and MCP tests compile and pass, so an inaccessible handler/schema/helper or lost operation fails.

## 2. Verification

- [ ] 2.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test`, including the existing HTTP/auth/extractor/panic suites; retain results in `.agent-runs/`. A moved/missing route, changed rejection/auth behavior or moved panic layer must still fail the relevant existing assertions.
- [ ] 2.2 Run `npx --no-install openspec validate --all --strict --no-interactive` and review the final diff for the five selected boundaries, one API composition point and only necessary visibility changes; verify no behavior delta, new dependency or additional abstraction was introduced.
