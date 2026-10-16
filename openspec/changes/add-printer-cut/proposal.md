## Why

Chain and continue leaves the last printed label attached to the tape, and the only way to release it today is to print another label. ptouch-print-server is adding a dedicated cut action, the project-local IPP operation `Ptouch-Cut` (`0x5f01`), which releases the held label without a document or job. Labeler should let operators and agents send it.

Fixes #460

## What Changes

- The printer probe reports `cut: true` when the printer lists `0x5f01` in `operations-supported`.
- New `POST /api/printers/{id}/cut` sends one `Ptouch-Cut` request to a stored printer and answers `200` with `{status: "ok"}` or `{status: "failed", detail}`.
- The print form shows a Cut button beside Print when the selected printer's probe reports cut support.
- New MCP tool `cut_printer` forwards to the cut endpoint.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `printing`: cut capability in the probe and the cut endpoint.
- `ui`: Cut button on the print form.
- `mcp`: `cut_printer` tool.

## Impact

`src/driver.rs` (capability parsing, a cut request on the driver trait, the fake driver), `src/api.rs` and `src/openapi.rs` (route and response model), `src/models.rs`, `src/bin/labeler-mcp.rs`, and `ui/src/` (API client, types, print form). No dependency changes: the `ipp` crate's `IppRequestResponse::header_mut` sets the vendor operation code.
