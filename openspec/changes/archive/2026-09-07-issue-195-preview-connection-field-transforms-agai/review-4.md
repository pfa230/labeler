- The proposed freshness key fails after an edit is reverted. The design treats serialized rule content as sufficient and assumes every edit permanently changes that key ([design.md:294-305](/home/pfa/projects/labeler/.worktrees/issue-195/openspec/changes/issue-195-preview-connection-field-transforms-agai/design.md:294)). If request A starts with list L, the operator changes L to L′ and restores L before A resolves, A’s key again equals the current key; its sequence is also still current. The stale response is therefore displayed, violating the contract that any intervening edit invalidates an in-flight result ([spec.md:356-364](/home/pfa/projects/labeler/.worktrees/issue-195/openspec/changes/issue-195-preview-connection-field-transforms-agai/specs/connector-field-transforms/spec.md:356)).

### Required changes

- Replace the serialized-list freshness key in `design.md` and the “candidate-list keying” implementation language in `proposal.md` with a monotonic candidate-list revision. Increment it on every rule-field edit, addition, removal, or reorder; capture it when issuing a preview; and display the response only when both that revision and the per-rule request sequence remain current. Serialized rules may still form the request body, but must not serve as the freshness token.
- Expand the planned UI tests in `proposal.md` ([proposal.md:131-137](/home/pfa/projects/labeler/.worktrees/issue-195/openspec/changes/issue-195-preview-connection-field-transforms-agai/proposal.md:131)) to prove both asynchronous guarantees: an in-flight response remains discarded after editing and restoring the exact original list, and the earlier of two identical overlapping requests cannot overwrite the later response.

The author applies these changes and NO further review follows.

VERDICT: APPROVE_WITH_CHANGES
