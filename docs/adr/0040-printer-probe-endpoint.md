# 40. Printer probe endpoint and shared IPP egress screen

Date: 2026-07-02

## Status

Accepted

Part of the printer-setup redesign (#117). Uses the capability negotiation from
[ADR-0033](0033-capability-aware-rendering.md) and the driver abstraction from
[ADR-0007](0007-printer-architecture-and-transport-model.md). Auth handling is deferred to #118.

## Context

The printer setup UI hand-typed an IPP URI and a pile of driver config with no way to confirm the
printer was reachable or discover what it could do. Meanwhile the backend already performed IPP
`Get-Printer-Attributes` internally at print time (ADR-0033 slice 3), but that capability was invisible
and unreachable from the UI, and `capabilities()` collapsed every failure mode into `None`.

The setup redesign reframes the screen around confirming a printer: both discovery and manual entry
produce a URI, and that URI feeds one operation, "ask the printer what it is." That operation needs to
be an HTTP endpoint the UI can call before saving, and it needs to distinguish a reachable printer that
answered from one it could not reach.

Exposing an endpoint that dials a user-supplied URI is SSRF-adjacent: without screening, an operator
(or, under `LABELER_NO_AUTH`, anyone) could probe internal services by IP. The connector already
screens its outbound requests through `src/egress.rs`; the print path did not.

## Decision

1. **`ProbeOutcome` replaces the lossy `Option`.** The driver trait gains
   `async fn probe(&self) -> ProbeOutcome` where `ProbeOutcome` is `Ok(PrinterCapabilities)` or
   `Unreachable(String)` (the detail carries the IPP status or transport error text). `capabilities()`
   becomes a provided default (`probe().ok()`), so the print path is unchanged.

2. **`POST /printers/probe`** takes `{ kind?, config }`, validates and builds the driver, calls
   `probe()`, and **always returns `200`** with the result as body data
   (`{ status: "ok", capabilities }` or `{ status: "unreachable", detail }`). Reachability is data, not
   the labeler call's HTTP status; a `401` there would collide with labeler's own auth. A malformed
   config or URI (rejected before dialing) is `422`. Capabilities are shaped for UI feedback, including
   a three-way `color: "color" | "bilevel" | "unknown"` so an unknown printer is not shown as color.

3. **All IPP dialing is screened.** `screen_ipp_uri` resolves the target host and rejects any address
   the `egress::ip_allowed` policy blocks (loopback, link-local/metadata `169.254/16`, unspecified,
   multicast). It runs at the top of both `CupsDriver::probe` and `CupsDriver::send`, so the new probe
   endpoint and the existing print path share one guard. Auth-gating is explicitly not the mitigation
   (`LABELER_NO_AUTH` opens the routes); the egress screen is.

4. **Auth is out of scope (#118).** The probe sends no credentials, takes no printer `id`, and merges
   no stored secret, which removes the secret-reuse vector entirely. Existing per-printer stored
   credentials (ADR-0032) are untouched and still used by `/print`.

## Consequences

- The UI can test-connect a printer before saving and render its self-reported media/resolution/color
  as feedback instead of asking the user to type them.
- The print path gains an egress screen it lacked; a printer configured at a loopback/link-local/
  metadata address is now refused (`PrintError::Transport`). This is a deliberate hardening; such a
  target was never a valid printer.
- `screen_ipp_uri` resolves DNS at screen time and the client resolves again at dial time (TOCTOU);
  accepted for a self-hosted tool, matching the `egress.rs` posture.
- Discovery (`GET /printers/discover`, DNS-SD) builds on this endpoint as a URI supplier and is a
  separate slice.
