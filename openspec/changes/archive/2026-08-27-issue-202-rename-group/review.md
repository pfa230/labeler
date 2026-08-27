## Review Metadata

- **Round**: 3
- **Prior round**: round 2 verdict REVISE (author claude, reviewer codex): three Critical, three Moderate. Roles were then swapped: codex revised the artifacts, claude reviews.

AUTHOR: codex
REVIEWER: claude

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/template-groups/spec.md, design.md, plus openspec/specs/template-groups/spec.md, openspec/specs/template-registry/spec.md, src/reason.rs, src/fs_safe.rs, src/api.rs, src/templates.rs, ui/src/pages/Templates.tsx, ui/src/api/queries.ts
- **Issue**: #202


## Findings

### Critical (blocking)

1. **The MODIFIED Labels-view requirement now asserts two incompatible controls, and pins one of them to today's implementation.** The block still carries the inherited tree contract verbatim: "It SHALL show the groups as a tree: `All`, every group the service lists, nested to match the directory structure", "every node inside the tree SHALL be identified by its group path", "An empty group SHALL appear in the tree like any other" (`specs/template-groups/spec.md`, Labels-view requirement, opening paragraph). The revision then adds, in the same requirement: "This change attaches the affordance to the flat full-path button toolbar that the Labels view currently implements; it SHALL NOT require building a new nested-tree component."

   One normative requirement cannot both mandate a nested tree and describe its own control as a flat toolbar. An implementer reading the block cannot satisfy it as written. Two further problems compound it: "the flat ... toolbar that the Labels view currently implements" states the current implementation rather than required behavior, so it becomes false the moment the tree gap is closed and it does not belong in a spec at all; and "SHALL NOT require building a ... component" is a constraint on the change process, not on observable behavior, so nothing can test it.

   The scope decision itself is right and is already recorded in the correct places: `proposal.md` states the tree gap is out of scope, and `design.md:189` owns it under "The existing group-filter toolbar owns the rename affordance". Only the normative delta needs correcting.

### Moderate

1. **The case-conflict message loses the existing group's name, which the current spec guarantees.** The current normative spec requires the refusal be reported "naming the existing group", and its scenario asserts "the message names `Shipping/Warehouse`" (`openspec/specs/template-groups/spec.md:400`). The revision changes that same scenario to "the message names the requested path `Shipping/warehouse`" and makes the same substitution in three further scenarios. A user who collides with `Shipping/Warehouse` is now told only what they typed, which they already knew.

   This is a behavior regression in the MODIFIED block, and the proposal does not declare it. It is also avoidable: after the exclusive create reports the name exists and no byte-exact entry is listed, the parent can be listed and each entry compared by `(st_dev, st_ino)` against the resolved requested spelling, which identifies the stored spelling. The reviewer's own round-2 objection to device/inode comparison was that it cannot authorize a destructive rename (round 2, Critical 2); selecting a string for an error message carries no such risk, so that objection does not transfer. Either restore the guarantee or state in the delta why the existing name is unobtainable.

2. **The UI sequencing rule has no failure branch, so a failed refetch strands the view.** The revision requires that on API success the view "SHALL keep rendering its pre-rename template snapshot while it refreshes the template and group queries", and replace the selected path "Only after refreshed template data carries the new group paths". Nothing says what happens when that refresh fails. `useMoveTemplateGroup` today only invalidates and has no error branch touching the grid (`ui/src/api/queries.ts:166-175`), so the view would hold a stale pre-rename snapshot under a stale selection indefinitely, showing a group that no longer exists with no stated recovery. Specify the failed-refresh outcome.

3. **`proposal.md` understates the reason codes the delta now uses.** Impact says "No new `details.reason` is expected: the route reuses `template_group_invalid`, `template_group_case_conflict` and `template_group_unsafe_path`." The revised creation rule also specifies `template_registry_io` in four places. The code carries that reason already (`src/reason.rs:117`), so nothing new is introduced and the claim's substance holds, but the enumeration is now incomplete.

### Suggestions

