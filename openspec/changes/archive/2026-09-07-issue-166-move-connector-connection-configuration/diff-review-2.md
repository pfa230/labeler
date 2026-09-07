TREE_SHA256: 50bd4485a2ec3fa550062d7498c9c35233f51ab9dfbcdf453cff45e8fec51453
SPECS_SHA256: ccb596aa5e07161704082e000c489f587c2aa6ddc9e8d2a358636ebdaede431b

# Diff review: issue-166-move-connector-connection-configuration

## Scope

Diff: `ui/src/pages/Connect.tsx`, `ui/src/pages/Settings.tsx`, `ui/src/pages/{settings→connect}/ConnectionsSection.{tsx,test.tsx}` (pure rename, 0 content lines), `ui/src/pages/Connect.test.tsx`, `ui/src/pages/Settings.test.tsx`, plus the change folder. Against `proposal.md`, `design.md`, `tasks.md`, both spec deltas, the published `connections` / `default-connection` / `connector-browser` specs, frozen `docs/SPEC.md` §12, `AGENTS.md`, and `review.md` / `diff-review-1.md`.

Gates I ran in this worktree `[verified]`: `npm run lint` exit 0, `npm run test` 50 files / 490 tests passed, `npm run build` exit 0, `cargo fmt --check` and `cargo clippy --all-targets --all-features` exit 0, `openspec validate <change> --strict` reports valid. Every finding below is backed by a mutation run in a throwaway copy at `/tmp/mut166`; nothing in the worktree was edited (`git status` unchanged).

## What holds up

Delta bookkeeping is correct: both `REMOVED` names exist verbatim (`openspec/specs/connections/spec.md:273`, `openspec/specs/default-connection/spec.md:164`), the §12 supersession claim matches `docs/SPEC.md:889-891`, and `MODIFIED` resolves against an existing requirement. The four plan-review required changes all landed. The disclosure at `Connect.tsx:108-123` reproduces the existing idiom (`ui/src/pages/settings/PrintersSection.tsx:190-199`) exactly and sits below both pickers, above composer and browse table. The clearing predicate at `Connect.tsx:63-69` writes `""` rather than `null` so it cannot fall back through the latch, and terminates.

Three tests are discriminating, confirmed by mutation:
- `Settings.test.tsx:65-77` fails when `ConnectionsSection` is re-added to `Settings.tsx`.
- `Connect.test.tsx:436` case 1 fails when the latch is changed to `setOpen(true)`; case 2 fails when changed to `setOpen(false)`.
- `Connect.test.tsx:590,604` (delete and disable) fail when `Connect.tsx:63-69` is deleted entirely.

## Findings

**1. Blocking. The failure-case disclosure test still asserts nothing, and the spec scenario it stands for has no test that can fail** (`ui/src/pages/Connect.test.tsx:465-478`, scenario at `specs/connections/spec.md:60-63`, claimed by `tasks.md:51`).

`fireEvent.click(btnFailed)` at `:471` runs on the render after `findByRole` resolves, before the connections query has settled. `open` is still `null` at that point, so the click itself resolves the latch to `true` and the guard at `Connect.tsx:56` never fires again. Every later assertion (`:472`, `:473`, `:476`, `:477`) then describes operator-driven toggling, not the auto-resolution under test.

Evidence: I added an explicit violation to `Connect.tsx`,

```
  } else if (open === null && connectionsFailed) {
    setOpen(true);
  }
```

and the whole page suite passed, 287/287, including this test `[verified]`. A block that flies open on a failed connections list ships green.

This is the same defect `diff-review-1.md` finding 1 marked blocking. The fix applied took the first of that review's two suggested shapes, which is the shape its own next sentence said "does not rescue it".

A fix that works, verified both directions in `/tmp/mut166` (passes on the current implementation, fails on the violation above):

```js
const { queryClient: qcFailed } = renderConnect();
const btnFailed = await screen.findByRole("button", { name: /manage connections/i });
await waitFor(() => expect(qcFailed.getQueryState(["connections"])?.status).toBe("error"));
expect(btnFailed).toHaveAttribute("aria-expanded", "false");
```

**2. Non-blocking. `tasks.md:66` (6.8) checks off a test that cannot fail, because the guard it targets is dead** (`ui/src/pages/Connect.tsx:63`, test at `Connect.test.tsx:667`).

Deleting `!connectionsFailed` from the clearing predicate leaves all 28 Connect tests green `[verified]`. The reason is React Query semantics: `connectionsFailed && connections !== undefined` only arises on a *refetch* failure, where `connections` is the last successful list and therefore still offers the selection; a first-load failure is already stopped by `connections !== undefined`. So the guard never changes an outcome, and the spec sentence "A connections list that ... failed to load SHALL clear nothing" (`specs/default-connection/spec.md`) rests on nothing the suite can show. The guard is harmless and states intent; the checked box is what overstates.

**3. Non-blocking. `tasks.md:63` (6.7) names two properties its test cannot show** (`Connect.test.tsx:618-665`).

Deleting `Connect.tsx:63-69` outright leaves this test passing `[verified]`; only 6.5 and 6.6 fail. After a delete with no clearing rule, `connectionId` stays `"c1"` while the `<select>` has no matching option, so `picker.value` reads `""` from the DOM and `:645` passes anyway. Deleting just `setSelected([])` at `Connect.tsx:67` also leaves it passing `[verified]`, because the test reaches `c2` through `fireEvent.change(picker, ...)` at `:647`, whose own `onChange` clears `selected` (`Connect.tsx:83`). `design.md` already labels that write defensive; what is left unstated is that no test distinguishes it. The scenario's picker-reset half is genuinely covered by 6.5/6.6, so this is a claim-accuracy problem, not a behavior gap.

VERDICT: REVISE
