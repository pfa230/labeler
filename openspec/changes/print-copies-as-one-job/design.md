## Context

See proposal.md for motivation. Today the UI repeats a label `copies` times (`PrintForm.tsx`, `resolveLabels` in `ui/src/lib/labelGrid.ts`), `render_single_batch` in `batch.rs` renders every entry and makes one `PrintUnit` per label, and the dispatch loop in `api::run_batch` sends one `Print-Job` per unit through `print_job_request`, which carries document format, title and `media-col` only. Connect and Import map a failure index back to its row with `sourceRowForExpandedIndex(index, copies)`, which assumes labels are numbered in expanded order.

The PT-2730 server at `0603578c02a577e61b6a50abc08a5fef3975a320` treats one IPP job as one batch and expands PNG copies inside it. PAPPL 1.4.11 refuses a `copies` value outside 1 to 999 on every job, whatever the fidelity setting (`valid_job_attributes` in `pappl/printer-ipp.c`) [verified]. The 500-label cap keeps every count labeler can send inside that range.

## Goals / Non-Goals

**Goals:** carry the copy count from every screen to the IPP job, render each entry once, and keep label numbering, downloads, sheet pagination, media geometry, validate-before-send and summaries as they are.

**Non-Goals:** reading the printer's copy limit, splitting a count across jobs, a fallback for printers without copy support, inspecting the printer's reply beyond today's success test, combining different labels into one document, retries, a `copies` argument on the MCP `print_labels` tool (its entries stay `{data}`), and finishing selection (#430).

## Decisions

### 1. The count is a field of the label entry

`LabelInput` gains `copies: NonZeroU32` with `#[serde(default = ...)]` naming a function that returns `NonZeroU32::MIN` (the type has no `Default`), so `0`, negatives, fractions, `null` and values past `u32::MAX` fail deserialization as `json_malformed` with no new reason. utoipa does not map `NonZeroU32` to an integer schema by itself, so the field declares its schema as an integer with minimum 1 and default 1, and a test reads that from the generated document. The user chose a per-entry field over two alternatives. Having the server merge adjacent labels with equal `data` would guess a count the client already knew. A request-wide `copies` cannot give rows different counts and would leave downloads expanding in the browser while prints did not.

### 2. Labels are numbered after expansion

An entry with `copies: N` occupies N consecutive label indices. This keeps one index space for the 500 cap, ZIP names, sheet slots, `BatchInvalid` failures and the print summary. Downloads and sheets produce exactly what the repeated entries produce today, and the UI's existing `sourceRowForExpandedIndex` mapping stays correct. Numbering by entry would make `total` count labels while `failed` counted entries.

The label count is a checked sum of the entries' `copies`, taken before anything is expanded or rendered. A sum past 500, or one that overflows, is `413`. `total` in the summary uses that count.

### 3. Each entry renders once

`render_single_batch` validates and renders per entry. A failure is reported once for each label index the entry covers. For a download, the rendered bytes are written once per label into the ZIP. For a print, there is one `PrintUnit` per entry, carrying its bytes, width, the entry's label indices and its count. `render_sheet_pages` receives the entries expanded into labels, because every slot is drawn anyway.

### 4. Every job asks for exact treatment and states its copies

`PrintOptions` gains `copies: u32`. `print_job_request` puts `ipp-attribute-fidelity: true` in the operation group and the integer `copies` in the job group of every job: the unit's count for `single`, 1 for a sheet. `run_batch` sends one job per unit, as today, and on failure records all of the unit's label indices.

The user chose this over reading `copies-supported` and either splitting large counts into several jobs or refusing them before sending. With fidelity set, a printer that cannot honour the count, or any other attribute, refuses the job, and labeler reports those labels failed. Labeler does not judge printer capability or check the printer's reply for substitutions. A printer that accepts a fidelity job and then changes it is breaking the standard, and guarding against that is out of scope.

The flag goes on every job, not only those with more than one copy, so one rule covers all prints. That changes today's prints: a job a printer would accept with a substituted value, such as a different media size, now fails.

### 5. A job is sent once

The dispatch loop has no retry today, and none is added. A job whose reply never arrives fails like a refused one, with all its label indices.

### 6. Tests observe the real IPP wire

`FakeDriver` cannot show what goes on the wire. The `/print` tests that assert attributes, job count, media dimensions and failure mapping use a loopback IPP endpoint inside the test. It runs on the existing axum and `ipp` dependencies, answers `Get-Printer-Attributes` with fixed capabilities, parses and records each `Print-Job` with its attribute groups and payload, can refuse a chosen job, and can close the connection after reading a job without replying. The real `CupsDriver` sends to it. Each test names the regression that would make it fail.

### 7. Ordering with #430

#430's uncommitted plan also replaces the whole "Batch request" requirement of `rendering` (adding its continuation field) and adds a per-job finishing attribute. This change no longer touches `printing`'s "Printer capabilities" or "Render negotiation". Whichever change archives second must first rebuild its full MODIFIED "Batch request" block on the other's archived text, so neither field is lost. A finishing choice applies to the job, so a copies job carries it once and its copies share it. With fidelity on every job, #430's trim enum is only sent when the printer advertises it. Every job carries `copies` after this change, so #430's own finishing tests already run on copies jobs and need no copy here.

## Risks / Trade-offs

- [A printer without `copies` support refuses every job] → Its labels are reported failed instead of printed. The user accepted this: printer capability is the printer's business.
- [A printer substitutes today's `media-col` silently] → With fidelity the job now fails loudly, and the operator sees which labels did not print.
- [Copies on the same job share a precut and the server's cut rules] → That is the point of #433; the PT-2730 acceptance run confirms the physical result.

## Migration Plan

No stored data or configuration changes. The MCP adapter keeps sending `{data}` entries, which default to one copy. Physical acceptance is not part of this change's tasks. The loop runs unattended and ends with automated verification. Afterwards, in an interactive session with the owner, print one PT-2730 label with three copies and one two-label request with three copies each through labeler's `/print`, and record the submitted attributes and the owner-observed result (label count, cuts) under `.agent-runs/` before claiming hardware success.
