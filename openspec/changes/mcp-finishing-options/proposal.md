## Why

MCP clients cannot choose the chain-and-continue finishing already available in Labeler's REST API and print form because `print_labels` rejects that argument. Agents need the same chain or ordinary-print choice.

Fixes #450

## What Changes

- Add optional boolean `chain_and_continue` to `print_labels` discovery and argument handling.
- Forward supplied true or false unchanged to `POST /api/print`; omit the field when absent and reject null or non-boolean values before REST.
- Explain true versus false/omitted in the tool description and `docs/MCP.md`, using the existing REST finishing behavior and refusal handling.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `mcp`: Publish and forward the optional finishing selection on `print_labels`.

## Impact

The adapter's `PrintLabelsArgs`, generated tool schema, print description and request construction in `src/bin/labeler-mcp.rs`; adapter tests in `tests/mcp.rs`; and `docs/MCP.md`. No dependency or REST/IPP changes are needed.
