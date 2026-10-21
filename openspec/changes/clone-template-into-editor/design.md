## Context

See proposal.md for motivation. Template detail already loads raw source, deletes templates and hosts `RawYamlSection`. That section is keyed by the route id, with local draft/baseline state and a disclosure that normally starts closed. `useCreateTemplate` already submits raw YAML and seeds the new id's source cache. POST refuses occupied filenames, including broken templates absent from served summaries. No embedded id belongs in the body.

The existing yaml v2 dependency and BOM-preserving source decoding support source-preserving browser edits. The same documented [node-range API](https://eemeli.org/yaml/#content-nodes) can locate `name` without serializing the document. Local scalar-range probes preserved the following key/comment for plain, quoted and multiline names, flow mappings, BOM/Unicode and CRLF. No application clone has been implemented or rendered in this scoping session.

## Goals / Non-Goals

Use existing GET/POST, a name-value splice and a small entry point to the existing editor. No clone endpoint, naming dialog, description rewrite, second editor, rename operation, generic patch framework, rollback or automatic retry of an uncertain failed create.

## Decisions

- **User decisions:** Clone immediately with automatic ids/names. Ignore the shared-name-reference concern: do not add anchor/alias/tag detection, refusal, reference rewrites or special-case tests for it. #406's category-form refusals remain specific to category editing and must not be copied into cloning.
- **Routine defaults:** Try `<source-id>-copy`/`<source-name> (copy)`, then integer suffixes starting at 2 in both. Use POST's 409 as the authoritative occupied-filename result and advance only on that response; a preliminary served-template list cannot see broken files. Build every candidate from the same original source. Reuse the existing create query rather than a second HTTP/error/cache implementation. Other errors stop and display through the detail page's existing notice patterns.
- Use yaml's parsed root key/value range to replace only `name` with a JSON-quoted new string, preserving the remainder verbatim. Retain a consumed line terminator for block scalars; use the existing BOM-preserving source decoding. Do not use the category helper's restrictive form policy or introduce a configurable/general-purpose YAML patcher. The parsed source's name and the selected id supply the naming base. Regex can match an unrelated multiline value, while whole-document serialization changes comments/formatting.
- On successful create, navigate to the copy with only an open-for-editing flag in router state. Use the existing source query/cache to initialize RawYamlSection's equal draft and baseline; useCreateTemplate already seeds that cache for the new id. Never put YAML in router history, because history state survives reload and Back/Forward. A revisit must use the current saved source, as required by the existing Unsaved YAML navigation contract, rather than replaying clone-time YAML. Open the disclosure in edit mode for the clone flag; ordinary detail visits keep their existing behavior. Keep route-id keying so a prior template's draft cannot be reused against the new id.
- **User decision:** Disable Clone while the current YAML editor has unsaved changes; the operator saves or discards edits before cloning. Clone reads the stored source. Reuse the existing editor dirty-state signal; create nothing and add no confirmation flow while dirty. Also keep Clone unavailable during its in-flight operation and when the displayed detail is placeholder data for a different route id. This uses the detail query's existing placeholder flag and prevents the actual route/detail mismatch already documented beside RawYamlSection. On source/parse errors send no create; on a create error do not issue a compensating delete or retry anything except the guaranteed-no-write filename conflict. Existing atomic creation owns write integrity.

## Risks / Trade-offs

The generated filename id cannot be changed in the YAML editor; the user selected the immediate workflow instead of a naming dialog.

Extend existing TemplateDetail tests with captured GET/POST bodies, one conflict followed by success, another clone of the same original, and source/non-conflict failure. An original-id write, description change, wrong suffix or navigation before successful create must fail. Exercise the real route transition and seeded editor, not a mocked navigate callback alone. No Rust changes or new rendering harness are needed.
