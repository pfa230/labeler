## Context

See `proposal.md` for motivation and scope. Homebox is the sole member of `ConnectorRegistry`. Inventory connections are persisted separately from printers, but application state, settings validation, OpenAPI and the UI import their types. `LabelGrid` imports `displayCellText` from `connectorRows.ts` only for its `list` cell branch. Import filters every `list` parameter out of its grid fields, so that branch is unreachable once Connect is removed. The shared row model also carries connector provenance that no remaining production consumer reads.

This is a cross-cutting removal with a stored-data change, so a design is needed.

## Goals / Non-Goals

**Goals:** Delete the inventory subsystem completely, preserve the existing supplied-data paths, and remove its stored state using the existing migration mechanism.

**Non-Goals:** Changes to Homebox tooling, new CSV features, replacement integration architecture, printer transport changes or broad grid refactoring.

## Decisions

### 1. Delete the connector subsystem and use existing route fallbacks

Delete `src/connector/`, `src/egress.rs`, their application state, connection handlers/routes, OpenAPI registrations, store models/methods, error conversion and unused reason variants. Remove the connector-only `Upstream` error code and `connection` not-found kind. Keep HTTP libraries and URL helpers still used by the MCP adapter, printers or rendering; remove only dependencies/features made unused by this deletion.

The existing unknown-API-route handler supplies the error contract for deleted routes. Removing only the Homebox implementation would leave connection persistence, endpoints and schema machinery with no consumer. Dedicated compatibility handlers would add behavior the issue does not request.

### 2. Use one appended database migration

Append a migration in `store.rs` that drops `connections` and deletes only the `default_connection_id` row from `app_settings`. Retain historical migrations so databases at supported existing versions can still reach the latest version. Remove that key from known settings, default-id handling and connection-existence validation, preserving `default_printer_id` handling.

Leaving the table behind would retain obsolete credentials and fail the persisted-state cleanup requirement. Rewriting historical migrations would invalidate the upgrade path for existing databases. A new export, backup subsystem or reversible connector migration is unnecessary.

### 3. Remove the UI pages and detach the remaining CSV grid

Delete Connect, Connections forms/list, connector browsing, mapping, sorting/filtering/column helpers, query hooks and their dedicated tests. Remove their routes and navigation entries; existing wildcard routing handles unrecognized UI paths. Labels, Import and Settings remain the primary navigation.

Delete `displayCellText` with `connectorRows.ts`, its import in `LabelGrid`, the unreachable `list` cell branch and the `list` check in `isOperable`. Remove `RowSource`, the unused `origin` discriminator and `source` field from grid rows and their construction sites. Remove connector-only styles and test setup while retaining the SVAR grid dependency, shared grid styling and setup used by `LabelGrid`.

Import already omits list columns, as required by `ui`: Grid columns and Parameter controls. Keep its existing "skips list parameters when building grid columns and does not break import" test as the guard: it fails if a CSV list column is shown or submitted. Moving the helper would preserve dead code without serving any remaining Import behavior. Another shared module or grid abstraction is unnecessary.

### 4. Retire the capability and update the remaining contract

The user approved this retirement method on 2026-10-16: remove the connections delta from this change, then delete `openspec/specs/connections/` during implementation before synchronization/archive. All 23 requirements are retired. Keep the retirement record in this design and `proposal.md`, which remain in the archived change folder and Git history. Archive refuses automatic retirement because the main spec's requirement-body tables are treated as unaccounted content; retaining a removal-only delta after deleting the main spec makes archive recreate an empty, invalid capability. Direct deletion with no connections delta avoids both failures.

Update settings, UI and errors through their deltas. Explicitly remove the combined service/upstream Error codes requirement and add Service error codes with the retained vocabulary and route-not-found scenario; the upstream-rate-limit scenario ends with its capability. A plain MODIFIED Error codes block was checked during this revision: the pinned validator rejects it with `MODIFIED "Error codes" omits scenario(s) the current spec still has: "Upstream rate limit"`. Explicit removal/addition records that intentional scenario retirement and passes validation; this is why the simpler modification is not used. During implementation, update the UI Purpose directly to describe the remaining screens; delta Purpose sections cannot update an existing capability. Remove `connections` from the active domain lists in `AGENTS.md` and `openspec/config.yaml` as part of retiring that domain.

