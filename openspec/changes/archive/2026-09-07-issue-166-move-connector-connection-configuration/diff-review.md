# Diff review

AUTHORS: agy
REVIEWER: claude
VERDICT: APPROVE
ROUNDS: 3
TREE_SHA256: 813e8d093fe384fe1396710d424a227c1f5b05f177c7d9a0f030f2f82b4b444b
SPECS_SHA256: ccb596aa5e07161704082e000c489f587c2aa6ddc9e8d2a358636ebdaede431b

# Diff review: issue-166-move-connector-connection-configuration

TREE: working tree at `.worktrees/issue-166` (4 modified + 2 renamed files under `ui/src/pages/`, plus the untracked change folder)

## Scope

Diff reviewed: `ui/src/pages/Connect.tsx`, `ui/src/pages/Settings.tsx`, `ui/src/pages/Connect.test.tsx`, `ui/src/pages/Settings.test.tsx`, and `ui/src/pages/{settings→connect}/ConnectionsSection.{tsx,test.tsx}` (pure rename, 0 content lines — confirmed by `git diff --stat`). Against `proposal.md`, `design.md`, `tasks.md`, both spec deltas, the published `connections` / `default-connection` / `connector-browser` specs, frozen `docs/SPEC.md` §12, `AGENTS.md`, and the prior `review.md` / `diff-review-1.md` / `diff-review-2.md`.

No `ANSWERS.md` existed; nothing blocked me, so no `QUESTIONS.md` was written. I edited no file in the worktree (`git status --porcelain` unchanged); all mutation work ran in a throwaway copy at `/tmp/mut166b`.

## Gates I ran myself `[verified]`

`npm run lint` exit 0 · `npm run test` 50 files / 490 tests passed · `npm run build` exit 0 · `cargo fmt --check` exit 0 · `cargo clippy --all-targets --all-features` exit 0 · `cargo test` 879 passed, 0 failed · `openspec validate <change> --strict` reports valid. Every box in `tasks.md` §7 is earned.

## What holds up

**Delta bookkeeping.** Both `REMOVED` names exist verbatim in the published specs (`openspec/specs/connections/spec.md:273`, `openspec/specs/default-connection/spec.md:164`), so `archive-merge-check.sh` can resolve them. The `MODIFIED` resolves against an existing requirement. The `ADDED` requirements restate the removed ones in full: all four public-URL scenarios and the table/form contract carry over, and every scenario of "Settings names the default connection" reappears under "Connect names the default connection". The §12 supersession claim ("its first two sentences") matches `docs/SPEC.md:889-891` exactly. I swept every live reference: the only remaining "Settings > Connections" strings in `openspec/specs/` are inside the two requirements this delta removes, plus `connector-browser/spec.md:324`, and the stale `default-connection/spec.md:109` ("the default is set from Settings") sits inside the `MODIFIED` requirement and is rewritten.

**Implementation.** The disclosure (`Connect.tsx:108-123`) reproduces the existing idiom at `ui/src/pages/settings/PrintersSection.tsx:190-199` line for line, and sits below both pickers and above the composer and browse table as the spec requires. The clearing predicate (`Connect.tsx:63-69`) writes `""` rather than `null`, so it cannot fall back through the latch (`Connect.tsx:60-61`), and terminates on the next pass. The move is clean: nothing anywhere still imports `settings/ConnectionsSection`, and no nav or shell code references it.

**Mutation-verified discrimination `[verified]`**, run in `/tmp/mut166b`:

| Mutation | Result |
|---|---|
| `setOpen(connections.length === 0)` → `setOpen(true)` | 8 tests fail |
| → `setOpen(false)` | 1 test fails (the disclosure test) |
| add `else if (open === null && connectionsFailed) setOpen(true)` | disclosure test fails at `Connect.test.tsx:471` |
| delete the whole clearing block `Connect.tsx:63-69` | 6.5 and 6.6 fail |

That third row is the point: `diff-review-2`'s blocking finding was that a block flying open on a failed connections list shipped green. It no longer does. The fix at `Connect.test.tsx:470` (`await waitFor(() => expect(qcFailed.getQueryState(["connections"])?.status).toBe("error"))`) is the shape that review prescribed and it discriminates.

Both plan-review rounds' four required changes landed: `setSelected([])` plus the cleared-rows scenario, the enabled/disabled qualification with a disabled-creation scenario, the Settings fetch-spy assertion, and all thirteen picker queries narrowed to `/^connection$/i` (no `/connection/i` lookups remain).

## Findings

None blocking. Three carry over from earlier rounds, all confirmed still true and all accuracy-of-claim rather than behavior:

**1. Non-blocking. `tasks.md:63` (6.7) names a property no test isolates.** Deleting `Connect.tsx:63-69` entirely leaves the "rows selected against a connection do not come back" test green `[verified]` — only 6.5 and 6.6 go red. After a delete with no clearing rule, `connectionId` stays `"c1"` while the `<select>` has no matching option, so `picker.value` reads `""` from the DOM anyway; and the test reaches `c2` through `fireEvent.change(picker, ...)`, whose own `onChange` already clears `selected` (`Connect.tsx:83`). The scenario's picker-reset half is genuinely covered by 6.5/6.6.

**2. Non-blocking. The `!connectionsFailed` guard at `Connect.tsx:63` is dead, so `tasks.md:66` (6.8) rests on nothing.** `connectionsFailed && connections !== undefined` only arises on a *refetch* failure, where `connections` is the last successful list and therefore still offers the selection; a first-load failure is already stopped by `connections !== undefined`. The guard states intent and is harmless.

**3. Non-blocking, no action needed.** `openspec/specs/connector-browser/spec.md:324-325` still says §12's "Settings > Connections form … remains authoritative", which the new `ADDED` requirement now contradicts more widely than the old one did. `design.md:95-99` considered this and its reasoning holds: that clause scopes what the browse-table requirement supersedes, it already coexisted with `connections` superseding the same paragraph's field list, and `AGENTS.md`'s precedence rule lands a reader on the new requirement.

