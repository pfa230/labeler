## Context

ptouch-print-server's change `add-ipp-cut-operation` defines `Ptouch-Cut` (`0x5f01`): a printer operation at the queue URI with the standard operation attributes, no document and no job. PT-P710BT queues list it in `operations-supported`; other models omit it and answer `server-error-operation-not-supported`. A cut with nothing held succeeds as a no-op; `server-error-busy` and `server-error-device-error` report a busy or faulted device. That change is not merged yet, so this one is built and tested against labeler's fake driver and request encoding, not hardware.

Labeler already probes each stored printer (`POST /api/printers/{id}/probe`) and the print form reads that probe to show the Chain and continue checkbox. `CupsDriver::send` posts one IPP request with the `ipp` crate and reports a non-success status as a transport error.

## Goals / Non-Goals

**Goals:** send `Ptouch-Cut` to a stored printer from REST, the print form and MCP; report support in the probe.

**Non-Goals:** a job log entry for cuts (the job log records print requests and their templates; a cut has neither); a cut capability in Test connection's summary on the Printers page; a separate MCP probe tool, since a cut on an unsupported printer reports the printer's own refusal.

## Decisions

**Driver.** Add `cut()` to `PrinterDriver`. `CupsDriver` builds the request with `IppRequestResponse::new` for a standard operation and then sets `header_mut().operation_or_status = 0x5f01`; the `ipp` 6 crate has no vendor `Operation` variant, and the header write keeps its attribute encoding instead of hand-building the IPP bytes. A pure `cut_request(uri)` function returns that request so a unit test can check the operation code, the `printer-uri` attribute and the empty payload, as `print_job_request` is tested today. The client uses the same timeout as `send` (none): the server cuts synchronously and may wait for the device. `FakeDriver` gains a `cut` knob (`ok`, or an IPP status name to fail with) so API tests exercise both outcomes.

**Capability.** `PrinterCapabilities::from_attributes` sets `cut` when `operations-supported` contains enum `0x5f01`, the same tolerant set-or-scalar reading the chain check uses.

**No capability pre-check on cut.** Chain and continue checks capabilities because a printer that ignores an unknown finishing would print with the wrong cuts and nobody would learn of it. A printer without `Ptouch-Cut` refuses the operation itself (`server-error-operation-not-supported`), so a pre-check would add a second IPP round trip and a new reason slug to report what the printer already reports.

**200 with a status body, not an HTTP error.** The probe answers an unreachable printer with `200 {status: "unreachable", detail}`, and printing reports printer-side failures in a `200` summary; HTTP errors are labeler's own refusals. A cut follows that split: `{status: "ok"}` or `{status: "failed", detail}`. The alternative, a new `502` error kind, would add an `AppError` constructor and a code used by nothing else.

**MCP annotations.** `cut_printer` is destructive and non-idempotent like `print_labels`, and never retried: it moves tape, and whether a repeat is a no-op is the printer's guarantee, not labeler's. Its printer id is a URL path segment, so it is checked against the printer id rule before any request, as template ids are.

**UI.** The Cut button reuses the selected printer's existing probe query and the print form's notice for the outcome. It is a plain button beside Print with no confirmation, matching Print.

## Risks / Trade-offs

- [Sibling change `unambiguous-template-reload` also modifies the MCP `Tool surface` requirement] → whichever archives second must carry both edits (the reload path and the `cut_printer` row); rebase this change onto it if it lands first.
- [ptouch-print-server could change the operation code before merging] → the code appears once in the driver and once in the spec; update both.
