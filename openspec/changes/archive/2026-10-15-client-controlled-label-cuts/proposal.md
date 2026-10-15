## Why

The refactored PT-2730 server advertises a chain-and-continue finishing template, allowing successive prints to share a healthy tape run without discarded leaders between jobs. Labeler needs a simple, printer-supported way to select that behavior beside Print.

Fixes #430

## What Changes

- Add optional boolean `chain_and_continue` to the print request, defaulting to false.
- Show an initially unchecked Chain and continue checkbox beside the print form's Print button only when the selected printer advertises the required collection/template support.
- When checked, send `finishings-col` with `finishing-template: ptouch-chain` on every dispatched job, including the last job of the request.
- Ordinary printing explicitly requests trim when supported; other printers retain their existing finishing behavior.
- Extend existing capability handling and expose a stored-printer probe so the form can discover support without receiving the stored password. Reject unsupported explicit continuation before rendering or dispatch.
- Offer no no-cuts or final-cut-only choice. Preserve the merged one-job-per-entry copies behavior, attribute fidelity, rendering, media geometry, job order, summaries and job logging, without an invented finalization job or replay.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `rendering`: Add the print-only boolean and request-level continuation validation.
- `printing`: Discover chain-template support, probe stored printers, and submit either chain-and-continue or ordinary trim through the existing IPP builder.
- `errors`: Register the unsupported continuation reason in the existing vocabulary.
- `ui`: Gate the checkbox on the selected printer's advertised support and carry its value only on print requests.

## Impact

The change touches request models, printer capability and request-building code, the existing print API, a stored-printer probe route, existing error constructors and reason enum, OpenAPI, UI API types/query hooks, and the existing print form and tests. It needs no dependency, stored-printer schema, database, renderer, new output format or print-server configuration change. The accepted user direction supersedes the issue's earlier no-cuts/batch-mode acceptance; physical continuation must be checked through the changed labeler path before claiming hardware success.
