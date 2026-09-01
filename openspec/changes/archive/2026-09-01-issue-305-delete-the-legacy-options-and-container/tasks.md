## 1. Prove the refusals are absent

Write all four tests first and see each one fail on the current tree, where both spellings load. A
test whose subject is a refusal is the kind that most easily cannot fail (design.md — Decisions).

- [x] 1.1 Add a registry-load test in `src/templates.rs` for the top-level spelling, shaped like
      `superseded_shape_spellings_are_quarantined_at_registry_load`: write a valid template and one
      carrying a top-level `options:` map into a temp templates directory, load the registry, and
      assert the second is reported broken with an error naming `options` while the valid one still
      loads and is served.
- [x] 1.2 Add a registry-load test in `src/templates.rs` for the container spelling: a template whose
      `container` carries `option: { ... }` is reported broken with an error naming `option` and that
      item's layout path, and every other template in the directory still loads.
- [x] 1.3 Add an HTTP test in `src/lib.rs` (`yaml_post` / `build_app_in`) that `PUT`s a body carrying
      a top-level `options:` map over an already-stored template, and asserts the whole envelope:
      `422`, `error.code` `TemplateInvalid`, `error.details.reason` `template_parse_failed`, and a
      message naming `options`. Assert the stored file still holds its original bytes.
- [x] 1.4 Add an HTTP test in `src/lib.rs` that `PUT`s a body whose `container` carries `option:`,
      asserting the same envelope with a message naming `option` and the item's layout path, and that
      a create-only write (`If-None-Match: *`) left no file in the templates directory.
- [x] 1.5 Run the four tests against the unmodified tree and record that each fails, because today
      both spellings are accepted and desugared.

## 2. Delete the two spellings

- [x] 2.1 Remove `options: Option<RawOptions>` from `TemplateDefinitionRaw` and the `RawOptions`
      newtype beside it (`src/raw.rs:196`, `:204`).
- [x] 2.2 Remove `option: Option<BTreeMap<String, String>>` from `ContainerRaw` (`src/raw.rs:303`),
      leaving `when` and its `deserialize_when_map` untouched.
- [x] 2.3 Remove the fold of `raw.options` into `params` in `TryFrom<TemplateDefinitionRaw>`
      (`src/convert.rs:628`).
- [x] 2.4 Change `when: self.when.or(self.option)` to `when: self.when` in the container conversion
      (`src/convert.rs:300`).
- [x] 2.5 Confirm no reference to either deleted item remains: `RawOptions`, `raw.options` and
      `self.option` have no readers left, and nothing else was added in their place — no new error
      kind, no `details.reason` slug and no branch, since `deny_unknown_fields` is the whole refusal.

## 3. Confirm the deletion stopped where the plan says

- [x] 3.1 Confirm the out-of-scope neighbours are untouched: `TemplateContent::options()`
      (`src/templates.rs:90`) and `models::Options` (`src/models.rs:374`), the renderer's
      option-selection plumbing (`normalize_option`, `default_option_selection`, the `option:`
      arguments in `src/render/mod.rs`), the request-side `option` map on `LabelInput`
      (`src/models.rs:1222`, #214), the CSV `option.<name>` column (`src/api.rs:2733`), and the
      `options_not_supported` reason (`src/reason.rs:67`).
- [x] 3.2 Confirm no route, handler, OpenAPI model, status code or error code moved, and that no YAML
      under `catalog/` or `tests/fixtures/templates/`, no file in `docs/`, and no file in `ui/src`
      needed an edit.
- [x] 3.3 Re-run the four tests from group 1 and confirm each now passes.

## 4. Gates

- [x] 4.1 `cargo fmt`
- [x] 4.2 `cargo clippy --all-targets --all-features`
- [x] 4.3 `cargo test`
