## 1. Stdio adapter and tool contract

- [x] 1.1 Add the `labeler-mcp` binary and the minimal official Rust MCP SDK dependencies; verify initialization, tools-capability negotiation, clean protocol-only stdout and termination on stdin closure in a spawned-process test, plus a locked build with the pinned compiler.
- [x] 1.2 Implement service-origin configuration and bearer-header forwarding; verify the default origin, an explicit HTTP(S) origin, invalid-origin startup failure and token-present/token-absent requests using isolated process environments and a mock REST service; assert captured diagnostics/results contain no token.
- [x] 1.3 Publish the four specified tools, descriptions, argument schemas and annotations; verify discovery through an MCP client and assert unknown tools and malformed call envelopes produce protocol errors, while unlisted arguments, missing required fields and null `start_slot` produce `isError: true` execution results, all without an HTTP call; verify nested JSON values and arbitrary `data` names survive forwarding.

## 2. REST-backed operations

- [x] 2.1 Implement template and printer list tools; verify exact methods/paths and unchanged JSON results, including every template's published parameters and quarantined entries, against mock REST responses.
- [x] 2.2 Implement `render_label` as a PNG-only preview; verify request body/query fidelity, decoded MCP image byte equality, `image/png` content, rejection of a successful preview response failing the PNG signature check and propagation of a sheet refusal, with no print request emitted.
- [x] 2.3 Implement `print_labels`; verify explicit printer/labels forwarding, omitted-versus-present `start_slot`, full and partial print summaries and their `isError` flags, and that a dropped response after POST receipt never produces a second print POST.
- [x] 2.4 Map service and connection failures into MCP execution errors; verify `401`, `404`, `413`, indexed `422 BatchInvalid`, undecodable JSON responses, preview responses failing the PNG signature check and an unavailable service, asserting unchanged REST error objects/statuses and that failures do not terminate subsequent tool calls.

## 3. Integration and delivery

- [x] 3.1 Exercise the adapter against a local real labeler service with an MCP client and a controlled fake IPP receiver; verify filled preview output, successful batch dispatch, zero dispatch for an invalid batch, and job-log attribution for a personal token and no-auth actor `local`; record which observable break each integration assertion detects.
- [x] 3.2 Add setup documentation linked from the README covering a locked native build, Claude Code stdio registration with placeholder environment values, all four tools, single-template preview limits, print error/uncertainty behavior and the human acceptance procedure from design.md; verify the locked build and that Claude Code accepts the documented registration command in isolated client configuration, saving redacted command results under `.agent-runs/`.
- [x] 3.3 Add `mcp` to the spec-domain lists in `AGENTS.md` and `openspec/config.yaml`; verify both lists contain it and the diff changes only their domain enumeration.
- [x] 3.4 Run the cut pass, review the resulting diff, and run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` and `npx --no-install openspec validate --all --strict --no-interactive`; verify all pass and every retained mechanism names its requirement before the loop's patch review and archive.
