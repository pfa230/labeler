## 1. Discovery and identifier routing

- [ ] 1.1 Add the seven template tools and their strict argument types and descriptions; verify through a spawned MCP client that discovery lists exactly eleven tools with the required fields and effect annotations, and that missing, null, wrong-type and unlisted arguments fail without HTTP calls.
- [ ] 1.2 Reuse the template-id validator and implement one encoded identifier-segment builder for all six path-based tools; verify ordinary ids, underscore/hyphen ids and `reload` reach the intended captured method/path, while invalid ids and path traversal are refused before HTTP. Publish the same identifier constraint in discovery.

## 2. Read operations

- [ ] 2.1 Implement `get_template`; verify unchanged JSON detail, including parameters, variables and an additional response field, plus preservation of REST not-found errors.
- [ ] 2.2 Implement `get_template_source`; verify exact source text with comments and trailing whitespace, source access independent of a broken template's failed detail, not-found propagation, and an execution error for invalid UTF-8 rather than lossy decoding.
- [ ] 2.3 Implement `get_template_thumbnail` using the existing PNG handling; verify an unconditional GET, decoded image byte equality and MIME type, non-PNG success-body refusal, service-error propagation and zero print requests.

## 3. Mutation operations

- [ ] 3.1 Implement create and complete-source replacement with raw `text/yaml` bodies; verify exact YAML bytes and complete response JSON for 201/200, preserve 409/404/422/500 error objects, and assert no create/replace fallback or YAML rewriting occurs.
- [ ] 3.2 Implement deletion; verify a 204 with no body yields successful empty MCP content, a 404 is preserved, and a later tool call still works.
- [ ] 3.3 Implement registry reload; verify complete JSON including extra fields and served/broken counts, success with nonzero `broken_count`, and propagation of a failed reload's service error.
- [ ] 3.4 Retain disabled redirects and automatic retries for all four template mutations; verify each emits only one request after a dropped response or a redirect, a redirect target receives no replay, and a lost response reports an unknown outcome while the adapter remains usable.

## 4. Real-service authoring and repair

- [ ] 4.1 Exercise create → detail/source → replace → filled PNG preview/thumbnail → delete through the actual adapter against an isolated real service; assert stored source bytes, published detail, decoded image output and final file/registry removal, and assert duplicate create and invalid YAML leave the existing file unchanged.
- [ ] 4.2 Exercise the lifecycle of the valid id `reload` through the SDK and actual HTTP client, including all six path-based tools; assert creation writes `reload.yaml` rather than merely reloading, and independently verify `reload_templates` still invokes registry reload.
- [ ] 4.3 Seed a quarantined file, read its source and repair it through replacement; verify it becomes served after the write, reload reports served/broken counts, and a sheet thumbnail has the expected one-slot dimensions without printing.
- [ ] 4.4 Verify template writes through a personal bearer token and no-auth mode against the real service, and verify a revoked token returns the service's 401 before a file changes; keep credentials out of captured diagnostics/results.

## 5. Documentation and validation

- [ ] 5.1 Update `docs/MCP.md` with all eleven tools, exact arguments, a complete-source create/read/edit/replace/delete example, broken-template repair, thumbnail/reload semantics and unknown mutation outcomes; link `docs/AUTHORING.md` and review the examples against the discovered schemas and integration calls.
- [ ] 5.2 Run the cut pass and review the diff; verify `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` and `npx --no-install openspec validate --all --strict --no-interactive` pass, with each retained mechanism mapped to its requirement and each test naming the observable break it detects.
