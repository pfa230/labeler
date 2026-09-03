TREE_SHA256: 61a58ac536246530d766428125c74cec3288645716e74e122c71c927682d1cd4
SPECS_SHA256: c82193fbc94b8a43ea6d147da22cf6846fc3b6305fab7acb5e38b974b83e73c1

Findings (verified against live code):

No blocking defect. Diff fulfils the sequenced-params contract as repartitioned across `template-inputs`, `datetime-params` and `template-groups` (`docs/SPEC.md` §3.0).

1. **File shape / refusal** – `src/raw.rs:180` `RawParamEntry { name, #[serde(flatten)] spec }` `deny_unknown_fields` + `src/raw.rs:196` `Vec<RawParamEntry> #[serde(default)]` makes omission → `[]` and both `params: null` and mapping-shaped `params:` fail at `params` as Yaml (`serde_path_to_error` path `params`). `src/templates.rs:9135-9138` and `src/templates.rs:9114-9118` assert exactly that, and `src/lib.rs:5400-5460` `template_put_params_shape_and_duplicate_refusals` proves `PUT` gets `422 template_parse_failed` for both shapes. Matches `openspec/specs/template-inputs/spec.md:1724`.

2. **Duplicate-name as conversion-stage error** – `src/convert.rs:743-755` `for entry in raw.params { if contains_key → Err(Validation { path: params.<name>, msg: duplicate })}` → inside `parse_template` `src/parse.rs:25` → `src/api.rs:641` `TemplateParseFailed`, not `ValidationFailed`. Spec requires `template_parse_failed` naming duplicate (`spec.md:1726`). `src/convert.rs:1781` and `src/lib.rs:5442` assert path `params.title`/`params.zebra` and reason `template_parse_failed`.

3. **Declaration-order precedence** – `src/convert.rs:743` loop, `src/templates.rs:1006` `validate_params` iterating `IndexMap` insertion order, and `src/render/mod.rs:230` `for (name,spec) in &template.params` all iterate `IndexMap` (insertion-ordered, `Cargo.toml:18` `indexmap` with `serde`). Three reverse-alphabetical tests prove it: conversion `src/convert.rs:1788` zebra→alpha, validation `src/templates.rs:9175` zebra(enum defaults), coercion `src/render/mod.rs:11526` zebra/alpha integer, each asserting zebra surfaces and alpha does not.

4. **Input-list ordering** – `src/templates.rs:427` `for (name,spec) in &self.params { let Some(info)=collected.get(name) else continue }` pushes in declaration order; `src/templates.rs:514` deleted `sort_by name`. `src/templates.rs:9065` asserts `inputs.default`, `inputs.all` and `derive_inputs_for_label` are `title, subtitle, code` while layout reads `code→subtitle→title`, plus `TemplateSummary`/`TemplateDetail` `src/templates.rs:2389,2420` `map(|(name,spec)| ParamEntry)` in same order.

5. **Wire array on both endpoints** – `src/models.rs:62,101` `Vec<ParamEntry>` (no `skip_serializing_if`) serialises `[]` not `null`/`{}`; `src/templates.rs:2392,2422` and `src/openapi.rs:116` `ParamEntry` schema. `src/lib.rs:1708` `template_list_and_detail_params_array_shape_and_empty_broken` asserts every `templates[].params` is_array and `brother_24mm_qr` detail is `["code","message"]` declaration order, identical on summary/detail.

6. **UI preserves wire order** – `ui/src/api/types.ts:78,94` `Array<{name}&ParamSpec>`; `ui/src/pages/TemplateDetail.tsx:286` `detail.params.map` with no `sort`; `ui/src/pages/print/FieldForm.tsx:36,60` `activeInputs = inputs ?? detail.inputs.default` `map` with no sort; `Import.tsx:136` / `Connect.tsx:153` walk `inputs` grids – no sorting left (`rg sort` shows none).

7. **Fixtures/docs** – 16 YAML files + `docs/AUTHORING.md` rewritten to `- name:` form (5 catalog + 11 fixtures, verified `git diff --stat`); no mapping example remains (`grep params docs/AUTHORING.md` all sequence).

`cargo test` 870 passed, `cargo clippy` clean [verified].

No `params: null` alias, no second spelling, no `store.rs` touch, `BTreeMap`→`IndexMap` preserves O(1) lookups per design. Minor: `validate_param_name` invalid-char case surfaces as `ValidationFailed` (second stage) rather than `Yaml`; spec does not fix the reason for that sub-case and quarantine still occurs, so not a contract breach.

VERDICT: APPROVE
