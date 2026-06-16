# 11. Unified batch render/print endpoint

**Status:** Accepted

## Context

The service grew three overlapping ways to turn data into output: `POST /render/label` (one single
label → PNG/PDF), `POST /render/batch` (sheet only → one PDF), and `POST /print` (one single label →
print or download). The M5 web UI needs to render and print *batches* of labels (a CSV of N rows, a
form with copies), across both template formats. Driving that from the existing endpoints forces the
frontend to branch on template format: loop per-label for single (tape) templates, but compose into
slots via `/render/batch` for sheet templates. That couples the UI to backend rendering concerns,
duplicates batch logic between the UI and `/import/csv`, and leaves sheet-to-printer and multi-page
sheets unbuilt (deferred in #28).

`/import/csv` already renders/zips/prints a batch server-side, but only for single-format templates and
from a raw CSV body, so it cannot serve the UI's customized, edited batches.

## Decision

Introduce one endpoint that owns the format decision:

- **`POST /batch { template, labels: [{ data, option }], mode, printer?, format?, start_slot? }`**
  (moves to `/api/batch` with the #15 `/api` sweep). Clients post a list of resolved labels and a mode;
  the backend dispatches on the template's format and returns the format-appropriate result. The client
  branches only on `mode`: **download** yields a binary, **print** yields a JSON summary. It never
  branches on single vs sheet.

- **Dispatch matrix:** single+download → ZIP of per-label files; single+print → one job per label;
  sheet+download → labels laid into slots, paginated across pages, one PDF; sheet+print → that PDF sent
  to the printer. `start_slot` skips a leading N slots on a sheet's first page; per-label `copies` is
  expanded client-side (no server parameter).

- **Synchronous execution** with a configured cap on `labels` (default 500 → `413 BatchTooLarge`),
  matching the in-process Typst pipeline and home/SMB scale. The handler is structured so an async job
  runner can replace it later without a contract change.

- **Validate-then-execute error model.** Render-validate every label first; if any has bad data, return
  `422 BatchInvalid` listing all failing indices and produce/print nothing (atomic, both formats). Then
  execute: downloads return the blob; print transport failures are best-effort and reported per-label in
  a `200` summary `{ total, succeeded, failed: [{ index, error }], jobs }`.

- **Consolidation.** `/render/batch` and `/print` are removed and absorbed. A shared `batch` module
  (`render_batch`) backs the endpoint, and `/import/csv` is refactored to parse the CSV and call it,
  remaining the self-contained CSV path. `render_sheet_labels` generalizes to multi-page pagination.

- **`/render/label` is retained** as the single-label preview / one-off download path (raw image/PDF,
  no zip), which the UI uses for live previews.

## Consequences

- The frontend posts a uniform batch and renders the response; format complexity lives in one backend
  place. The same `render_batch` serves the UI and the CSV API path.
- Delivers sheet-to-printer and multi-page sheets, previously deferred in #28; server-side `copies`
  from #28 is moot (client-side expansion), so #28 is rescoped/closeable.
- Breaking API change: `/print` and `/render/batch` are removed. Their tests migrate to `/batch`, and
  SPEC endpoints, `scripts/*.sh`, the sample webhook path (#22), and `openapi.rs` update. Done while
  pre-release with no external consumers.
- New stable error codes `BatchInvalid` (422) and `BatchTooLarge` (413).
- Job/template-level options (#31, e.g. richer slot skipping) extend this contract additively.

## Alternatives considered

- **Frontend loops the existing per-label/sheet endpoints.** Rejected: it forces the UI to branch on
  template format and re-implement batch/pagination/summary logic that belongs in the backend.
- **Keep all endpoints and add `/batch`.** Rejected: redundant, two ways to do the same thing.
- **`/batch` subsumes preview too (batch-of-one).** Rejected: a live preview should not pay ZIP/compose
  overhead or unwrap a single-file archive; `/render/label` stays for that.
