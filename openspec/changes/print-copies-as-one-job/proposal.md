## Why

A request for three copies of one label reaches the printer as three separate IPP jobs, because the UI sends the label three times and `/print` sends one job per label. On the PT-2730 print server one job is one physical batch with its own precut leader, so three copies waste two leaders and lose the server's copy and cut boundaries. The browser knows the copy count and discards it. The request should carry the count so the server can submit one job asking the printer for three copies.

Fixes #433

## What Changes

- A label entry in `/render` and `/print` gains an optional `copies` integer (at least 1, default 1). An entry with `copies: N` stands for N labels with its data. Label indices, the 500-label cap, ZIP entries and sheet slots count those labels, so downloads and sheet pagination produce what they produce today.
- `/print` of a `single` template sends each entry as exactly one job carrying the IPP `copies` attribute, rendered once. A sheet job carries `copies` 1.
- **Breaking:** every job carries `ipp-attribute-fidelity: true`, so a printer refuses a job it cannot print as asked, for example a copy count it does not support, instead of quietly changing it. Those labels are reported failed. A print that a printer changes and accepts today now fails, and a printer without copies support no longer prints repeated labels as separate jobs.
- The print summary keeps its shape. `total` and `sent` count labels and `jobs` counts jobs, so three accepted copies report `total=3, sent=3, jobs=1`. A failed job lists every label index it carried, and no job is sent twice.
- The print form sends its label once with `copies`. Connect and Import send each row once with `copies`, for print, download and the sheet preview alike.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `rendering`: Label entries carry `copies`; `/print` output and the summary examples change to one job per entry.
- `printing`: Every job carries `ipp-attribute-fidelity` and `copies`, a `single` entry is one job, and a job is never resent.
- `ui`: Screens send each label or row once with its copy count instead of repeating it.

## Impact

Touches `LabelInput` and its OpenAPI schema, `batch.rs` (count and expand entries, render each entry once, one print unit per entry), `driver.rs` (`copies` in `PrintOptions` and both attributes in `print_job_request`), the dispatch loop in `api.rs`, and `ui/src/lib/labelGrid.ts`, `ui/src/lib/sheetPreview.ts`, `PrintForm.tsx`, `Connect.tsx` and `Import.tsx` with their tests. No new error reason, dependency, stored data or print-server change. Capability parsing and negotiation are unchanged. The MCP `print_labels` argument schema stays `{data}` per entry.
