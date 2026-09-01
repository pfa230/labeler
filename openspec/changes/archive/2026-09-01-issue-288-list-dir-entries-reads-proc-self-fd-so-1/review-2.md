1. [verified] The delta forbids “any path” naming the directory ([spec.md:19](/Users/pfa/projects/labeler/.worktrees/issue-288/openspec/changes/issue-288-list-dir-entries-reads-proc-self-fd-so-1/specs/template-registry/spec.md:19)), but the chosen implementation internally calls `openat(fd, ".")` ([design.md:77](/Users/pfa/projects/labeler/.worktrees/issue-288/openspec/changes/issue-288-list-dir-entries-reads-proc-self-fd-so-1/design.md:77)). The implementation therefore violates the literal contract.

2. [verified] The delta requires every entry to be reported and the listing to be complete ([spec.md:29](/Users/pfa/projects/labeler/.worktrees/issue-288/openspec/changes/issue-288-list-dir-entries-reads-proc-self-fd-so-1/specs/template-registry/spec.md:29), [spec.md:32](/Users/pfa/projects/labeler/.worktrees/issue-288/openspec/changes/issue-288-list-dir-entries-reads-proc-self-fd-so-1/specs/template-registry/spec.md:32)), while the design deliberately filters `.` and `..` and omits non-UTF-8 names ([design.md:83](/Users/pfa/projects/labeler/.worktrees/issue-288/openspec/changes/issue-288-list-dir-entries-reads-proc-self-fd-so-1/design.md:83), [design.md:96](/Users/pfa/projects/labeler/.worktrees/issue-288/openspec/changes/issue-288-list-dir-entries-reads-proc-self-fd-so-1/design.md:96)). “Complete” must distinguish clean traversal from intentional filtering.

3. [verified] The proposal says a mid-iteration error changes a previous answer into `500 template_registry_io` ([proposal.md:96](/Users/pfa/projects/labeler/.worktrees/issue-288/openspec/changes/issue-288-list-dir-entries-reads-proc-self-fd-so-1/proposal.md:96)), but the migration section claims no request that succeeds today starts failing ([design.md:300](/Users/pfa/projects/labeler/.worktrees/issue-288/openspec/changes/issue-288-list-dir-entries-reads-proc-self-fd-so-1/design.md:300)). That deployment statement contradicts the declared behavior change.

### Required changes

1. Replace the path prohibition with a rule forbidding enumeration through a pathname resolved independently of the existing descriptor, including `/proc/self/fd/<n>` and `/dev/fd/<n>`, while explicitly allowing descriptor-relative reopening such as `openat(fd, ".")`.

2. State that traversal aliases and non-UTF-8 names remain intentionally omitted, exact stored spelling applies to retained names, and completeness means iteration reaches its normal end without a read error.

3. Replace the migration claim with: requests encountering a mid-iteration read failure may now return `500 template_registry_io` where they previously returned an answer derived from a truncated listing.

The author applies these changes, and NO further review follows.

VERDICT: APPROVE_WITH_CHANGES
