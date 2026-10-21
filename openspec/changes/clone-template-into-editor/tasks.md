## 1. Immediate clone workflow

- [ ] 1.1 Add the name-value source splice using the existing yaml dependency and source decoding. Verify one plain name and one block-scalar name with independently written expected source including comments, description and the next key; only the name span may differ. Whole-source serialization or consuming the next key/comment must fail the assertions. Do not add shared-reference handling or special-case tests.
- [ ] 1.2 Add Clone to template detail, generate matching copy id/name suffixes, advance on POST 409 only, disable Clone while the editor is dirty, and navigate on success with the copied YAML seeding equal initial draft/baseline values and an open disclosure. Extend TemplateDetail tests with a real route transition, a repeated clone/collision sequence, captured POST body, source/create errors, and an unsaved edit that disables Clone until saved or discarded. Assert the original receives no write, descriptions/comments remain unchanged, each successful copy is distinct, and non-conflict errors stay visible on the original; wrong suffixes, false success or a closed/unseeded editor must fail.

## 2. Integration and gates

- [ ] 2.1 Run UI lint, tests and build, plus `npx --no-install openspec validate --all --strict --no-interactive`. No new Rust code or test harness is required.
