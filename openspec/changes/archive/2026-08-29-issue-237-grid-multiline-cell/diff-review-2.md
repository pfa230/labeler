# Diff review

AUTHOR: agy
REVIEWER: claude-subagent
VERDICT: REVISE
ROUNDS: 2

## What I checked

Scoped to the round-2 delta only. `ui/src/components/LabelGrid.tsx`, `Import.tsx`, `Connect.tsx`,
`Import.test.tsx`, `docs/adr/README.md` and the ADR are byte-identical to what I cleared in round 1
(`LabelGrid.tsx` still has the editor branch at :38, `stopPropagation` at :49, `preventDefault` at :51,
the CRLF split at :161), so I did not re-review them. Only `LabelGrid.test.tsx` (149 → 178 changed
lines) and `tasks.md` moved. `diff-review-1.md` is untouched.

**Both mutations re-run, in a fresh `/tmp` copy of `ui/` with `node_modules` symlinked. The worktree
was never mutated.**

| mutation | round 1 | round 2 |
| --- | --- | --- |
| `control === "textarea"` → always true (every cell a `<textarea>`) | 406 passed, survived | **2 failed / 405 passed** — killed |
| `control === "textarea"` → never true (control ignored entirely) | n/a | **4 failed / 403 passed** — killed |
| delete `e.preventDefault()` (LabelGrid.tsx:51) | 406 passed, survived | **407 passed — still survives** (see Moderate) |

**Round-1 M2 resolved.** The always-textarea mutation is now caught by two tests:
`LabelGrid.test.tsx:84` (a `tagName === "INPUT"` assertion added to the pre-existing edit test) and the
new `:99` "renders an input element for a text-control cell and commits on blur", which drives the
component through an explicit `cellInput` stub reporting `control: "text"`.

**Round-1 M1 resolved as asked.** The plain-Enter test (`:196`) and the Escape test (`:227`) now assert
`textarea.tagName === "TEXTAREA"` (`:204`, `:235`). I proved they are no longer self-satisfying by
running the *inverse* mutation, which makes the component ignore the reported control: three
`LabelGrid` tests plus the `Import` page test go red. In round 1 those two tests passed against the
pre-change `<input>`-only source; they no longer can.

**Round-1 M3 resolved.** `tasks.md` §6 now carries only 6.1 and 6.2 as boxes; the by-hand
browser-and-rendered-label step is a plain paragraph with no checkbox, citing AGENTS.md ("Templates are
visual artifacts") and stating that its only evidence is a transient session. I read all 17 remaining
`- [x]` lines: every one is machine-checkable (a test, a source edit, a file, or a gate). No box claims
anything unverifiable.

**Nothing was weakened to make something pass.** Comparing the test file against its round-1 state:
five assertions were added (`:84`, `:107`, `:175`, `:204`, `:235`), one whole test was added (`:99`),
one CRLF row was added to the multiline test (`:270`, row `c` = `"crlf one\r\ncrlf two"`), and
`getByText("+1")` became `getAllByText("+1")).toHaveLength(2)`, which is *stronger*: it now asserts the
single-line row gets no marker. Exactly one assertion was deleted, the literal-`\n` text query I called
vacuous in round 1. I verified that judgement empirically rather than restating it: rendering
`<span>{"line one\nline two"}</span>` and querying `queryByText("line one\nline two")` returns null even
there, because testing-library normalizes the element's text but not the matcher string. It could not
fail against any implementation, so its removal lost no coverage. The stray trailing blank line
(round-1 S4) is also gone.

**CRLF (round-1 S1) is now covered**: the multiline test's third row is a CRLF value and the test
asserts `getByText("crlf one")` with no stray `\r`, plus exactly two `+1` markers across three rows.

**Gates, run by me in the worktree, not taken on trust:**

- `ui/`: `npm run lint` clean (no output). `npm run test` **407 passed / 407, 49 files**.
  `npm run build` green (built in 1.13s).
- root: `cargo fmt --check` clean. `cargo clippy --all-targets --all-features` no warnings.
  `cargo test` **671 passed, 0 failed**.
- `git status --porcelain` still shows only the six modified files, the ADR and the change folder; my
  scratch work and `npm run build` output (`ui/dist`, gitignored) left the worktree unchanged.

## Critical (blocking)

None.

## Moderate

**M4. `preventDefault` is assertable under jsdom, and the code now carries a comment saying it is not.**
Deleting `LabelGrid.tsx:51` still leaves all 407 tests green, so the spec scenario "Enter commits a grid
cell rather than breaking the line" has no assertion for the *no newline* half. In round 1 I told agy
this was not assertable and to say so plainly instead; agy did exactly that
(`LabelGrid.test.tsx:207-209`). **I was wrong, and I verified it this round rather than repeating the
claim.** `fireEvent` returns `dispatchEvent()`'s result, which is `false` when a listener called
`preventDefault()`, and React's synthetic `preventDefault` calls through to the native event. I wrote a
throwaway test in the scratch copy: `expect(fireEvent.keyDown(textarea, { key: "Enter" })).toBe(false)`
passes against the current source and **fails** (`expected true to be false`) with `preventDefault`
removed. So it is a discriminating one-line lock on the half of decision (b) that is currently
unguarded.

Fix: change `LabelGrid.test.tsx:210` from `fireEvent.keyDown(textarea, { key: "Enter", shiftKey: false });`
to capture and assert the return value, and delete the now-false comment at `:207-209` (a comment
asserting a limitation that does not exist is worse than none). A matching assertion that Shift+Enter
does *not* cancel (`toBe(true)`) would pin both halves of the ADR's decision 2 in one place.

This is the only open finding and it is one line of test plus three lines of comment removal. The
implementation itself is correct; nothing here says the shipped behavior is wrong.

## Suggestions

Round-1 S2, S5 and S6 were not addressed and remain non-blocking, unchanged in force: `Connect.tsx`'s
`cellInput` still has no test of its own (`Import` has the page-level one), a trailing newline still
renders as `abc +1`, and the Enter handler still has no `e.nativeEvent.isComposing` guard (pre-existing
for the `<input>` path, so not a regression from this diff). None of them needs to hold up this change.
