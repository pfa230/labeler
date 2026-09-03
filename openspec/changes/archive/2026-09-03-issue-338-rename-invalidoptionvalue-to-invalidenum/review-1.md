1. `openspec/changes/issue-338-rename-invalidoptionvalue-to-invalidenum/specs/template-inputs/spec.md:175` calls `InvalidEnumValue` “unchanged from today,” contradicting the breaking rename declared at `proposal.md:9`. That sentence would publish a false history.

2. `proposal.md:30` and `design.md:28` intentionally leave stale operational documentation. `docs/AUTHORING.md:753` identifies `InvalidOptionValue` as the applicable code, and `docs/AUTHORING.md:766` tells users to expect it. After implementation, both statements will be false. `AGENTS.md:49-51` also forbids leaving deferred work as an untracked “if desired” follow-up.

### Required changes

1. Replace `template-inputs/spec.md:175` with wording that identifies `422 InvalidEnumValue` as the new code defined by `enum-validation`, without claiming it is unchanged.
2. Include `docs/AUTHORING.md` in scope: update lines 753 and 766 to `InvalidEnumValue`, and revise `proposal.md:30` and `design.md:28` accordingly. This documentation-only alignment introduces no additional behavioral change.

The author applies these changes and NO further review follows.

VERDICT: APPROVE_WITH_CHANGES
