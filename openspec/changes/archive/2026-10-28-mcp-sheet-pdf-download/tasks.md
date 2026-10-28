# Tasks

## 1. Sheet PDF tool

- [x] 1.1 Extend the shared label entry with optional positive nonnullable `copies` for rendering and printing; add `render_sheet` argument decoding, read-only discovery and REST forwarding with the embedded PDF resource result; update the shared tool-name expectation and add mock MCP tests for the schema, forwarded option data and supplied/omitted copies and start slot, byte preservation, single-template ZIP refusal and REST errors. Verify with `cargo test --test mcp` and name the observable regression each new test detects.
- [x] 1.2 Exercise `render_sheet` against the real service using the existing sheet fixture and fake IPP receiver; select template options, supply copies and skip slots; assert a PDF blob result and zero print jobs. Verify with `cargo test --test mcp_integration`.

## 2. Integration

- [x] 2.1 Run the repository's configured loop checks and the required cut pass; verify all checks succeed and the diff contains only the `render_sheet` tool plus the shared label-entry `copies` change, their tests and this change's OpenSpec artifacts.
