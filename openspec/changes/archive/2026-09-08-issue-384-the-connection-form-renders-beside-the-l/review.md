# Plan review

AUTHOR: claude
REVIEWER: codex
VERDICT: APPROVE_WITH_CHANGES
ROUNDS: 3

## Required changes

1. **[P1] Make editor identity follow navigation; unmounting is not automatic.** [design.md:88](/home/pfa/projects/labeler/.worktrees/issue-384/openspec/changes/issue-384-the-connection-form-renders-beside-the-l/design.md:88) relies exclusively on unmounting to suppress an old mutation’s navigation callback. Navigating between `/connections/A` and `/connections/B` can reuse the same component. The existing form initializes drafts from props once ([ConnectionsSection.tsx:106](/home/pfa/projects/labeler/.worktrees/issue-384/ui/src/pages/connect/ConnectionsSection.tsx:106)) and currently depends on an explicit key ([line 630](/home/pfa/projects/labeler/.worktrees/issue-384/ui/src/pages/connect/ConnectionsSection.tsx:630)). An in-memory reproduction with the installed libraries confirmed that navigating from B to A retained B’s draft and allowed B’s pending save to navigate away afterward. This matches [React’s state-preservation rules](https://react.dev/learn/preserving-and-resetting-state).

   Amend the design to key the subtree owning draft state, previews, and mutation observers by router location key and connection identity, including create mode. Add a spec scenario and planned regression covering history navigation between editor entries during a pending save: the destination displays its own stored values, retains no previous draft or preview, and the previous completion performs cache maintenance without navigating or changing the destination form.

2. **[P2] Reconcile the freshness contract with the intended eviction policy.** [connections/spec.md:313](/home/pfa/projects/labeler/.worktrees/issue-384/openspec/changes/issue-384-the-connection-form-renders-beside-the-l/specs/connections/spec.md:313) requires reads made after a write settles, but [line 369](/home/pfa/projects/labeler/.worktrees/issue-384/openspec/changes/issue-384-the-connection-form-renders-beside-the-l/specs/connections/spec.md:369) expressly requires reusing held answers after failure. Likewise, [default-connection/spec.md:101](/home/pfa/projects/labeler/.worktrees/issue-384/openspec/changes/issue-384-the-connection-form-renders-beside-the-l/specs/default-connection/spec.md:101) forbids either resolution input predating any management write, whereas [design.md:78](/home/pfa/projects/labeler/.worktrees/issue-384/openspec/changes/issue-384-the-connection-form-renders-beside-the-l/design.md:78) evicts only settings when changing the default. An unchanged cached connections list therefore satisfies the design but violates the delta.

   Preserve the explicitly intended assumption: failures evict nothing, and successful writes evict affected answers. Amend those clauses and [proposal.md:54](/home/pfa/projects/labeler/.worktrees/issue-384/openspec/changes/issue-384-the-connection-form-renders-beside-the-l/proposal.md:54) to require waiting for **all pending management writes**, then require each affected answer to come from a request started after the latest **successful write affecting that answer**. Explicitly permit reuse of unaffected answers and answers retained after failure.

The author applies these specific edits; **NO further review follows**.

CHANGES_APPLIED: yes
SPECS_SHA256: 8e05c93ec646b33bec3879affb23ebe8e85196be5d074283f371426e01e26d4f
