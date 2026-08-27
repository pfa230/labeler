# 79. Token grammar: namespaces, system values, and format syntax

**Status:** Accepted. Supersedes [ADR-0028](0028-datetime-interpolation-token.md) (in part: syntax and resolution precedence) and [ADR-0068](0068-datetime-parameter-type.md) (in part: token list and formatting syntax).

## Context

Issue #239 and OpenSpec change `issue-239-token-grammar` addressed ambiguities and legacy behaviors in template interpolation:

1. **Token grammar and ambiguity:** Previously, tokens like `{datetime.long_date}` used dot separation for both namespaces (`vars.<key>`) and formats (`datetime.<format>`), leading to ambiguity when parameters and data fields were introduced.
2. **Reserved words vs bare names:** The word `datetime` was reserved, preventing templates or connectors from using `datetime` as an ordinary data field or parameter name.
3. **Format attachment syntax:** As datetime parameters were added (ADR-0068), parameter formatting was conflated with dotted field navigation.

A single, canonical token grammar is required across the server, UI, and documentation.

## Decision

The interpolation token grammar is formally defined as:

```text
token       := "{" value-path [ ":" format-name ] "}"
value-path  := bare-name | root "." key
bare-name   := ^[a-zA-Z0-9_-]+$
format-name := ^[a-zA-Z0-9_-]+$
root        := "vars" | "sys"
```

1. **Namespace roots:** `vars` and `sys` are the only two recognized namespace roots (case sensitive, exact match).
   - `vars.<key>` resolves from the variables store.
   - `sys.now` resolves the server request's captured instant. `sys` is a closed set containing only `now`.
2. **Format attachment (`:`):** Formatting patterns configured in `datetime_formats` are attached via a colon (`:`), e.g. `{sys.now:short_date}` or `{printed_on:short_date}`.
   - A format attached to any value that is not an instant (`sys.now` or declared `type: datetime` parameter) is rejected at load time.
3. **No reserved words:** `datetime`, `vars`, and `sys` are valid bare parameter and data field names. `{datetime}` resolves an ordinary data field or parameter named `datetime`.
4. **Load-time validation & helpful refusal messages:**
   - `{datetime.<name>}` is an unknown source error pointing to `{sys.now:<name>}` as replacement.
   - `{sys.now.<name>}` is an unknown system value error pointing to `{sys.now:<name>}` as replacement.
   - Malformed tokens (empty segment, trailing colon, multiple colons, whitespace) quarantine the template file at load time or return `422 TemplateInvalid` (`template_validation_failed`) on template writes.
5. **Connector transforms:** Capture group names in connector field transforms must match `^[a-zA-Z0-9_-]+$`. Reserved namespace checks (`datetime`, `vars.`, `datetime.`) are removed.

## Consequences

- Templates using the old syntax (`{datetime.iso_date}`, `{param.short_date}`) must be updated to `{sys.now:iso_date}` and `{param:short_date}`.
- Single-pass, non-fallthrough token resolution at render time eliminates ambiguous precedence rules.
- The UI and backend share the unified grammar and data field semantics, with the UI using best-effort token scanning for form discovery and field warning analysis.
