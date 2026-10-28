# Tasks

## 1. Sheet PDF tool

- [ ] 1.1 Add `render_sheet` argument decoding, read-only discovery and REST forwarding with the embedded PDF resource result; update the shared tool-name expectation and add mock MCP tests for the schema, forwarded data and supplied/omitted start slot, byte preservation, single-template ZIP refusal and REST errors. Verify with `cargo test --test mcp` and name the observable regression each new test detects.
- [ ] 1.2 Exercise `render_sheet` against the real service using the existing sheet fixture and fake IPP receiver; assert a PDF blob result and zero print jobs. Verify with `cargo test --test mcp_integration`.

## 2. Integration

- [ ] 2.1 Run the repository's configured loop checks and the required cut pass; verify all checks succeed and the diff contains only the PDF-download adapter change, its tests and this change's OpenSpec artifacts.
