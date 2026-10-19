## 1. Immediate clone workflow

- [ ] 1.1 Add the name-value source splice using the yaml dependency/source decoding from #406. Verify plain, quoted and multiline names with independently written expected source including comments, description, next key, Unicode/CRLF and BOM; only the name span may differ. Whole-source serialization or consuming the next key/comment must fail the assertions. Do not add shared-reference handling or special-case tests.
- [ ] 1.2 Add Clone to template detail, generate matching copy id/name suffixes, advance on POST 409 only, and navigate on success with the copied YAML seeding the existing editor's initial draft/baseline and open disclosure. Extend TemplateDetail tests with a real route transition, a repeated clone/collision sequence, captured POST body and source/create errors. Assert the original receives no write, descriptions/comments remain unchanged, each successful copy is distinct, and non-conflict errors stay visible on the original; wrong suffixes, false success or a closed/unseeded editor must fail.

## 2. Integration and gates

- [ ] 2.1 Create an ordinary literal-name/literal-text copy through the existing API and compare original/copy PNGs with identical fixed render data before editing; retain the source and output evidence in `.agent-runs/`. Run UI lint, tests and build, plus `npx --no-install openspec validate --all --strict --no-interactive`. No new Rust code or test harness is required.