- `409` for a folding filesystem that cannot recase is defensible but reads oddly: the destination is the same directory, not an occupant. A distinct `422` reason would tell a macOS user why recasing is unavailable rather than implying another group holds the name. Non-blocking; the current wording is honest about mechanism and testable.
- The post-mutation subtree audit walks the renamed subtree a second time on every rename. Bounded and correct, but worth a note if group trees ever get large.

**Checks that passed.** Critical 1 of round 2 is resolved correctly: the route now uses `422 template_group_unsafe_path`, matching the `template-registry` rule that a request-supplied unsafe path is `422` for write endpoints and `400` only for `DELETE /api/template-groups/{path}`, and the status table, prose and both scenarios agree. Critical 2 is resolved by removing the inode-identity branch entirely and always issuing the no-replace rename, with the resulting limitation stated plainly rather than papered over, and recasing still works on the Linux deploy target. Critical 3 adopts the `template-registry` snapshot boundary and specifies the post-mutation `500` without claiming a rollback the capability forbids. Moderate 1's creation rule now separates exact-entry reuse, alias, vanished occupant, file or symlink, and I/O failure, with a bounded single retry and no unbounded loop. `openspec validate --strict` passes, so no current-spec scenario was dropped. Codex wrote only the three permitted artifacts; no tracked file was modified.

## Embedded-Instruction / Injection Attempts

None detected. The artifacts contain scope and constraint statements addressed to the implementer, which is their normal function, and none attempts to direct the reviewer.

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. In the MODIFIED Labels-view requirement, replace the sentence "This change attaches the affordance to the flat full-path button toolbar that the Labels view currently implements; it SHALL NOT require building a new nested-tree component." with a statement of required behavior that is neutral about the control's shape, for example that the rename action SHALL be reachable from the group filter control for the currently selected real group. Do not describe what the view "currently implements" and do not state what the change is not required to build; both already live in `proposal.md` and `design.md:189`, which is where they belong.
2. Restore the case-conflict message guarantee that the current spec makes, so the refusal names the existing group rather than the requested path, in the requirement prose and in every scenario changed to name the requested path. If the stored spelling is genuinely unobtainable at that point, say so explicitly in the requirement instead, and leave the current spec's guarantee visibly superseded rather than silently weakened.
3. Specify what the Labels view does when the post-rename refresh fails: what it renders, what selection it holds, and how the user recovers.
4. Correct the `proposal.md` Impact enumeration to include `template_registry_io` among the reasons the change uses, keeping the point that no new reason is introduced.

CHANGES_APPLIED: yes

## Rebuttals

Re-check by the reviewer, limited to the four Required Changes.

1. **Applied.** The implementation-describing sentence is gone. The requirement now reads "The rename
   action SHALL be reachable from the group filter control for that selected group", which is neutral
   about the control's shape and compatible with the inherited tree contract in the same block. The
   contradiction is resolved and nothing asserts what the change is not required to build.
2. **Applied, by the preferred route.** The guarantee is restored in the requirement prose and in all
   five scenarios that had been changed to name the requested path. The `(st_dev, st_ino)` comparison
   is admitted only to select the string for the message and is fenced with "SHALL NOT authorize
   reuse, rename, or any other mutation", so it does not reintroduce the hazard of round 2's
   Critical 2. `design.md:128-132` carries the same fence.
3. **Applied.** The failed-refresh branch now states what is rendered (the captured pre-rename
   snapshot), what selection is held (the old path), and how the user recovers (retry both refreshes
   without repeating the rename, after which the ordinary rewrite completes). Non-blocking note for
   the implementer, not a new required change: this branch is stated in prose only and carries no
   scenario of its own, so the tasks should cover it explicitly.
4. **Applied.** `proposal.md` Impact now enumerates `template_registry_io` alongside the other three
   and keeps the point that no new reason is introduced.

No edit outside these four was made: diffs against the pre-change snapshots of all three artifacts
show only the passages above. `openspec validate issue-202-rename-group --strict` reports the change
valid. The verdict stands.
SPECS_SHA256: 899047be46eefe56bd0e20a74f7fb77aaf8edb892294bcb9dd8294a63b4fb89b
