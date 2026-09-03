TREE_SHA256: 61a58ac536246530d766428125c74cec3288645716e74e122c71c927682d1cd4
SPECS_SHA256: c82193fbc94b8a43ea6d147da22cf6846fc3b6305fab7acb5e38b974b83e73c1

Findings:

1. **[BLOCKING] Out-of-scope file in diff — `ui/src/app/toast.tsx` is not part of the contracted change**
   - Evidence: `git diff HEAD` shows `ui/src/app/toast.tsx:1-40` adds `useEffect`, `timeouts` ref and cleanup for toasts. `openspec/changes/archive/2026-09-03-issue-360-the-input-list-is-ordered-by-name-so-the/proposal.md:58-66` Impact lists `src/raw.rs`, `src/convert.rs`, `src/models.rs`, `src/templates.rs`, `src/openapi.rs`, `src/parse.rs`, `catalog/`, `tests/fixtures/templates/`, `docs/AUTHORING.md`, `ui/src/api/types.ts`, `ui/src/pages/TemplateDetail.tsx`, `ui/src/pages/print/FieldForm.tsx` — `toast.tsx` is absent. `tasks.md:36-40` (§7) does not list it. `openspec/changes/.../specs/*` deltas do not name it. Per `AGENTS.md` “One change, one worktree, one issue” and OpenSpec workflow, a change folder is the contract; an unrelated UI leak fix must be a separate issue/PR, not ride this wire-shape break. Remove it from this branch or file a dedicated issue and re-apply after.

2. **[Non-blocking, confirm] `src/models.rs:62` / `101` — `params: Vec<ParamEntry>` wire shape is correct but relies on derive to emit `[]`**
   - Verified: `TemplateSummary.params` and `TemplateDetail.params` no longer `skip_serializing_if`; empty vec serializes as `[]`, satisfying `openspec/changes/.../specs/template-inputs/spec.md:31` “An omitted or empty `params:` SHALL be published as an empty array; the field SHALL be present as `[]` and never omitted”. No omission bug. `src/templates.rs:2386-2416` (`From<&TemplateDefinition>` / `build_detail`) maps `IndexMap` to `Vec<ParamEntry>` in declaration order — correct.

3. **[Non-blocking, confirm] Declaration-order error precedence implemented correctly**
   - `src/convert.rs:743-755` iterates `Vec<RawParamEntry>` in file order and checks duplicate before `ParamSpec::try_from`, returning `TemplateError::Validation { path: "params.<name>" }` — `src/api.rs:640-645` maps `parse_template` (`TryFrom`) failures to `Reason::TemplateParseFailed`, so duplicate on file load quarantines as `template_parse_failed` and on `PUT` returns `422 TemplateInvalid` `template_parse_failed` per `openspec/changes/.../specs/template-inputs/spec.md:29`. Verified by `src/lib.rs:5458-5499` and `src/convert.rs:1755-1808` tests.
   - `src/templates.rs:1006-1009` `validate_params` loops `for (name,spec) in &self.params` (an `IndexMap` — insertion order) and `src/render/mod.rs:230` `for (name,spec) in &template.params` do the same for coercion, satisfying `spec.md:33` “No path that iterates `params` SHALL report errors in name order”. Tests `src/templates.rs:9174-9203` and `src/render/mod.rs:11535-11570` assert `zebra` before `alpha` for validation and coercion. Input-list ordering `src/templates.rs:427-428` walks `&self.params` and skips unread entries, preserving declaration order (`spec.md:196`) — test `src/templates.rs:9036-9101` asserts `title, subtitle, code`.

No other contract mismatches found on wire `params` file shape (`src/raw.rs:178-200` `Vec<RawParamEntry>` with `#[serde(default)]` — omission → `[]`, `params: null` / mapping → `TemplateError::Yaml` at `params`), `src/models.rs:256-261` `ParamEntry`, or catalog/fixtures rewrite.

VERDICT: REVISE
