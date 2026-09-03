## Findings

1. The frozen-spec ownership is contradictory. `specs/datetime-params/spec.md:7-13` both claims the complete §3.0 contract and transfers container ownership; `specs/template-inputs/spec.md:7` also supersedes §3.0. Meanwhile `specs/template-groups/spec.md:132-134` names only `template-inputs`, omitting the existing `datetime-params` and `interpolation-tokens` supersessions. This contradicts `design.md:56-58` and the unambiguous precedence required by `AGENTS.md:19-27`.

2. The proposed raw type accepts a spelling the spec forbids. `proposal.md:59` proposes `Option<Vec<RawParamEntry>>`, but Serde treats a missing field and explicit YAML null alike for plain `Option` (`src/models.rs:173-177`). Consequently, `params: null` would become an empty parameter list, contradicting the sequence-only/no-second-spelling contract at `specs/template-inputs/spec.md:20-28`.

3. Duplicate-name HTTP classification contradicts the chosen implementation stage. The spec promises `template_validation_failed` (`specs/template-inputs/spec.md:22`), while the design detects duplicates during raw-to-domain conversion (`design.md:46-50`). Conversion happens inside `parse_template` (`src/parse.rs:25-34`) and every such failure maps to `template_parse_failed`; only later `validate()` failures become `template_validation_failed` (`src/api.rs:640-645`). The published conversion-stage precedent explicitly confirms this behavior (`openspec/specs/list-params/spec.md:62-72`).

4. Core ordering outcomes lack discriminating scenarios. The input-list scenario declares `title, subtitle, code` but does not specify a conflicting first-use order (`specs/template-inputs/spec.md:252-257`), so it cannot detect a layout-first-use implementation. The issue also requires the print form, Import grid, and Connect grid to retain that order (`.agent-runs/issue-360.md:63-69`), but the only new UI scenario covers the Parameters card (`specs/template-inputs/spec.md:55-58`). Finally, declaration-order error precedence is normative at `specs/template-inputs/spec.md:26` and required by `.agent-runs/issue-360.md:78-80`, yet no scenario or test scope at `proposal.md:67` exercises it.

## Required changes

1. Partition §3.0 explicitly and uniquely: assign its opening declaration/container example to `template-inputs`, its per-entry/type table to `datetime-params`, and its namespace rules to `interpolation-tokens`. Remove the duplicated container rules from the datetime requirement and update the template-groups authority paragraph to name all three owners.

2. Replace the proposed raw `Option<Vec<RawParamEntry>>` with a defaulted `Vec<RawParamEntry>` so omission produces an empty vector while explicit null fails deserialization. Add a scenario requiring `params: null` to be quarantined and rejected on write with `template_parse_failed`.

3. Change the duplicate-name write reason to `template_parse_failed`, consistently describing it as a conversion-stage validation failure. Do not widen the shared parse/validation classifier solely for duplicates.

4. Make the ordering scenario declare `title, subtitle, code` while the layout first reads them in a different order, then require `title, subtitle, code`. Add normative scenarios requiring the print form, Import grid, and Connect grid to preserve input-list order without sorting. Add reverse-alphabetical multi-error scenarios for conversion, template validation, and render-time coercion, and include those cases in the proposal’s test scope.

The author applies these changes; NO further review follows.

VERDICT: APPROVE_WITH_CHANGES
