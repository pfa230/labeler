### Adversarial Code Review: Post-Rebase & Date Control Edits (`issue-200-ui-when-gating`)

This adversarial review evaluates only the two sets of edits made on top of the previously reviewed implementation:
1. **Rebase across 46 commits onto `main`** (ADR-0079 token grammar, ADR-0073 template ID & directory groups, ADR-0080/ADR-0081 layout extent resolution).
2. **Date control fix** (`ui/src/api/types.ts` gaining `"date"` and dropping `time`, with branching across `ParamInput.tsx`, `PrintForm.tsx`, `templateFields.ts`, `Import.tsx`, `Connect.tsx`, and test fixture corrections).

---

### 1. Rebase onto `main` & Token Derivation Refactor

#### 1.1 Conflict Resolutions & Router / OpenAPI Registration
- [`src/api.rs:236-241`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/api.rs#L236-L241): The route `/templates/{id}/inputs` is registered with `post(template_inputs)`.
- [`src/api.rs:1200-1206`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/api.rs#L1200-L1206): `thumbnail` handler delegates placeholder generation to [`template.placeholder_data()`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L156-L181).
- [`src/api.rs:1249-1282`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/api.rs#L1249-L1282): `template_inputs` enforces the `MAX_BATCH_LABELS` cap, loads the template, and invokes [`template.derive_inputs_for_label(&label.data, now)`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L140-L154) per label.
- [`src/openapi.rs:37-43`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/openapi.rs#L37-L43), [`src/openapi.rs:105-110`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/openapi.rs#L105-L110): All models (`TemplateInputs`, `InputSpec`, `InputControl`, `TemplateInputsRequest`, `TemplateInputsResponse`) and the `template_inputs` endpoint are registered with `utoipa`.
- [`docs/adr/README.md:82`](file:///home/pfa/projects/labeler/.worktrees/issue-200/docs/adr/README.md#L82): ADR-0070 is cataloged in numerical sequence.

#### 1.2 Porting Token-Derivation Sites to ADR-0079 Grammar (`src/templates.rs`)
The nine token-derivation and inspection call sites in [`src/templates.rs`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs) have been rewritten onto `crate::interpolation::scan_tokens` and `crate::interpolation::parse`:
- **Helper functions**:
  - [`src/templates.rs:20-31`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L20-L31): [`bare_token_names`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L20) extracts `Source::Bare` names while ignoring `vars.*` and `sys.*` roots and ignoring ill-formed tokens.
  - [`src/templates.rs:34-45`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L34-L45): [`vars_token_keys`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L34) parses and extracts `Source::Vars` keys.
  - [`src/templates.rs:49-53`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L49-L53): [`is_datetime_param`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L49) tests whether a name maps to `ParamType::Datetime`.
- **Derivation call sites**:
  1. [`src/templates.rs:110`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L110) (`variables()` on `Text`/`Qr`): uses `vars_token_keys(value)`.
  2. [`src/templates.rs:115`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L115) (`variables()` on `Image` `src`): uses `vars_token_keys(src)`.
  3. [`src/templates.rs:290`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L290) (`derive_inputs_internal` on `Text` item): extracts names with `bare_token_names(value)`.
  4. [`src/templates.rs:309`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L309) (`derive_inputs_internal` on `Qr` item): extracts names with `bare_token_names(value)`.
  5. [`src/templates.rs:330`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L330) (`derive_inputs_internal` on `Image` `src`): extracts names with `bare_token_names(s)`.
  6. [`src/templates.rs:499`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L499) ([`collect_single_line_names`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L489)): extracts single-line token names with `bare_token_names(value)`.
  7. [`src/templates.rs:4072`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L4072) (`check_items` on `Text`): tests coverage of `bare_token_names(value)` in `inputs.all`.
  8. [`src/templates.rs:4092`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L4092) (`check_items` on `Qr`): tests coverage of `bare_token_names(value)` in `inputs.all`.
  9. [`src/templates.rs:4122`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L4122) (`check_items` on `Image` `src`): tests coverage of `bare_token_names(s)` in `inputs.all`.

#### 1.3 Signature Cleanup & Test Fixture Alignment
- [`src/templates.rs:489`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L489): [`collect_single_line_names`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L489) takes only `layout: &Layout`.
- [`src/templates.rs:4035-4039`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L4035-L4039): [`check_items`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L4035) takes only `items`, `inputs_all_names`, and `template_id`.
- [`src/render/mod.rs:50-56`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/render/mod.rs#L50-L56): [`resolve_parameters_mode`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/render/mod.rs#L50) accepts `template: &TemplateContent`, called seamlessly by [`derive_inputs_for_label`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L145).
- [`src/render/helpers.rs`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/render/helpers.rs): Legacy `scan_interpolation_tokens` and its unit test were removed.
- [`src/render/mod.rs:6304-6334`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/render/mod.rs#L6304-L6334): [`advertised_fields_token_grammar_test`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/render/mod.rs#L6304) is ported to test `template.inputs_all()` directly.
- Test fixtures in [`src/templates.rs`](file:///home/pfa/projects/labeler/.worktrees/issue-200/src/templates.rs#L4154-L4400) and `tests/fixtures/templates/` have all top-level `id:` fields removed to match ADR-0073.

---

### 2. Date Control Refactor & UI Branching

#### 2.1 Type Definitions (`ui/src/api/types.ts`)
- [`ui/src/api/types.ts:25-29`](file:///home/pfa/projects/labeler/.worktrees/issue-200/ui/src/api/types.ts#L25-L29): [`InputControl`](file:///home/pfa/projects/labeler/.worktrees/issue-200/ui/src/api/types.ts#L25) gains `"date"` alongside `"datetime"`.
- [`ui/src/api/types.ts:31-44`](file:///home/pfa/projects/labeler/.worktrees/issue-200/ui/src/api/types.ts#L31-L44): [`InputSpec`](file:///home/pfa/projects/labeler/.worktrees/issue-200/ui/src/api/types.ts#L31) does not carry `time`.

#### 2.2 Component & Form Branching
- [`ui/src/components/ParamInput.tsx:212-228`](file:///home/pfa/projects/labeler/.worktrees/issue-200/ui/src/components/ParamInput.tsx#L212-L228): Correctly selects `"date"` input for `control === "date"` (and for `ParamSpec` with `time === false`) and `"datetime-local"` for `control === "datetime"` (and `ParamSpec` with `time === true`).
- [`ui/src/pages/print/PrintForm.tsx:25-30`](file:///home/pfa/projects/labeler/.worktrees/issue-200/ui/src/pages/print/PrintForm.tsx#L25-L30): `initialDataFromInputs` handles `input.control === "date"` with `formatLocalDate(now)` and `input.control === "datetime"` with `formatLocalDateTime(now)`.
- [`ui/src/lib/templateFields.ts:1-10`](file:///home/pfa/projects/labeler/.worktrees/issue-200/ui/src/lib/templateFields.ts#L1-L10): [`hasServerDefault`](file:///home/pfa/projects/labeler/.worktrees/issue-200/ui/src/lib/templateFields.ts#L1) includes `spec.control === "date"` and `spec.control === "datetime"`.
- [`ui/src/pages/Import.tsx:143`](file:///home/pfa/projects/labeler/.worktrees/issue-200/ui/src/pages/Import.tsx#L143) & [`ui/src/pages/Connect.tsx:174`](file:///home/pfa/projects/labeler/.worktrees/issue-200/ui/src/pages/Connect.tsx#L174): In `validateRow`, cell validation for both `"datetime"` and `"date"` controls passes through [`datetimeCellError`](file:///home/pfa/projects/labeler/.worktrees/issue-200/ui/src/lib/templateFields.ts#L79).

#### 2.3 Test Fixtures & Grid Tests
- [`ui/src/lib/labelInputs.test.ts:55-60`](file:///home/pfa/projects/labeler/.worktrees/issue-200/ui/src/lib/labelInputs.test.ts#L55-L60): Pruning test uses `{ name: "day", control: "date" }` and `{ name: "printed_on", control: "datetime" }` without synthetic `time` properties.
- [`ui/src/pages/print/FieldForm.test.tsx:150-162`](file:///home/pfa/projects/labeler/.worktrees/issue-200/ui/src/pages/print/FieldForm.test.tsx#L150-L162): Tests rendering of both `datetime` (`datetime-local`) and `date` (`date`) inputs from actual `InputSpec` definitions.
- [`ui/src/pages/Import.test.tsx:517-528`](file:///home/pfa/projects/labeler/.worktrees/issue-200/ui/src/pages/Import.test.tsx#L517-L528): Adds the grid test verifying that an unparseable cell on a `"date"` control flags a cell error and blocks the download/submit run.

---

### Verification Summary
- **Rust Backend**: `cargo test` passes (637 passed, 0 failed), `cargo clippy --all-targets --all-features` passes with 0 warnings, `cargo fmt --check` clean.
- **Frontend**: `npm --prefix ui run lint` passes clean, `npm --prefix ui run build` passes clean, `npm --prefix ui test` passes (400 passed across 49 test files).
- **OpenSpec & Process Gates**: `openspec validate issue-200-ui-when-gating --strict` passes; `.workflow/review-gate-check.sh issue-200-ui-when-gating` passes with valid `SPECS_SHA256`.

VERDICT: APPROVE

