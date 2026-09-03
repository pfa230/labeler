## 1. Raw parsing — sequence shape

- [x] 1.1 Add `RawParamEntry { name: String, #[serde(flatten)] spec: RawParamSpec }` with `deny_unknown_fields` in `src/raw.rs` and change `TemplateDefinitionRaw.params` from `Option<BTreeMap<String, RawParamSpec>>` to `Vec<RawParamEntry>` with `#[serde(default)]` so omission defaults to empty while `params: null` fails deserialization
- [x] 1.2 Verify `params: null` is quarantined on load and rejected on `PUT /api/templates/{id}` with `422 TemplateInvalid` `template_parse_failed`, and a mapping-shaped `params:` is refused with `template_parse_failed` naming the file and `params` path

## 2. Domain conversion — ordered container and duplicate handling

- [x] 2.1 Update `TryFrom<TemplateDefinitionRaw>` in `src/convert.rs` to iterate `Vec<RawParamEntry>` in declaration order, build an order-preserving container for `TemplateContent.params` (e.g. `IndexMap<String, ParamSpec>` or `Vec<ParamEntry>` + index), and refuse duplicate `name` during conversion as a conversion-stage error mapping to `template_parse_failed` (do not widen the shared parse/validation classifier)
- [x] 2.2 Ensure all 13 `params.get`/`contains_key` sites (`src/templates.rs:429,1416,1448,1478,1490,1506,1521,1540,1729,1744`, `src/render/mod.rs:170,6203`) continue to work via the ordered container without linear scans

## 3. Models and wire — array shape on summary and detail

- [x] 3.1 Change `TemplateContent.params`, `TemplateSummary.params`, and `TemplateDetail.params` in `src/models.rs` from `BTreeMap<String, ParamSpec>` to `Vec<ParamEntry>` (`name` + flattened `ParamSpec`) preserving declaration order; an omitted or empty `params:` is published as `[]` and never omitted nor as an object
- [x] 3.2 Update `src/templates.rs` `From<&TemplateDefinition>` for `TemplateSummary` and `build_detail` to serialize `params` as an array in declaration order with identical order on summary and detail
- [x] 3.3 Update `src/openapi.rs` schemas for `TemplateSummary`/`TemplateDetail` to array shape for `params`

## 4. Template validation and input-list ordering

- [x] 4.1 Change `src/templates.rs` `validate_params` and related loops to iterate `params` in declaration order
- [x] 4.2 Change `derive_inputs_internal` ordering from `sort_by(|a,b| a.name.cmp(&b.name))` (`src/templates.rs:512`) to declaration order for `inputs.default`, `inputs.all`, and `POST /api/templates/{id}/inputs`; `variables` stays ascending and `param_defaults` stays keyed by name
- [x] 4.3 Make error surfacing declaration-order: where multiple parameter declarations would error (conversion, `validate()` forbidden attributes, and render-time coercion at `src/templates.rs:1008`, `src/convert.rs:743`, `src/render/mod.rs:230`), report the declaration-order first error; add reverse-alphabetical multi-error cases for each stage (`zebra` before `alpha` alphabetically inverted)

## 5. Tests — ordering and refusals

- [x] 5.1 Update input-list ordering tests to declare `title, subtitle, code` while layout first reads `code`→`subtitle`→`title`, and assert the list is `title, subtitle, code`
- [x] 5.2 Add tests that `params: null` and mapping-shaped `params:` are quarantined and rejected on write with `template_parse_failed`, and that duplicate `name` is quarantined/rejected with `template_parse_failed` naming the duplicate (conversion-stage)
- [x] 5.3 Add tests that reverse-alphabetical multiple errors surface the declaration-order first name for conversion, template validation, and render-time coercion

## 6. Fixtures, catalog, and docs

- [x] 6.1 Rewrite 16 YAML files under `catalog/` and `tests/fixtures/templates/` from mapping to sequence form (`- name: …` entries) and update inline Rust fixtures that embed `params:` YAML
- [x] 6.2 Update worked examples in `docs/AUTHORING.md` to sequence form

## 7. UI — consume array in declaration order without sorting

- [x] 7.1 Update `ui/src/api/types.ts` `TemplateSummary.params` and `TemplateDetail.params` from `Record<string, ParamSpec>` to `Array<{name:string} & ParamSpec>`
- [x] 7.2 Verify `ui/src/pages/TemplateDetail.tsx:286` Parameters card renders `title, subtitle, code` in wire (declaration) order without sorting
- [x] 7.3 Verify `ui/src/pages/print/FieldForm.tsx:61` print form renders the three controls as `title, subtitle, code` in input-list (declaration) order without sorting
- [x] 7.4 Verify Import grid (`ui/src/pages/Import.tsx:136`) and Connect grid (`ui/src/pages/Connect.tsx:153`) walk the input list and preserve `title, subtitle, code` order without sorting

## 8. Verification

- [x] 8.1 Run `cargo fmt`, `cargo clippy --all-targets --all-features`, and `cargo test`
- [x] 8.2 Run `npm run lint` and `npm test` in `ui/` (when UI was touched) and verify `GET /api/templates` and `GET /api/templates/{id}` both return `params` as `[]` or `[…]` in declaration order and `broken` is empty

