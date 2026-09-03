TREE_SHA256: fe9908851fa4de205021c66305924ea9eaeb59649802ae1680522c41ada5b6db
SPECS_SHA256: c82193fbc94b8a43ea6d147da22cf6846fc3b6305fab7acb5e38b974b83e73c1

Verified correct against proposal, specs, design, tasks and AGENTS.md:

- `src/raw.rs:180-196` `RawParamEntry{name,spec}` with `deny_unknown_fields` and `TemplateDefinitionRaw.params: Vec<RawParamEntry>` `#[serde(default)]` correctly makes omission `[]` and refuses `params: null` and mapping-shaped `params` at path `params` as `TemplateError::Yaml` -> `template_parse_failed` (`src/parse.rs:27-34`, `src/api.rs:640-645`) per `specs/template-inputs/spec.md:27-28`. Spec scenario for `params: null` and mapping refused is satisfied (`src/templates.rs` quarantine tests, `src/lib.rs:5450` PUT refusal asserts `template_parse_failed`).

- `src/convert.rs:743-755` iterates `Vec<RawParamEntry>` in declaration order into `IndexMap`, refuses duplicate `name` at `params.{key}` as `Validation` inside `parse_template` so `api.rs:640` maps to `template_parse_failed` naming duplicate, consistent with `list-params` precedent and `specs/template-inputs/spec.md:29`. Reverse-alphabetical multi-duplicate test at `src/convert.rs:1781` asserts `zebra` first.

- `src/models.rs:59,101` `Vec<ParamEntry>` with `name+flatten` and no `skip_serializing_if` publishes `[]` never omitted on both `TemplateSummary` and `TemplateDetail` (`src/templates.rs:2386,2416` build arrays via `IndexMap` iteration preserving declaration order, identical on summary/detail). `src/openapi.rs:17,113` registers `ParamEntry` array; `ui/src/api/types.ts:78,94` `Array<{name}&ParamSpec>` required, `ui/src/pages/TemplateDetail.tsx:282` walks `detail.params.map` without sorting. Catalog and `tests/fixtures/templates/` (16 files, e.g. `catalog/tape/brother/brother_24mm.yaml:5`) and `docs/AUTHORING.md:80,142` rewritten to `- name:` form.

- `src/templates.rs:427` `derive_inputs_internal` now iterates `&self.params` (IndexMap) and removed `sort_by` at former `:512`, filtering via `collected` while preserving declaration order for `inputs.default`/`all` and `POST /inputs`. `validate_params:1006` and `src/render/mod.rs:230` (render-time coercion) likewise iterate `IndexMap` so error precedence is declaration-order first. Reverse-alphabetical validation at `src/templates.rs:9164` (`zebra` before `alpha` with forbidden `format`) and render coercion at `src/render/mod.rs:11459` (`zebra` before `alpha` both `integer` coercion) assert declaration order; they passed (`cargo test` 865 passed).

- AGENTS.md breaking-change rule respected: map spelling dropped, `deny_unknown_fields` gives parse error naming file/key, no migration/alias/desugaring; `store.rs` untouched. Prior `diff-review-1` rebase revert of `enum-validation`/`InvalidEnumValue` is now fixed: `HEAD` equals `origin/main` `4c387ec`, both expose `CODE_INVALID_ENUM_VALUE` (`src/errors.rs:18`).

No contract violation remains.

VERDICT: APPROVE
