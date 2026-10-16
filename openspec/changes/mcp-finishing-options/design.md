## Context

See `proposal.md` for motivation and `specs/mcp/spec.md` for the delta. `PrintLabelsArgs` already models optional, nonnullable `start_slot` with `Option`, the generic `non_null` deserializer and schema attributes. `print_labels` builds its REST body explicitly. REST already owns finishing selection and capability validation.

## Goals / Non-Goals

**Goals:** Reuse the existing optional-argument pattern so discovery, parsing and forwarding agree on the finishing argument.

**Non-Goals:** New tools, adapter-side printer probing, copy-count support, or changes to REST/IPP finishing behavior.

## Decisions

### Reuse the existing nonnullable optional-field pattern

Add `chain_and_continue: Option<bool>` using the same serde and schemars attributes as `start_slot`. Insert it into the REST body only when present, preserving both true and false. A defaulted `bool` is shorter but loses the distinction between absent and explicitly false required by the MCP forwarding contract. An ordinary `Option<bool>` would accept explicit null and publish a nullable schema. Reusing `non_null` avoids a second parser or new abstraction.

### Keep finishing decisions in REST

Explain true versus false/omitted in the tool description and `docs/MCP.md`. Use the existing print request and result paths, including the existing error mapping for unsupported continuation. A new finishing enum, tool or capability-discovery flow adds an interface the existing REST boolean does not need; hardcoding ordinary printing leaves the requested selection unavailable.

### Verify the adapter boundary

Extend `tests/mcp.rs` to check the discovered schema and description, exact forwarded JSON for true/false/omitted, and refusal of null/string/number before HTTP. Reuse existing mock REST and MCP-process helpers. These checks fail if schema optionality or type changes, an argument is dropped/coerced/defaulted, or invalid input reaches REST. Existing MCP tests cover unchanged service-error preservation; existing REST/IPP tests own finishing serialization and printer capability rules. Reuse that coverage rather than duplicating it for this field.

## Risks / Trade-offs

- Schema drift between optionality and null handling: verify the published schema as well as calls over MCP.
- Clients discover the option without a new probe tool: REST's existing capability validation returns the existing refusal when the selected printer cannot continue.
