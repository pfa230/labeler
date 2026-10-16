## 1. Remove the inventory backend

- [ ] 1.1 Delete the connector/Homebox modules, connector-only egress client, application state, connection handlers/routes and OpenAPI registrations/models, removing dependency features made unused by this deletion (`connections` removal; issue #452); verify authenticated calls to the deleted CRUD/schema/browse/materialize paths use the existing route-not-found envelope, their operations/models are absent from OpenAPI, and retained MCP/printer HTTP consumers keep their dependencies.
- [ ] 1.2 Remove connector-only error conversion, `Upstream`, connection record kind and the twelve retired reason slugs (`errors`: Service error codes, Reason slugs); replace malformed-body tests using deleted routes with a retained typed endpoint and verify the existing JSON rejection assertions still pass.
- [ ] 1.3 Append the database migration dropping inventory connections and their default override, remove connection store types/methods and default-connection setting handling (`settings`: Settings endpoints, Persisted inventory connection cleanup); verify an upgrade test preserves every unrelated populated table and setting, and API assertions show only supported settings and reject the deleted key as unknown.

## 2. Remove inventory UI and preserve Import

- [ ] 2.1 Delete Connect/Connections pages, routes, navigation entries, API hooks, browser/mapping/sort/filter/column helpers and their dedicated tests (`ui`: Primary navigation; removed Connect requirements); verify the shell navigation test expects exactly Labels, Import and Settings and the router no longer registers the inventory screens.
- [ ] 2.2 Move the existing display helper into `labelGrid.ts`, remove unused row provenance/discriminator fields, and remove connector-only CSS/test setup while retaining shared grid assets (`ui`: Grid columns, Batch grid preview follows the template format, Parameter controls, What a screen submits, CSV Import screen); verify the retained Import and LabelGrid tests cover cell edits, single/sheet previews, copies, row failures, download and print without importing connector code.

## 3. Retire the contract and update current documentation

- [ ] 3.1 Update the UI spec Purpose directly to describe the remaining screens, remove the retired domain from active lists in `AGENTS.md` and `openspec/config.yaml`, and update `README.md`/`docs/VISION.md` (issue #452); verify current documentation no longer advertises built-in inventory connectors and generic Homebox QR fixtures/examples and historical archives remain intact; retain `retire_capabilities: true` so normal spec synchronization/archive deletes the connections capability.

## 4. Verify the complete removal

- [ ] 4.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `npm --prefix ui run lint`, `npm --prefix ui run test`, `npm --prefix ui run build`, and `npx --no-install openspec validate --all --strict --no-interactive` (`AGENTS.md` gates); record the results under `.agent-runs/` and verify retained MCP, rendering, printing and CSV tests pass alongside the focused removal assertions.
