# Tasks

## 1. Clarify result presentation

- [x] 1.1 Derive grid result-column visibility from current row annotations and pass the originating operation's heading from Import. Retain the existing annotation mapping and clearing rules, and keep successful Download toast-only. Extend existing grid/Import tests for initial absence, print and download-validation headings, successful-download absence and disappearance after the last annotation is cleared; wrong headings, stale visible results or per-row download-success marks must fail.
- [x] 1.2 Make each failed cell's complete message available through a keyboard-operable button/native popover using current palette tokens. Update the existing failure-cell assertions and exercise a long row failure with pointer and keyboard in a browser, recording the complete text and interaction under `.agent-runs/`; clipped or inaccessible details must fail the check.

## 2. Integration gates

- [x] 2.1 Run `npm --prefix ui run lint`, `npm --prefix ui run test`, `npm --prefix ui run build` and `npx --no-install openspec validate --all --strict --no-interactive`; require success before committing implementation.
