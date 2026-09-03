TREE_SHA256: 4545f66038d94a676e1179163bbf9bfc2a0fe03c24a2d14a1cfa1da472eb88e6
SPECS_SHA256: c82193fbc94b8a43ea6d147da22cf6846fc3b6305fab7acb5e38b974b83e73c1

Findings against proposal, specs, design, tasks and AGENTS.md:

**[BLOCKING] Stale base reverts `enum-validation` / `InvalidEnumValue` rename (AGENTS.md rebase violation)**
Branch is behind `origin/main`. `HEAD` lacks `openspec/specs/enum-validation/spec.md` and uses `InvalidOptionValue` (`src/errors.rs:18` `CODE_INVALID_OPTION_VALUE`, `src/render/mod.rs:353/1216` `invalid_option_value`), while `origin/main` has the capability from #338 (`git show origin/main:src/errors.rs:18` `CODE_INVALID_ENUM_VALUE`, `openspec/specs/enum-validation/spec.md:1` exists, `openspec/specs/template-inputs/spec.md:366` `InvalidEnumValue`). `git diff origin/main --stat` shows deletion of `openspec/specs/enum-validation/spec.md` and revert of `openspec/specs/template-inputs/spec.md:366/450` to `InvalidOptionValue`. Landing this diff would revert #338. `AGENTS.md: rebase onto main; never merge main into itself` and `rebase before diff review so reviewed tree is landed tree` is violated. Must `git rebase origin/main`, restore `enum-validation` capability, keep `InvalidEnumValue` in code (`src/errors.rs:18`, `src/render/mod.rs:353`) and in delta/spec text, and update the change's `MODIFIED template-inputs` rendering paragraph to reference `InvalidEnumValue` as `origin/main` does.

**[BLOCKING] Delta would land stale spec text**
`openspec/changes/issue-360-the-input-list-is-ordered-by-name-so-the/specs/template-inputs/spec.md:366` currently references `InvalidOptionValue`; after rebase it must reference `InvalidEnumValue` to match `origin/main:openspec/specs/template-inputs/spec.md:366`. Archiving the current delta on current `main` overwrites `enum-validation`'s contract.

**[Non-blocking] Clippy warning not fixed**
`cargo clippy --all-targets --all-features` emits `src/lib.rs:1730` `unnecessary_map_or` (`body["broken"].as_array().map_or(false, |b| b.is_empty())` should be `is_some_and`). `AGENTS.md: run fmt/clippy/test before reporting; fix root cause` requires it to be fixed (one-line `is_some_and`).

Verified correct (no finding):
`src/raw.rs:196` `Vec<RawParamEntry>` with `#[serde(default)]` correctly refuses `params: null` and mapping-shaped `params` as `TemplateError::Yaml` path `params` -> `template_parse_failed` (`src/templates.rs:9104`, `src/lib.rs:5347`). `src/convert.rs:743-755` builds `IndexMap` in declaration order and refuses duplicate `name` as `TemplateError::Validation` path `params.{key}` which `src/api.rs:640-645` maps via `parse_template` to `TemplateParseFailed` (`src/lib.rs:5388` asserts `template_parse_failed`). `src/templates.rs:427` iterates `&self.params` (IndexMap) and removed `sort_by` at former `:512`, so `inputs.default`/`all` and `POST /inputs` are declaration order; `validate_params:1006` and `src/render/mod.rs:230` loops likewise give declaration-order first error. Reverse-alphabetical multi-error cases exist for all three stages (`src/convert.rs:1781`, `src/templates.rs:9164`, `src/render/mod.rs:11459`). Wire `src/models.rs:59/98` `Vec<ParamEntry>` with `#[serde(flatten)]` publishes `[]` never omitted on both summary/detail (`src/templates.rs:2386/2416`), `src/openapi.rs:17/113` registers `ParamEntry` array, `ui/src/api/types.ts:78/94` and `ui/src/pages/TemplateDetail.tsx:282` preserve wire order without sorting, catalog/fixtures and `docs/AUTHORING.md` rewritten to `- name:` form.

One blocking finding requires rebase; no other contract violation found.

VERDICT: REVISE
