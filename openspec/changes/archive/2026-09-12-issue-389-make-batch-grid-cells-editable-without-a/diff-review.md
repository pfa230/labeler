# Diff review

AUTHORS: agy
REVIEWER: opencode
VERDICT: APPROVE
ROUNDS: 1
TREE_SHA256: d225b23f2ba5b845c9ddcd41fa661aac05b5ba2eee321892163e5053703e85c9
SPECS_SHA256: 59161d1f02369a5f2c2fa6bb1d9b31929c636dcb8656ec70da95ef39f13b390a

Reviewing the implementation diff against its contract — checking for real defects before approval.Deep-diving into the grid implementation — verifying focus handling, keyboard coverage and accessibility details.Implementation matches the contracted always-on grid. Proposal, design and delta require rendering live controls via `cell` instead of the overlay editor, removing `registerInlineEditor` and the `editor` predicate, preserving per-row `cellInput`/`disabled` editability, Tab-only navigation, bordered surface styling, and explicit handling for focus retention, multiline, unrepresentable values and `aria-readonly`.

Diff evidence: `LabelGrid.tsx:1-24` removes `registerInlineEditor` imports, `LabelGrid.tsx:51-57` adds bordered surface classes, `LabelGrid.tsx:121-136` implements Arrow/Home/End `stopPropagation` and `mousedown`/`click`/`onFocus` suppression, `LabelGrid.tsx:609-621` intercepts `hotkey` and returns `false` for `INPUT`/`TEXTAREA`/`SELECT`, `LabelGrid.tsx:642-653` implements initial focus on first editable without preview column, `LabelGrid.tsx:138-523` replaces seven `*Editor` components with per-control branches for `text`/`textarea`/`select`/`checkbox`/`integer`/`number`/`date`/`datetime` with `list`/`image`/`disabled`/missing read-only paths, multiline `first +N` with `title` and unrepresentable empty control plus adornment with explicit-change boundary, and `LabelGrid.tsx:150-156` synchronizes `aria-readonly` via `closest('[role="gridcell"]')` [verified]. Gates: `cargo fmt --check`, `cargo clippy`, `ui npm run lint`, `ui npm test` 579 passed, `ui npm run build` succeed [verified].

Findings:
- No functional deviation from the spec delta scenarios: at-rest controls for every type, `list` and `disabled` as plain text, invalid cells keep operable control alongside `⚠` with `aria-describedby`, multiline `text` truncation to first line with `+N`, offset date `2026-09-01T12:00:00Z` empty control plus text adornment, non-numeric `abc` and malformed `maybe` as empty/unset plus adornment, and `aria-readonly` transitions without remount are covered and pass in `LabelGrid.test.tsx:107-791` [verified].
- `LabelGrid.tsx:612-621` suppresses `hotkey` for any `INPUT`/`TEXTAREA`/`SELECT` target rather than limiting to an always-on cell, broader than `spec.md:271-276` but harmless because the grid's data columns contain the only such controls, the preview radio is excluded from the initial-focus selector at `LabelGrid.tsx:645-647` [verified]. Non-blocking.
- `LabelGrid.tsx:643` initial-focus effect guards on `!onSelectRow` and `!disabled` as required by `spec.md:788-793`, but does not re-attempt if the first mount finds no editable due to `list`/`image`-only columns until a later `disabled` transition. No spec scenario requires it. Non-blocking.
- `LabelGrid.tsx:208-214` read-only `image` path surfaces error via `cellErrorStyle`/`title` without an adjacent `⚠` marker, while other controls render `⚠` alongside the control per `spec.md:335-340` and `spec.md:715-721`. The spec does not require a marker for `image` (never submittable-blocking in the same way), so the gap matches current behavior [assumption: verify whether image required errors should show `⚠`]. Non-blocking.
- No explicit DOM Tab walk assertion exists in `LabelGrid.test.tsx:377-513`; coverage proves `stopPropagation` for arrows and initial/click focus retention, Tab behavior relies on native DOM order plus the `hotkey` suppression above. The spec scenario `spec.md:766-771` is implemented by relying on the browser, but an explicit Tab/ShiftTab `document.activeElement` test as described in `review-3.md` is not present [verified]. Low severity, does not contradict rendered behavior.

No blocking violation of the proposal, specs, design, tasks or `AGENTS.md` gates was found.

