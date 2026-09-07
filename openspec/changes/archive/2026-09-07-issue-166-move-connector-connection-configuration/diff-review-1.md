TREE_SHA256: 29cff319abcf5ddfc1b7eec5c05adb221d4ce08c326788d5e4d8882beefd2bfb
SPECS_SHA256: ccb596aa5e07161704082e000c489f587c2aa6ddc9e8d2a358636ebdaede431b

## Scope reviewed

Diff: `ui/src/pages/Connect.tsx`, `ui/src/pages/Settings.tsx`, `ui/src/pages/{settings→connect}/ConnectionsSection.{tsx,test.tsx}` (pure rename, 0 content lines), `ui/src/pages/Connect.test.tsx`, `ui/src/pages/Settings.test.tsx`, plus the change folder. Against: `proposal.md`, `design.md`, `tasks.md`, both spec deltas, the published `connections` / `default-connection` / `connector-browser` specs, frozen `docs/SPEC.md` §12, and `AGENTS.md`.

Gates I ran myself in the worktree: `npm run lint` exit 0, `npx vitest run` 50 files / 490 tests passed, `npm run build` exit 0, `openspec validate <change> --strict` reports valid. `[verified]` The diff touches no Rust, so tasks 7.4-7.6 are not diff-touching gates; I did not re-run them.

## What holds up

The delta bookkeeping is correct. Both `REMOVED` requirement names exist verbatim in the published specs (`openspec/specs/connections/spec.md:273`, `openspec/specs/default-connection/spec.md:164`), all four scenarios of the removed `Connections settings UI` are restated in the `ADDED` requirement, and the `MODIFIED` `Connect opens on a resolved connection` preserves every prior scenario while adding the new ones. The `docs/SPEC.md` §12 supersession claim ("its first two sentences") matches `docs/SPEC.md:889-891` exactly. The stale reference at `openspec/specs/default-connection/spec.md:109` ("the default is set from Settings") is inside the `MODIFIED` requirement and is rewritten. `docs/adr/0063` and `0069` still say "Settings > Connections"; leaving them is right, the ADR set is frozen.

The implementation is minimal and matches the design. The clearing predicate at `ui/src/pages/Connect.tsx:63-69` is guarded on the list having loaded, writes `""` rather than `null` so it does not fall back through the latch (`Connect.tsx:60-61`), and cannot loop because `connectionId` is `""` on the next pass. The disclosure at `Connect.tsx:108-123` reuses the existing idiom verbatim (`ui/src/pages/settings/PrintersSection.tsx:190-199`) and is placed below both pickers and above the composer, as the spec requires. No React "cannot update a component while rendering" warnings appear anywhere in the test output. The move introduces no privilege change: neither `/connect` nor `/settings` is role-gated (`ui/src/app/App.tsx:34-35`).

`Settings.test.tsx:65-78` is a real test: if the section were still mounted its query would fire before `/api/variables` resolves, so `fetchSpy.mock.calls` would carry the call and line 76 would fail.

## Findings

**1. Blocking. The disclosure test's two "collapsed" assertions are not sequenced after any settle point, so they do not exercise the states they name** (`ui/src/pages/Connect.test.tsx:446` and `:468`).

Both run synchronously on the line after `await screen.findByRole("button", ...)`, which resolves on the first render, while the connections query is still pending. In that pre-load state `open` is `null` and `isOpen = open === true` is false (`Connect.tsx:56-58,75`), so `aria-expanded` reads `"false"` whether or not the list has loaded. The assertions therefore hold identically against an implementation that expands on a failed or non-empty list.

The evidence that the query has not settled at that point is in the same file: case 2 needed `await waitFor(...)` for the mirror-image assertion at `:459`, and every pre-existing latch test in the suite (`:261`, `:279`, `:296`, `:312`, `:329`, `:346`, `:363`, `:380`, `:412`) wraps its post-`findBy` assertion in `waitFor` for exactly this reason. The subsequent click does not rescue it: with `open` still `null`, the click resolves the latch to `true` itself, after which the guard `open === null` never fires again, so a buggy expand-on-failure implementation reaches the same final state and passes `:471` and `:472`.

This matters because `tasks.md` 6.1 checks off "collapsed when the request for the list fails", and that scenario (`specs/connections/spec.md:60-63`) then has nothing that can show it. Fix: assert collapsed only after an observable settle point, for instance open the block, `await screen.findByText(/failed to load connections/i)`, close it, and then assert `aria-expanded` is `"false"`; or wait on the picker reaching its resolved value first, as the neighbouring tests do.

**2. Non-blocking. `design.md:70-79` justifies the clearing path's `setSelected([])` with a claim the current code contradicts, and the test named as its proof cannot distinguish it.**

The design says missing the second write "would leave rows selected against a deleted connection to reappear the moment another is picked". The picker's own `onChange` already calls `setSelected([])` (`Connect.tsx:83`), and after clearing, `connectionId` is `""`, so neither `ConnectorBrowser` nor `Composer` mounts (`Connect.tsx:125,138`) and the stale array is unobservable until the next pick clears it anyway. The test at `Connect.test.tsx:612-659` reaches the new connection only through that same `fireEvent.change(picker, ...)` at `:642`, so it passes with `Connect.tsx:67` deleted. The line is harmless and keeps the invariant local, and the test does verify the spec scenario end to end; only the design's stated reason and the "this test proves that line" claim are wrong.

**3. Non-blocking. `Settings.test.tsx:75` asserts the whole page renders no `table` at all.** That is an assertion about `UsersSection`, `TokensSection`, `PrintersSection` and `DatetimeFormatsSection` as much as about connections, and it will break the day any of them grows a table for reasons unrelated to this change. The three targeted absence checks at `:72-74` plus the fetch-spy check at `:76` already carry the requirement.

**4. Note, no action needed.** `openspec/specs/connector-browser/spec.md:324-325` still says §12's "Settings > Connections form ... is unchanged and remains authoritative". `design.md:117-123` considered this and its reasoning checks out: that clause scopes what the browse-table requirement supersedes, and it already coexisted with `connections` superseding the same paragraph's field list. A reader following the precedence rule lands on the new `ADDED` requirement.

VERDICT: REVISE
