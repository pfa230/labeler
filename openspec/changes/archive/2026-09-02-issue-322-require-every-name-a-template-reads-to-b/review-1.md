### Finding

- The plan leaves a normative contradiction in `param-resolution`. The new contract says undeclared bare tokens fail at load and every input names a declared parameter (`specs/interpolation-tokens/spec.md:127-151`, `specs/template-inputs/spec.md:28-31`). However, the existing preview requirement still says previews supply values “the template does not declare” and create placeholders for “every request field or declared parameter” (`openspec/specs/param-resolution/spec.md:405-413`). The proposal lists only `interpolation-tokens` and `template-inputs` as modified capabilities (`proposal.md:52-65`), so that contradictory requirement would survive unchanged. Because it already exists in the main specs, `openspec/config.yaml:65-66` requires a `MODIFIED` delta.

### Required changes

1. Add a `param-resolution` delta containing the complete `MODIFIED` requirement “A preview invents values, and says which ones, because no caller supplied any,” preserving all existing scenarios. Replace its opening claim with: “Every placeholder stands in for a parameter the template declares, and exactly three rules govern it.” Change rule 1 to begin: “Every declared parameter that a token reads and that the service has no value of its own for gets a placeholder.” Leave the remaining rules and scenarios unchanged.

2. Add `param-resolution` to `proposal.md`’s modified capabilities and state that this is a specification correction only; request acceptance remains owned by #324 and no render-path behavior changes.

3. Update `design.md` to record that the undeclared-preview wording is removed because such a template is now quarantined before any preview can be derived, while undeclared request keys remain accepted but unread.

The author applies these changes and NO further review follows.

VERDICT: APPROVE_WITH_CHANGES