Update `README.md` and `docs/VISION.md` to describe data supplied through REST/MCP and CSV. Keep historical archived changes and Homebox-named template fixtures that test ordinary interpolation/QR behavior. A blanket deletion of the word Homebox would destroy useful generic examples and test coverage; an empty capability or placeholder requirement would retain a contract for a removed feature.

## Risks / Trade-offs

- Browser-only live inventory selection goes away. This is the accepted product trade-off; use the existing agent workflow or CSV import.
- The migration discards connection configuration and credentials. Restoring an older release with those records requires the normal pre-upgrade database backup; no downgrade migration is added.
- Shared helpers and CSS can hide dependencies on Connect. Preserve the CSV import tests and full UI gates, rather than deleting shared grid code by filename association.
- CSV does not import list parameters. This removal does not resolve that separately tracked limitation; supplied JSON/MCP data and the print form retain their current list behavior.

## Validation

Use the existing suites for generic rendering, template authoring, MCP, CSV preview/download/print and printer settings. Adapt malformed-body tests that currently use connection endpoints to a retained typed endpoint.

Add focused assertions only for changed behavior: authenticated requests to the deleted API paths use the existing route-not-found envelope; OpenAPI omits their operations and schemas; settings omit the removed key and reject writes/deletes as unknown; navigation contains only Labels, Import and Settings; upgrading a populated pre-change database removes connector state while preserving all other stored values. These fail if a route or setting remains, documentation still publishes its schema, navigation still exposes a page, or the migration leaves connector state or damages unrelated records.

Run the Rust, UI and strict OpenSpec gates required by `AGENTS.md`. No physical print or new visual template check is needed because this change edits neither templates nor printer behavior.

## Cut pass

| Kept work | Requirement or explicit request | Simpler version and concrete failure |
|---|---|---|
| Backend deletion and OpenAPI cleanup | Removed `connections` requirements; issue #452 | Hiding the UI leaves callable connector APIs and stored credentials. |
| Error/reason cleanup and retained malformed-body example | `errors`: Service error codes, Reason slugs, Request body rejection | Leaving the vocabulary publishes errors for a subsystem with no producer; removing the malformed-body example loses a still-supported contract example. |
| One appended migration and setting removal | `settings`: Settings endpoints, Persisted inventory connection cleanup | Deleting runtime code alone leaves obsolete credentials and the stored setting. |
| UI deletion and navigation | `ui`: Primary navigation; removed Connect requirements | Removing only links leaves the pages reachable by URL. |
| Deleting unreachable list-cell code and removing provenance | `ui`: Grid columns, Parameter controls and CSV Import screen; issue #452 | Moving `displayCellText` preserves an unreachable branch: Import omits list columns. Delete the helper, its import, the branch and the `isOperable` list check; retain the existing Import list-column test, which fails if a list column is shown or submitted. Leaving provenance retains connector metadata with no consumer. |
| Explicit main-spec deletion, retirement record and current-doc updates | Issue #452; user-approved retirement decision | Automatic retirement fails on the requirement-body tables; retaining a removal-only delta recreates an empty capability. Proposal/design preserve the retirement record, and current-doc edits stop advertising the removed workflow. |
| Focused changed-behavior assertions and required gates | The named requirements above; `AGENTS.md` gates | Compilation alone cannot establish routing, migration preservation or retained Import behavior. |

Cut: moving `displayCellText`, its unreachable `LabelGrid` list branch and the `isOperable` list check, because Import omits list columns; the connections removal delta, which prevents archival after direct capability deletion; replacement connectors, feature flags, compatibility routes, a generic shared grid layer, CSV list syntax, database export/backup tooling, and fixture renames. Folded a separate dependency-cleanup task into backend/UI deletion to avoid a duplicate pass. Existing REST/MCP, printing and rendering checks are reused rather than duplicated.
