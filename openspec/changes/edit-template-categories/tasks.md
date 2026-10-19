## 1. Source-preserving category edits

- [ ] 1.1 Add the direct yaml v2 dependency and a helper implementing the supported source forms in design.md. Verify add/change/clear with independently written full-source expected results for block/flow roots and lists, quoted keys, document markers, indentation, Unicode, CRLF and multiline strings; parse the results to assert the exact requested strings, including commas and quotes. Include the shared-anchor/alias/tag refusals. Whole-document reserialization, a wrong range or a missing separator must fail these checks. Update the existing source-read query to retain the BOM demonstrated lost by `Response.text()`, with a response-decoding and absent-key insertion case that fails if it is dropped.

## 2. Labels category editor

- [ ] 2.1 Add the per-card action and one Labels-level editor with string entries, Add/Remove, Save/Cancel and source/write error presentation, reusing source GET and `useReplaceTemplate`. Extend `Templates.test.tsx` to capture the exact PUT source, verify adding an absent key and clearing, successful summary/filter refresh including a removed active choice, and no write/success on Cancel or unsupported source. Exercise a failed PUT so a false success or discarded error fails; preserve the fetched categories as separate strings rather than comma-splitting them.

## 3. Gates

- [ ] 3.1 Run UI lint, tests and build, plus `npx --no-install openspec validate --all --strict --no-interactive`; retain results in `.agent-runs/`. Rust gates are needed only if implementation changes Rust, which this plan does not require.
