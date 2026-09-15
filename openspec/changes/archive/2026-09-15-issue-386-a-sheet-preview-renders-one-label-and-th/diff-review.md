# Diff review

AUTHORS: agy
REVIEWER: claude
VERDICT: APPROVE
ROUNDS: 2
TREE_SHA256: 11d5efd4430a88011faccc51b3271689237f2687b1b4f843d599ead51fd60360
SPECS_SHA256: 6288e41af3ce3cfe1f3995393609e1b3f427d1c5d988201eab16001266746cc8

# Diff review 2: issue-386 sheet preview

Gates run in this worktree [verified]: `npm run lint && npm run test && npm run build` green (52 files, 604 tests, log `.agent-runs/review2-ui-gates.log`); `cargo fmt --check`, `cargo clippy --all-targets --all-features`, `cargo test` green (911 passed, log `.agent-runs/review2-rust-gates.log`). `openspec validate` passes. The specs digest recomputed by the `specs-digest.sh` recipe is `6288e41a…`, equal to the one `review.md` recorded, so the delta reviewed at plan time is the delta implemented. The diff touches `ui/` only; no Rust, no API, no template change, as the proposal states.

## Prior review's blocking findings

**1 (focus steal on template switch): fixed.** `LabelGrid.tsx:644-645` now consumes `initialFocusDone` on the first render that has rows, whether or not the radio column is present, and only then decides whether to focus. A later `onSelectRow` flip can no longer trigger it. `Import.test.tsx:1278` covers the exact case the reviewer ran: grid loaded under a single template, picker focused, changed to a sheet template, focus stays on the picker. [assumption] That this test fails against the old `LabelGrid.tsx`: the old condition `!initialFocusDone.current && !onSelectRow && !disabled && rows.length > 0` with deps `[onSelectRow, disabled, rows]` fires when `onSelectRow` becomes `undefined`, which is what the prior reviewer observed in a scratch run; verify by reverting that one file and running the test.

The residual behavior, focus landing in the first cell when a sheet grid first shows rows, is the documented one, not a new one: both pages mount `LabelGrid` only inside `rows.length > 0` (`Connect.tsx:375`, `Import.tsx:400`), so the first render with rows is the "freshly mounted" grid the `template-inputs` scenario prescribes (`openspec/specs/template-inputs/spec.md:1682-1686`), and the proposal names that fixture becoming the sheet grid's ordinary state (`proposal.md:44-46`).

**2 (zero-request assertions that could not fail): fixed.** Each "no batch request" assertion in 5.3, 5.5 and 5.6 now sits after a 400 ms wait that outlasts the 300 ms debounce (`Connect.test.tsx:2924,2986,3040`; `Import.test.tsx:1326,1388,1440`), so an implementation that ignored `blocked` or `rowsPending` would have fired by then and the count would be nonzero.

The prior non-blocking 3 and 4 are also addressed: 5.8 step 4 now changes the printer to a real option on both pages (`Connect.test.tsx:3138` stubs `p1`; `Import.test.tsx:1539`), and 5.1 waits for a count of exactly one.

## Contract check

Traced and holding: one builder for both paths (`labelGrid.ts:37-48`, called with the same `dataFor` from `run()` and from the preview at `Connect.tsx:262,313` and `Import.tsx:196,290`); the preview body's `mode`/`labels`/`start_slot` matches `run()`'s download body less `printer`; `sheetPreviewBlock` carries the three spec strings with invalid rows before the cap and positions taken from the unfiltered grid (`Connect.tsx:258`, `Import.tsx:192`); `rowsPending` precedes `blocked` in both pages; `PreviewPane` precedence is loading, blocked, error, url, idle (`PreviewPane.tsx:23-40`); `useRowPreview` is single-only and idle for a sheet; radios gated at the call sites. The hook aborts on key change and on `enabled` going false, drops aborted responses without reporting an error, holds one object URL and revokes it on replacement and unmount. Tests for each of these are ones that would fail on the wrong code, as far as I traced them.

## Non-blocking

**3. An inputs-endpoint failure now leaves the sheet pane on "rendering preview…" forever, with no request and no message.** `labelInputs.ts:205,235` sets `error` and keeps `pending` true because the cache never fills; both pages discard `error` (`Connect.tsx:195`, `Import.tsx:106`); the page then composes `{ loading: true }` from `rowsPending` indefinitely. Before this diff the one-label sheet preview still rendered from the fallback inputs. It matches `run()`'s own permanent "Resolving row inputs; please wait." refusal, so it is a pre-existing shape rather than this change's defect, and the delta specifies only the pending state, not the failed one. The prior review asked for an issue; none is filed yet (`gh issue list` shows nothing for it). File it before or with the commit.

**4. No test for a refused sheet render.** The spec scenario "A refused render never gates the run" says "either format"; the code path exists (`sheetPreview.ts:52-55` → `PreviewPane.tsx:27`, and Download's `disabled` never reads preview state) but only 5.7 (single) exercises it. No task asked for it, so it is a gap to note, not an unearned box.

**5. Re-enabling the hook on an unchanged key shows the previous PDF for the debounce window.** `sheetPreview.ts:85` returns `st.url` with `loading: false` when `st.key === key`, while the `enabled` dependency has re-run the effect and a request is pending. Sequence: clear a required cell (blocked, effect torn down), retype the same value (same key, enabled): pane shows the old PDF for ~300 ms, then "rendering", then the same PDF again. The content shown is correct for the key throughout; the same property exists in `useLivePreview`. Cosmetic.

