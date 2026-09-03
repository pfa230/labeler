1. The plan leaves a concrete discovered defect untracked. `design.md:356-374` identifies contradictory preview requirements, calls the defect worthy of its own issue, then says filing it is not this stage’s responsibility. That violates `AGENTS.md:47-51` and `openspec/config.yaml:50-51`, which require out-of-scope work discovered during planning to become a GitHub issue rather than remain only in documentation.

### Required changes

- File a GitHub issue specifically for reconciling the `param-resolution` preview requirement with the existing thumbnail behavior. Update `design.md:370-374` to cite that issue while retaining the decision that #213 does not change this unrelated behavior.

The author applies this change and NO further review follows.

VERDICT: APPROVE_WITH_CHANGES
