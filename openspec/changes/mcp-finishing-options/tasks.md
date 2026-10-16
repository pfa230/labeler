## 1. MCP finishing selection

- [x] 1.1 Add optional nonnullable `chain_and_continue` to `PrintLabelsArgs` using the existing `start_slot` pattern, explain both choices in the tool description, and forward the field only when supplied. Extend `tests/mcp.rs` to verify discovery exposes an optional boolean without a null default, true/false reach REST unchanged, omission stays absent, and null/string/number calls return execution errors with zero HTTP requests. Run `cargo test --test mcp`; these assertions must fail for schema drift, dropped/defaulted arguments or invalid-input forwarding.
- [x] 1.2 Update the `print_labels` row and finishing guidance in `docs/MCP.md`; review against tool discovery and the delta to confirm true selects chain and continue, false/omitted selects ordinary printing, and unsupported continuation is a service refusal.

## 2. Required gates

- [x] 2.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, and `npx --no-install openspec validate --all --strict --no-interactive`; record actual outcomes before checking this task.
