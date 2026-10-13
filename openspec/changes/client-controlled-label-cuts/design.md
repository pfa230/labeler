## Context

See proposal.md for motivation. The accepted direction is one capability-gated checkbox, replacing the earlier cut-mode selector. The current contract is [ptouch-print-server PR #47](https://github.com/pfa230/ptouch-print-server/pull/47), revision `0603578c02a577e61b6a50abc08a5fef3975a320`: one IPP job is one batch, precut is mandatory once per batch, `ptouch-chain` separates labels while leaving the last end held, and trim finishes the run. A following healthy batch releases the previous held end without another discarded leader.

The existing `PrintRequest` is strict JSON. `run_batch` renders all units before dispatching them in order; single templates produce one job per entry carrying its copy count, and sheets produce one PDF job with printer copies 1. The merged copies change (#433, PR #435) renders a single entry once, counts its physical copies in summaries and failure indices, and sends `ipp-attribute-fidelity: true`. Driver probing already handles stored connection credentials and TLS settings. The existing browser-facing probe takes unsaved connection fields, while stored printer responses omit the password.

The server's chain-only preset has no registered enum. It uses the standard collection mechanism with an advertised printer-defined template, as documented by the [PWG job attributes](https://www.pwg.org/ipp/print.html) and [IPP Finishings 3.0](https://ftp.pwg.org/pub/pwg/candidates/cs-ippfinishings30-20220527-5100.1.pdf). A numeric finishing choice cannot stand in for `ptouch-chain`.

## Goals / Non-Goals

**Goals:** Carry one continuation choice from the print form to every job, expose only genuine printer support, and retain ordinary printing and existing render/dispatch behavior.

**Non-Goals:** No-label-cuts or final-cut-only choices, a precut toggle, job coalescing or raster conversion, persistent batch state, an extra finalization button/job, queue inference, retries, or new controls on Connect, Import or MCP. The existing job structure is sufficient for healthy cross-job continuation; each screen's ordinary requests use the API's ordinary behavior.

## Decisions

### 1. A strict boolean matches the checkbox

Use a `chain_and_continue` boolean in `PrintRequest`, with serde's existing false default convention. Omission and false both mean ordinary printing; non-boolean and null values use existing JSON rejection. Extend `PrintBody` without introducing an API enum or a free-form finishing template.

When checked, every job carries chain and continue, including the last job and a one-job request. In the print form, Copies 3 is now one entry and one IPP job with `copies: 3`; inside that job the printer separates the copies and holds the last end. Multiple distinct entries still produce multiple ordered jobs. The user's intent is to leave the run available for a later print, so no final-job index changes the choice. When unchecked, request trim enum 11 if advertised; otherwise keep the existing omission behavior for other printers.

The user's latest direction supersedes issue #430's earlier none/batch modes. The change remains within that issue's API/print-form purpose and reuses the same planning artifacts.

### 2. Discover only the two supported choices

Extend `PrinterCapabilities` with `supports_trim` and `supports_chain_and_continue` booleans. Trim requires enum 11 in `finishings-supported`. Continuation requires both the supported `finishing-template` collection member and the supported `ptouch-chain` template, accepting the standard scalar/set keyword/name forms. Neither a model name nor numeric finishings alone establishes chain support.

Expose only the continuation flag as `ProbeCapabilities.chain_and_continue`. Query once per print request and reuse the capabilities for render negotiation even with both render fields overridden. If explicit continuation is no longer supported, reject it before label rendering through an existing-style `AppError` constructor and `Reason::ChainAndContinueUnsupported`; the slug's canonical definition stays in the errors delta. Never replace a checked request with trim after a probe failure.

Two flags avoid storing unrelated policies, and server-side revalidation covers capability changes since the form's probe without trusting the browser.

### 3. A stored-printer probe supplies the form's capability

Add `POST /api/printers/{id}/probe` with no body. Resolve `get_printer_connection`, reuse `driver_for` and the existing probe/result shaping, and register the route in OpenAPI. The connection-fields probe gains the same continuation flag, so both routes have one response shape.

The browser queries this route for the selected printer through the existing query library, with key `["printers", id, "probe"]`. The existing printer-save/delete invalidation of `["printers"]` then also refreshes this capability result. Hide the checkbox until that printer's successful result confirms support, including on an unreachable result, while keeping normal Print usable. Reuse `sendJson` with an undefined body for this bodyless POST instead of adding another transport helper.

The existing unsaved-connection probe cannot authenticate using a stored password it never receives; returning that password to the form would violate the printer-fields contract. Probing on every printer list/detail read would add network latency to unrelated reads. One dedicated stored-id probe is the smallest read needed for the requested checkbox.

### 4. One driver option selects one finishing representation

Carry the request boolean through the existing printer `BatchTarget`. Resolve it once to an optional driver finishing choice: `Trim`, `ChainAndContinue`, or absent for unsupported ordinary printing. This small internal enum prevents the two IPP representations from being emitted together without exposing general finishing configuration.

Extend `PrintOptions` with that choice. In `print_job_request`, Trim attaches enum 11; ChainAndContinue attaches one `finishings-col` collection with keyword member `finishing-template: ptouch-chain`; absent attaches neither. Keep integer `copies`, operation `ipp-attribute-fidelity: true`, the media collection, document format, title and payload code intact. The current dispatch loop uses the same choice for each unit and retains send-error accounting and job logging. Add no cleanup request, replay or special terminal handling.

### 5. One checkbox belongs beside Print

Add an unchecked Chain and continue checkbox beside Print in `PrintForm`, using existing checkbox/form conventions. Its state belongs to the selected printer, resets on a printer switch or loss of confirmed support, and otherwise remains available across successive prints until the operator unchecks it. Key the probe by printer id so a late response for the previous printer cannot enable or select continuation for the new printer.

Submit `chain_and_continue: true` only when the supported checkbox is checked. Unchecked/absent omits the field. Keep preview and download bodies independent, and reuse print error presentation. Concise help explains that labels are separated and the final end stays attached for the next print; printing normally next finishes the healthy run. The form exposes no server policy knobs or raw IPP values.

## Risks / Trade-offs

- Discovering support adds a capability request for the selected printer; the existing three-second timeout and independent normal Print availability keep that read bounded.
- A cached form result can become stale. The print request probes again and refuses unsupported explicit continuation before any send rather than silently cutting the run.
- A send failure or canceled job can leave the last end held or output uncertain. Preserve the printer's recovery behavior and existing accepted-job summary; do not add client recovery or retries.
- Precut is mandatory at each job boundary on the refactored server. Keep one job per entry with its copies and use its healthy continuation semantics rather than promising a joined strip or adding a multi-label output format.
- Generic printers need the advertised `ptouch-chain` template to expose this specific checkbox. Other finishing policies remain unused.

## Migration Plan

No stored data, template, printer configuration or print-server deployment change is needed. Review and commit the revised plan locally, then implement through the repository's supervised loop. Tests must capture the collection at the IPP boundary and prove checkbox gating and state isolation. Record PT-2730 observations from the changed labeler path for a checked three-copy job, checked multi-entry continuation across an idle gap and an unchecked subsequent print that releases/finishes the run; upstream observations do not establish labeler acceptance. Keep run artifacts under `.agent-runs/`. Complete the touched-diff gates and keep changes local until spec sync and archival; integration requires the repository's human review.
