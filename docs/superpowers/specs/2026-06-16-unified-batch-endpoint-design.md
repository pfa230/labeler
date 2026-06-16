# Unified batch render/print endpoint (design)

Status: approved (2026-06-16)
Issue: #30. ADR: ADR-0011 (new). Feeds the M5 web UI
([2026-06-15-m5-web-ui-design.md](2026-06-15-m5-web-ui-design.md)).

## Goal

One endpoint the clients post a batch of resolved labels to and get back whatever the template's format
implies. The backend owns the single-vs-sheet decision, so the UI never branches on format: it posts a
batch and renders the response (a binary to download, or a JSON summary for a print run).

## Topology

- **`POST /batch`** — the workhorse for all real render/print jobs (single/sheet × download/print).
  Moves to `/api/batch` with the ADR-0008 `/api` namespacing sweep (#15), like the rest of the API.
- **`POST /render/label`** — kept, for single-label preview and one-off single downloads (returns raw
  `image/png` or `application/pdf`, no zip).
- **`POST /render/batch`** and **`POST /print`** — removed; absorbed by `/batch`.

## Request

```
POST /batch
{
  "template": "asset-tag",
  "labels": [ { "data": { … }, "option": { … } }, … ],
  "mode": "download" | "print",
  "printer": "office",       // required when mode=print, ignored otherwise
  "format": "png" | "pdf",   // download only; sheet output is always pdf
  "start_slot": 0            // sheet only; skip a leading N slots on page 1
}
```

- `labels` is already expanded: per-label **copies** are applied client-side by repeating entries, so
  the endpoint has no `copies` parameter.
- `option` per label is validated against the template's declared `options` (reusing existing
  validation).
- `start_slot` is rejected (`400`) for single templates. `format` is rejected (`400`) with
  `mode=print` (matches `/print` today).
- **Cap:** `labels.len()` over a configured maximum (default 500) → `413` (`code: BatchTooLarge`).

## Dispatch matrix

Dispatch on `template.format`:

| format | download | print |
| --- | --- | --- |
| **single** | render each label, ZIP → `application/zip` | render each, one print job per label |
| **sheet** | place labels in slots, paginate → one `application/pdf` | compose the multi-page PDF, send to the printer |

**Sheet pagination** generalizes today's single-page `render_sheet_labels`: fill `positions` starting
at `start_slot`; when labels exceed the page's slots, begin a new page at slot 0. All pages compose
into one PDF. (single ignores `start_slot`.)

## Execution

Synchronous (inline in the request), matching the current Typst pipeline and home/SMB scale. The cap
bounds worst-case work and the in-memory ZIP/PDF size. A future async job runner can replace the
handler body without changing this contract; not built now.

## Error model — validate-then-execute

1. **Validate by rendering all labels.** Rendering *is* validation. If any label has bad data (missing
   field, invalid option, failed interpolation), return and produce/print nothing:

   ```
   422 { "error": { "code": "BatchInvalid", "message": "…",
                    "details": { "failures": [ { "index": i, "code": "MissingField", "message": "…" } ] } } }
   ```

   All failing indices are listed (not just the first). Atomic for both formats; a sheet cannot compose
   a partial page.
2. **Execute** once validation passes:
   - **download** → `200` binary (`application/zip` or `application/pdf`) with a `Content-Disposition`
     filename (`<template>.zip` / `<template>.pdf`).
   - **print** → dispatch jobs. Printer/transport failures are best-effort (a sent label cannot be
     unprinted), collected into:

     ```
     200 { "total": N, "succeeded": M, "failed": [ { "index": i, "error": "…" } ], "jobs": K }
     ```

     `jobs` = print jobs dispatched (single: one per label; sheet: pages). A failed sheet page records
     each of its labels' indices in `failed[]`, so the summary stays per-label uniform across formats.

## Architecture

- **`batch` module** owns `render_batch(template, labels, mode, printer, format, start_slot, settings)`
  returning either bytes+content-type (download) or a summary (print). The handler is a thin wrapper.
- **`render_sheet_labels` generalizes** to multi-page pagination; the single-page path becomes the
  one-page case.
- **`/import/csv` refactors** to parse the CSV (existing `parse_csv_rows`) → build `labels` → call
  `render_batch`. It removes the duplicated render/zip/print loop and remains the self-contained CSV
  path. Its current single-format-only guard relaxes (sheet CSVs now compose via the same path).
- **Remove `/print` and `/render/batch`:** delete handlers/routes; migrate their tests to `/batch`;
  update SPEC endpoints, `scripts/*.sh`, and `openapi.rs` (new request/response models registered).
- **Settings** thread through as they do in the current render path (for interpolation).

## Relationship to prior decisions

- Delivers the **sheet-to-printer** and **batch** pieces deferred in #28. `copies` is now client-side
  expansion, so #28's server-side copies is moot for the UI (closeable or rescoped).
- **#22 webhook** (deferred) would later target `/batch` as a batch-of-one.
- **Job/template-level options** (#31) extend this contract additively (e.g. richer `start_slot`
  successors); designed separately.

## Testing

- Matrix: each of the four cells produces the right artifact/summary (single zip entry count; sheet PDF
  page count with overflow; single print job count; sheet print summary).
- Validate-then-execute: a batch with one bad label returns `422` with that index and prints/produces
  nothing; an all-valid batch succeeds.
- Print transport: a failing fake-driver yields a `200` summary with the right `failed[]` indices.
- `start_slot`: sheet offset places the first label in the right slot and paginates overflow;
  `start_slot` on a single template → `400`. `format` with print → `400`. Over-cap → `413`.
- `/import/csv` still works end to end through the shared `render_batch`.

## Docs to update in the same change

`docs/SPEC.md` (endpoints, error codes `BatchInvalid`/`BatchTooLarge`, changelog), `docs/adr/0011-*`,
`docs/PLAN-phase-1.md` (rescope #28; note `/print`,`/render/batch` removal), `openapi.rs`, the sample
`scripts/*.sh`.
