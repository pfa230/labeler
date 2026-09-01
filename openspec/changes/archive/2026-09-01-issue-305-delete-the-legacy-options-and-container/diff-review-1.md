TREE_SHA256: 0471239186d37c2b0f558205564733e4455eb08af9bd6d25519b0a018d768bb0

## Diff review: issue-305-delete-the-legacy-options-and-container

### What I verified clean

- The deletion itself is exactly the plan: `src/raw.rs` loses `TemplateDefinitionRaw::options`, `RawOptions` and `ContainerRaw::option`; `src/convert.rs:300` becomes `when: self.when` and the `raw.options` fold at `:625` is gone. No leftover readers (`grep RawOptions|raw\.options|self\.option` in `src/` and `ui/src` is empty). [verified]
- Gates green on this tree: `cargo fmt --check` 0, `cargo clippy --all-targets --all-features` 0, `cargo test` 762 passed / 0 failed / 2 ignored. [verified]
- Out-of-scope neighbours untouched: `TemplateContent::options()` (`src/templates.rs:1110`, `src/render/mod.rs:1108`), the renderer `option:` arguments, the request-side `option` map, CSV `option.<name>`, `options_not_supported`. Diff is 4 files, no route/OpenAPI/error-code movement (`src/api.rs:224-241`, `:725-741`). [verified]
- `PUT /api/templates/{id}` really is the only write door (no `POST /templates` in the router, `src/api.rs:226-240`), and `parse_and_validate` runs before the write lock (`src/api.rs:772` → `:639`), so "nothing written" holds structurally. [verified]
- Delta hygiene: `MODIFIED` for *A group is a directory under the templates directory* is a byte-faithful copy of `openspec/specs/template-groups/spec.md` with only the `options` row struck plus the new paragraphs/scenarios; `conditional-visibility` does not yet exist in `openspec/specs/`; `SPECS_SHA256` recomputes to the value in `review.md` (`89e47a88…`), so the plan verdict is not stale; `review-gate-check.sh --plan-only` passes. [verified]
- Spec claims I could not see a test for are nonetheless true. Compiling a probe against `target/debug/deps/liblabeler-*.rlib` and calling `labeler::parse::parse_template`: `option` on a `text` item, `option: {}`, and `when` + `option` together each produce `yaml error at layout[0]: layout[0]: layout: unknown field \`option\` …`. The top-level case produces `yaml error at options: options: unknown field \`options\`, expected one of \`name\`, …`. [verified]
- Red-before-green for the four tests is derivable rather than recorded: with the fields restored, `has_options.yaml` and `has_container_option.yaml` load, so `registry.len()` is 2 and `broken.len()` 0, and the PUTs no longer answer `422 template_parse_failed`. Each assertion inverts. [assumption, by deduction; no run log kept for task 1.5]

### Finding 1 (blocking) — the layout-path assertion in both container tests cannot fail

`src/templates.rs:2915` and `src/lib.rs:3259`:

```rust
broken_entry.error.contains("layout[0]") || broken_entry.error.contains("layout"),
msg.contains("layout[0]") || msg.contains("layout"),
```

The first disjunct implies the second, so each assertion reduces to `contains("layout")`. The word `layout` is emitted by serde_yaml_ng itself as the field name inside the message body (`… layout: unknown field \`option\` …`), so it survives even if the `serde_path_to_error` path is dropped entirely. The assertion therefore passes against code that no longer names the offending item's layout path, which is precisely what `tasks.md` 1.2 and 1.4 claim was tested and what both delta scenarios make normative ("in an error naming `option` and that item's layout path", `specs/conditional-visibility/spec.md`).

This also breaks the design's own justification for the test: "The added container test is exactly that alarm."

The fix is verified safe. The real string today is `yaml error at layout[0]: layout[0]: layout: unknown field \`option\` at line 14 column 3`, and the HTTP body carries the same string (`src/api.rs:640` maps `err.to_string()` into the message), so a conjunction passes now. The surrounding tests in both files already use exactly that form and are the convention this one departs from: `src/templates.rs:6307`, `:6388`, `src/lib.rs:2896`, `:3007` all read `err_str.contains("layout[0]") && err_str.contains("unknown field \`color\`")`.

Suggested shape, matching the neighbours:

```rust
broken_entry.error.contains("layout[0]") && broken_entry.error.contains("unknown field `option`")
```

That fixes a second weakness in the same two assertions at `src/templates.rs:2909` and `src/lib.rs:3255`: bare `contains("option")` matches as a substring of `options`, whereas ``contains("unknown field `option`")`` pins the key exactly.

### Finding 2 (non-blocking) — the create-only test checks one filename, not that nothing was written

`src/lib.rs:3263`: `assert!(!dir.join("new_tpl.yaml").exists())`. Task 1.4 and the delta scenario say no file is created at all. `temp_templates_dir()` (`src/lib.rs:2402`) starts empty and `build_app_in` uses `Store::open_in_memory()` (`src/lib.rs:2420`), so nothing else can land in that directory; asserting `std::fs::read_dir(&dir).unwrap().count() == 0` is the faithful check and is a one-liner. As written the test would miss a write to any other path.

### Not findings, recorded so they are not re-derived

- The issue's instruction to "delete the tests that assert the desugaring" has no referent: the only YAML `options:` / `option:` keys in Rust are the four new tests (`src/templates.rs:2821`, `:2888`, `src/lib.rs:3179`, `:3232`). `design.md` fact 1 is correct and the diff correctly deletes nothing. [verified]
- No YAML under `catalog/` or `tests/fixtures/`, no file in `docs/` (including `AUTHORING.md` §9, which teaches only `when:`), and nothing in `ui/src` spells either deleted key, so task 3.2's "no edit needed" holds. [verified]
- `docs/AUTHORING.md:758` still restates §5's lazy missing-field rule that `template-inputs` superseded. Pre-existing drift, untouched by this diff and deliberately out of the requirement's scope per `design.md`. Worth an issue, not a change here.

VERDICT: REVISE
