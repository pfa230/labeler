# 31. Inbound print webhook (POST /print)

Date: 2026-06-26

## Status

Accepted. Related to issue [#22](https://github.com/pfa230/labeler/issues/22).

## Context

Integrations such as Grocy need to trigger a label print with a single HTTP call: supply a template,
a printer, and a set of field values and get a label dispatched. The existing `POST /batch` endpoint
covers this use case but requires the caller to understand the full batch envelope (a `labels` array,
a `mode` field, format options) and to handle both download and print responses. That surface is
richer than automation needs for a one-shot webhook scenario.

A thin dedicated endpoint that accepts a flat "print this label N times" payload and always returns a
`BatchSummary` gives integrations a simpler, stable target without duplicating any print logic.

The endpoint is intended for trusted-LAN use (a homelab or a local automation bus). It must not be
exposed to the internet: the API token is the access gate, but there is no rate limiting, IP
allowlisting, or other hardening beyond the existing auth middleware.

## Decision

Add `POST /print` as a thin inbound print webhook. The handler:

1. Deserializes a flat `PrintRequest` payload (`template`, `printer`, `fields`, `option?`, `copies?`).
2. Validates `copies` is in `[1, 100]`; rejects out-of-range values with `400`.
3. Constructs a `LabelInput` from `fields` (mapped to `data`) and `option`.
4. Expands into `vec![label; copies]` and delegates entirely to the shared `run_batch` helper used by
   `POST /batch` and `POST /import/csv`.
5. Returns the `BatchSummary` from `run_batch` directly.

The endpoint is **not auth-exempt**: it follows the same authentication middleware as every other
`/api` data route (session cookie or `Authorization: Bearer` token). `LABELER_NO_AUTH=true` disables
auth uniformly, including this endpoint, for local use.

The request body is capped at **64 KiB** via axum's `DefaultBodyLimit::max(64 * 1024)` layer applied
only to this route. Bodies exceeding the cap are rejected with `413 PayloadTooLarge` before
deserialization, matching the stable error contract.

`copies` is defined as **label instances** (not printer copies). A value of 2 sends two identical
labels: two separate print jobs for single/tape templates, or two slots on a sheet. The cap of 100 is
an explicit sanity bound; the rationale is that a webhook caller intending hundreds of copies should
use `/batch` directly.

The handler adds no new dispatch or rendering logic: all format-dispatch, validate-then-execute,
`BatchSummary` assembly, and error mapping remain in `run_batch`.

## Consequences

- Integrations (Grocy, scripts, automation) can trigger a label print with a single `curl` call and a
  flat JSON payload, without understanding the batch envelope.
- No new print logic: `/batch` remains the canonical bulk endpoint; `/print` is a pure caller of the
  shared path.
- Send failures (transport errors after a job is accepted) are reported in `BatchSummary.failed[]`
  with a `200`, mirroring `/batch` behavior. Pre-dispatch failures (body too large, unknown printer,
  disabled printer) return the appropriate error status. A failure during the dispatch attempt itself,
  before any job is accepted, is a `502`.
- The 64 KiB body cap prevents runaway memory from unexpectedly large payloads; the cap is tight
  because the payload is a single label's fields, not a multi-label batch.
- The `copies` upper bound (100) is a webhook-appropriate sanity cap; callers needing more should use
  `/batch`.
- Trusted-LAN posture must be documented and enforced operationally: this endpoint should not be
  exposed to the internet.
