# Baseline Red Run Record: issue-271

Baseline run of `ui/src/components/LabelGrid.test.tsx` (group 1 tests 1.1–1.7) against `origin/main`'s `LabelGrid.tsx` prior to implementation changes.

## Environment & Summary
- **Target file under test**: `ui/src/components/LabelGrid.tsx` (`origin/main`)
- **Pre-existing test suite**: 21 passed
- **Group 1 red-first tests (1.1–1.7)**: 12 failed out of 12 tests
- **Total outcome**: 12 failed, 21 passed (33 total)

## Per-Test Failure Modes (Tests 1.1–1.7)

### 1.1 A test per control asserting opened editor (6 tests)
- `select`: Fails expecting `tagName === "SELECT"`. Receives `<input>` from `TEXT_EDITOR` fallback.
- `checkbox`: Fails expecting `<input type="checkbox">`. Receives text `<input>` from `TEXT_EDITOR` fallback without `type="checkbox"`.
- `integer`: Fails expecting `<input type="number">`. Receives text `<input>` from `TEXT_EDITOR` fallback.
- `number`: Fails expecting `<input type="number">`. Receives text `<input>` from `TEXT_EDITOR` fallback.
- `date`: Fails expecting `<input type="date">`. Receives text `<input>` from `TEXT_EDITOR` fallback.
- `datetime`: Fails expecting `<input type="datetime-local">`. Receives text `<input>` from `TEXT_EDITOR` fallback.

### 1.2 `select` editor & resting cell (1 test)
- Fails on resting cell: expects `(none)` for unset cell `r1`, but receives empty string.
- Fails on open editor: receives `TEXT_EDITOR` (`<input>`) instead of `<select>` with declared choices and retained out-of-set value.

### 1.3 `checkbox` activation cycle & resting box (1 test)
- Fails on resting cell: expects `role="checkbox"` with state name; receives raw text `"true"`.
- Fails on open editor: receives `TEXT_EDITOR` rather than checkbox control.

### 1.4 `integer` and `number` bounds & stepping (1 test)
- Fails on `expect(countEditor).toHaveAttribute("type", "number")`: editor is plain text `<input>` without `min`, `max`, `step`, or `aria-invalid`.

### 1.5 `date` and `datetime` controls (1 test)
- Fails on `expect(dEditor).toHaveAttribute("type", "date")`: editor is plain text `<input>`.

### 1.6 `image` non-editable & resting marker (1 test)
- Fails on resting cell: receives full data URI string instead of `"image"` marker.
- Fails on open editor: double-clicking cell opens `TEXT_EDITOR`, failing `expect(screen.queryByLabelText("edit logo")).toBeNull()`.

### 1.7 Unset `checkbox` and `select` resting cells & submission (1 test)
- Fails on resting checkbox: expects `role="checkbox"` with `indeterminate === true`; receives empty string.
- Fails on resting select: expects `(none)`; receives empty string.

---

## Status of Test 1.9 (Unrecognized checkbox value)
Test 1.9 was introduced during review round 2 to address Finding 2 (handling unrecognized values like `"maybe"`). When evaluated against `origin/main`'s `LabelGrid.tsx`, it previously passed because it asserted `indeterminate === false` and `checked === false`, which both evaluate to false on a plain text input. 

Because test 1.9 was created post-baseline as a regression guard rather than a red-first test, it is housed in its own `Review regression guards` describe block outside the red-first group 1 suite. It has been updated to explicitly assert `expect(editor).toHaveAttribute("type", "checkbox")` and verify activation cycling from undisplayable values.
