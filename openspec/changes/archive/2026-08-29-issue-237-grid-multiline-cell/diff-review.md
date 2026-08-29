# Diff review

AUTHOR: agy
REVIEWER: claude-subagent
VERDICT: APPROVE
ROUNDS: 3

## What I checked

Scoped to the round-3 delta, which is confined to `ui/src/components/LabelGrid.test.tsx`. The
implementation is untouched: `ui/src/components/LabelGrid.tsx` still hashes
`fc0e75ab1a70989b5af3890a170115e57fdc49a8`, identical to the round-2 snapshot, as do `Import.tsx`,
`Connect.tsx` and `Import.test.tsx`. `diff-review-1.md` and `diff-review-2.md` are untouched.

**The exact delta.** I kept the round-2 test file in my scratch copy, so I diffed it against the
worktree rather than trusting the summary. Three hunks, nothing else, 296 → 293 lines:

- `:177` `fireEvent.keyDown(textarea, { key: "Enter", shiftKey: true })` → wrapped in
  `expect(...).toBe(true)`
- `:207` `fireEvent.keyDown(textarea, { key: "Enter", shiftKey: false })` → wrapped in
  `expect(...).toBe(false)`
- the three-line jsdom comment deleted

No assertion was removed, loosened or reordered. `grep` confirms no "jsdom" or "native default"
comment survives anywhere in the file, and the five comments that remain (`:117`-`:142`) are the
pre-existing inert-cell notes, all still accurate.

**Round-2 M4 resolved, proven by mutation.** All mutations run in a fresh `/tmp` copy of `ui/` with
`node_modules` symlinked; the worktree was never mutated. Baseline on that copy: 407 passed.

| mutation | round 2 | round 3 |
| --- | --- | --- |
| delete `e.preventDefault()` (LabelGrid.tsx:51) | 407 passed, **survived** | **1 failed / 406** — `expected true to be false` at `:207` |
| delete `e.stopPropagation()` (LabelGrid.tsx:49) | killed | **2 failed / 405** — still killed |
| invert the branch (Shift→`preventDefault`, plain→`stopPropagation`) | n/a | **3 failed / 404** — `expected false to be true` at `:177` and `expected true to be false` at `:207` |

So the assertion added at `:207` is the one that kills the preventDefault mutation, which was green
through rounds 1 and 2. That was the sole open finding and it is closed.

**The Shift+Enter `toBe(true)` assertion discriminates, and I checked what it does and does not
catch.** It fires under the inverted-branch mutation (`expected false to be true`), which is the
mutation it exists to catch: it pins that Shift+Enter must *not* cancel the event, the half the
`:207` assertion cannot express. It does **not** fire when `stopPropagation` is merely deleted, and
that is correct rather than a gap: react-data-grid's `EditCell.handleKeyDown`
(`node_modules/react-data-grid/lib/index.js:1315-1316`) commits on Enter without calling
`preventDefault`, so `defaultPrevented` stays false either way. That mutation is caught by the same
test's value assertion instead (it fails with "expected vi.fn() to be called at least once"). Between
the two assertions and the value check, every single-call mutation I could construct on the key
handler is now caught.

**Earlier rounds, all resolved and re-confirmed unchanged this round:** M1 (plain-Enter and Escape
tests could not fail) and M2 (nothing distinguished the textarea branch from the input branch) were
fixed in round 2 by five `tagName` assertions and a new text-control test, verified there with
both directions of the control mutation. M3 (`tasks.md` 6.3 claiming an unverifiable by-hand step) was
fixed by replacing the box with a prose note citing AGENTS.md; the file still carries 16 `- [x]` lines
and every one is machine-checkable. Round-1 S1 (CRLF) and S4 (trailing blank line) were also fixed in
round 2.

**Gates, run by me in the worktree:**

- `ui/`: `npm run lint` clean (no output). `npm run test` **407 passed / 407, 49 files**.
  `npm run build` green (built in 1.05s).
- root: `cargo fmt --check` clean. `cargo clippy --all-targets --all-features` **0 warnings, 0 errors**.
  `cargo test` **671 passed, 0 failed**.
- `git status --porcelain` unchanged: the same six modified files, the ADR and the change folder. My
  scratch work and `npm run build` output (`ui/dist`, gitignored) left the worktree clean.

## Critical (blocking)

None.

## Moderate

None.

## Suggestions

Round-1 S2, S5 and S6 remain open and non-blocking, unchanged in force and not worth another round:
`Connect.tsx`'s `cellInput` has no test of its own (the `Import` page-level test covers the identical
shape), a trailing newline renders as `abc +1`, and the Enter handler has no
`e.nativeEvent.isComposing` guard, which is pre-existing for the `<input>` path and so not a
regression from this diff.
