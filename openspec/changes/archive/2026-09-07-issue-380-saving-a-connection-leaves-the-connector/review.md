# Plan review

AUTHOR: claude
REVIEWER: codex
VERDICT: APPROVE_WITH_CHANGES
ROUNDS: 4

1. **[P2] Schema invalidation can retain an in-flight, pre-save response.** [design.md:57](/home/pfa/projects/labeler/.worktrees/issue-380/openspec/changes/issue-380-saving-a-connection-leaves-the-connector/design.md:57) specifies invalidation alone. If the first schema request is pending when a connection-details save succeeds, React Query reuses that request: its cancellation branch requires existing cached data ([query.ts:411](/home/pfa/projects/labeler/.worktrees/issue-380/ui/node_modules/@tanstack/query-core/src/query.ts:411)). The pre-save schema can therefore become the page’s schema, violating [specs/connections/spec.md:9](/home/pfa/projects/labeler/.worktrees/issue-380/openspec/changes/issue-380-saving-a-connection-leaves-the-connector/specs/connections/spec.md:9). An in-memory probe against the installed library produced one request, the old schema, and `isInvalidated: false`. Explicit cancellation before invalidation produced two requests and retained the new schema.

2. **[P2] Deletion still relies on a callback that can leave the editor’s schema observer alive.** [design.md:130](/home/pfa/projects/labeler/.worktrees/issue-380/openspec/changes/issue-380-saving-a-connection-leaves-the-connector/design.md:130) keeps `ConnectionsSection.onDeleted` as the mechanism closing the editor. However, the delete callback captures that handler when deletion starts ([ConnectionsSection.tsx:563](/home/pfa/projects/labeler/.worktrees/issue-380/ui/src/pages/connect/ConnectionsSection.tsx:563)), and the handler reads that render’s `editing` value ([ConnectionsSection.tsx:585](/home/pfa/projects/labeler/.worktrees/issue-380/ui/src/pages/connect/ConnectionsSection.tsx:585)). Start deletion with no editor open, then click Edit before completion: the captured handler sees `null` and closes nothing. Collapsing and reopening the block before completion also defeats the old callback. An in-memory React probe combining the proposed deletion event/removal with the existing callback reproduced a cleared page selection, an editor still mounted, a recreated cache entry, and schema requests increasing from one to two. This violates [specs/connections/spec.md:35](/home/pfa/projects/labeler/.worktrees/issue-380/openspec/changes/issue-380-saving-a-connection-leaves-the-connector/specs/connections/spec.md:35).

## Required changes

The author applies these edits; **NO further review follows**.

1. Amend the proposal and design so the save hook cancels the exact connection’s pending schema query before invalidating it, while retaining the save event and connections-list invalidation. Add a delta scenario and required regression test where an initial schema read remains pending across a successful connection-details save: a fresh request must start, and subsequently resolving the old request must not replace the new schema.

2. Amend the proposal and design so the mounted `ConnectionsSection` also handles `labeler:connection-deleted`, synchronously scheduling closure through a functional `setEditing` update that checks the **current** editor’s connection ID. Make this event handler own editor retirement instead of the per-call deletion callback. Add delta scenarios and required regression tests for opening the editor after deletion starts and for collapsing, reopening, and editing before deletion completes. With the list refetch delayed or failed, the matching editor must close, unrelated editors must remain open, and rerendering must neither recreate the deleted schema entry nor request it.

CHANGES_APPLIED: yes
SPECS_SHA256: 37a85aa83ce89d0cbacc8744e19844079310eaad839992871adeb9fba68a0fec
