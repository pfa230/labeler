TREE_SHA256: 8d30340cc3f7988b178240f50f8a31c06890918fbf11982829fa0463ae9c7da1

## Diff review: issue-305-delete-the-legacy-options-and-container (round 2)

Both round-1 findings are fixed and I verified the fixes rather than trusting the log.

### Round-1 findings, re-checked

- **Finding 1 (blocking) — resolved.** `src/templates.rs:2910-2911` now reads `contains("layout[0]") && contains("unknown field \`option\`")`, and `src/lib.rs:3255` the same for the HTTP body. The tautological `||` is gone and bare `contains("option")` no longer matches `options` as a substring. This matches the neighbouring convention at `src/templates.rs:6303`, `:6384`. The top-level pair pins `unknown field \`options\`` at `src/templates.rs:2844` and `src/lib.rs:3197`. [verified]
- **Finding 2 (non-blocking) — resolved.** `src/lib.rs:3259` asserts `std::fs::read_dir(&dir).unwrap().count() == 0`, not one filename. [verified]

### What I verified clean

- **Red before green, run rather than deduced.** I copied the tree to `/tmp`, reverse-applied the `src/raw.rs` + `src/convert.rs` diff, and ran the four tests: `0 passed; 4 failed`. Both registry tests fail at `registry.len() == 1` (the legacy template loads, so it is 2); both HTTP tests fail at the status assertion (`src/lib.rs:3193`, `:3249`). Each inverts on the post-change tree: `3 passed` / `2 passed` by name here. Scratch copy deleted. [verified]
- **Gates on this tree**: `cargo fmt --check` exit 0, `cargo clippy --all-targets --all-features` exit 0, `cargo test` 762 passed / 0 failed / 2 ignored. [verified]
- **The deletion is exactly the plan and nothing else.** `src/raw.rs` loses the field, `RawOptions`, and `ContainerRaw::option`; `src/convert.rs:300` is `when: self.when` and the fold at `:625` is gone. `rg 'RawOptions|raw\.options|self\.option'` over `src/` leaves only `src/api.rs:2718,2733`, the CSV `option.<name>` path the proposal keeps. No new error kind, reason slug or branch. [verified]
- **No YAML anywhere spells either key**: `rg '^\s*options?:'` over all `.yaml`/`.yml` outside `target/` returns nothing. `docs/AUTHORING.md` §9 teaches only `when:`; `src/openapi.rs` never mentioned `options`. Task 3.2 holds. [verified]
- **`parse_and_validate` runs before the write lock** (`src/api.rs:772` → `:639`), so "nothing written" is structural, not incidental. [verified]
- **Delta hygiene.** The `MODIFIED` requirement is byte-faithful against `openspec/specs/template-groups/spec.md` apart from the struck `options` row plus the new paragraphs and scenarios — I diffed the extracted requirement bodies; no name or wording drift for `archive-merge-check.sh` to trip on. `specs-digest.sh` recomputes `89e47a88…`, matching `review.md`, so `specs/` was not edited after the plan verdict. [verified]

### Finding (non-blocking) — task 1.5's box claims a record that does not exist

`tasks.md` 1.5 says "Run the four tests against the unmodified tree and **record** that each fails," and is checked. Nothing records it: `.agent-runs/implement-opencode.log` (13 lines) mentions only the green run, and no other artifact carries a red run. Round 1 flagged it `[assumption, by deduction]`; it stayed an assumption in round 2. The claim is true — my reverted-tree run above is the evidence, and the failure lines are quoted there, so the record now exists in `diff-review.md` rather than nowhere. Not worth another round on its own.

### Not findings, recorded so they are not re-derived

- Five delta scenarios have no dedicated test (`option` on a non-container item, `option: {}`, `when` + `option` together, and the two "the correct spelling still loads" ones). `tasks.md` requires four tests and no more; the plan review approved that scope, and round 1 confirmed the first three against a compiled probe. Adding tests now would be work nobody planned, not a defect in the diff.
- `openspec/specs/template-inputs/spec.md:418` names an `options` key, but it is about the `GET /api/templates/{id}` response body carrying none — not the template field. The proposal's "only place in `openspec/specs/`" claim is about the field spelling and holds. [verified]
- `docs/SPEC.md:1106` mentions both deleted spellings in its frozen changelog. Correctly untouched; that file is frozen.
- The container HTTP test builds its request inline instead of via `yaml_post` because it needs `If-None-Match: *`. Task 1.4 names `yaml_post` as the shape, not a mandate; the deviation is forced and harmless.

VERDICT: APPROVE
