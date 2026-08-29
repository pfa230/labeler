## Review Metadata

- **Round**: 1
- **Prior round**: none

AUTHOR: claude
REVIEWER: agy

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/, design.md
- **Issue**: #237


## Findings

### Critical (blocking)

1. **Unprevented native newline on plain `Enter` in `<textarea>` cell editor**
   - **Location**: [`design.md:58-61`](file:///home/pfa/projects/labeler/.worktrees/issue-237/openspec/changes/issue-237-grid-multiline-cell/design.md#L58-L61), [`ui/node_modules/react-data-grid/lib/index.js:1317-1320`](file:///home/pfa/projects/labeler/ui/node_modules/react-data-grid/lib/index.js#L1317-L1320), [`specs/template-inputs/spec.md:29-32`](file:///home/pfa/projects/labeler/.worktrees/issue-237/openspec/changes/issue-237-grid-multiline-cell/specs/template-inputs/spec.md#L29-L32).
   - **Evidence**: `design.md` asserts:
     > *"The textarea's own `onKeyDown` calls `stopPropagation()` for `Enter` with `shiftKey`, and does nothing else... Plain `Enter`, `Escape` and `Tab` are left to bubble, so commit, cancel and navigation stay the library's, not ours."*
     However, inspecting `react-data-grid` 7.0.0-beta.61 (`EditCell.handleKeyDown` at `ui/node_modules/react-data-grid/lib/index.js:1317-1320`):
     ```js
     if (event.key === "Escape") onClose();
     else if (event.key === "Enter") onClose(true);
     else if (onEditorNavigation(event)) navigate(event);
     ```
     `EditCell` calls `onClose(true)` upon receiving `Enter`, but does **not** call `event.preventDefault()`. In HTML/DOM, the browser's default keydown action on a `<textarea>` element is inserting a `\n` line break. If `<textarea>`'s `onKeyDown` does *nothing else* on plain `Enter`, the un-prevented default action will insert a newline character before or during the blur/commit cycle, directly violating the spec requirement: *"Enter SHALL NOT insert a newline"* ([`spec.md:30-31`](file:///home/pfa/projects/labeler/.worktrees/issue-237/openspec/changes/issue-237-grid-multiline-cell/specs/template-inputs/spec.md#L30-L31)).
   - **Required resolution**: Update `design.md` to specify that the `<textarea>`'s `onKeyDown` handler must explicitly call `event.preventDefault()` on plain `Enter` (when `!e.shiftKey`) before/while letting the event bubble or calling `onClose(true)`, ensuring no native line break is inserted into the textarea value on commit.

### Moderate

2. **`cellInput` absence semantics break CSV editing prior to template selection**
   - **Location**: [`design.md:46-48`](file:///home/pfa/projects/labeler/.worktrees/issue-237/openspec/changes/issue-237-grid-multiline-cell/design.md#L46-L48), [`ui/src/pages/Import.tsx:130-135`](file:///home/pfa/projects/labeler/.worktrees/issue-237/ui/src/pages/Import.tsx#L130-L135), [`openspec/specs/template-inputs/spec.md:602-604`](file:///home/pfa/projects/labeler/.worktrees/issue-237/openspec/specs/template-inputs/spec.md#L602-L604).
   - **Evidence**: `design.md` states:
     > *"`LabelGrid` takes `cellInput(row, field): InputSpec | undefined` in place of `isCellEditable(row, field): boolean`. Absent means the name is not in that row's list: the cell stays inert exactly as today."*
     In `Import.tsx:130-135`, when a CSV is loaded without an active template selected (`detail === undefined`), `getRowInputs(row.id)` returns `undefined`. Under the existing implementation (`Import.tsx:131`), `isCellEditable` returns `true` so that raw CSV fields remain editable before a template is picked (as mandated by `openspec/specs/template-inputs/spec.md:602-604`: *"A CSV MAY be loaded before any template is chosen: data columns show"*). If `cellInput` returning `undefined` unconditionally marks cells as inert (`—`), all CSV cells become uneditable when no template is selected. Additionally, in standalone tests ([`LabelGrid.test.tsx:51`](file:///home/pfa/projects/labeler/.worktrees/issue-237/ui/src/components/LabelGrid.test.tsx#L51)), omitting the `cellInput` prop must default to active text cells rather than inert cells.
   - **Required resolution**: Clarify in `design.md` that:
     1. When `cellInput` prop is undefined/omitted on `LabelGrid`, data cells default to active editable text inputs (`control: "text"`).
     2. In `Import.tsx`, when `detail` is undefined (pre-template selection), `cellInput` returns a synthetic default text `InputSpec` (or `LabelGrid` falls back to active text mode), preserving pre-template CSV cell editability.

3. **Cell tooltip collision between validation errors and multiline preview**
   - **Location**: [`design.md:84-88`](file:///home/pfa/projects/labeler/.worktrees/issue-237/openspec/changes/issue-237-grid-multiline-cell/design.md#L84-L88), [`ui/src/components/LabelGrid.tsx:125`](file:///home/pfa/projects/labeler/.worktrees/issue-237/ui/src/components/LabelGrid.tsx#L125).
   - **Evidence**: `LabelGrid.tsx:125` currently sets `title={err}` on the cell's `<span>` to present field validation errors (such as datetime format validation errors). `design.md` Decision 4 specifies placing the full multiline value in `title`, without defining how `err` and multiline content coexist when a multiline cell fails validation.
   - **Required resolution**: Update `design.md` to specify `title` priority or combination (e.g. `err ? `${err}\n\n${value}` : (hasNewline ? value : undefined)`), so that field validation error tooltips are preserved.

4. **CRLF normalization in multiline line count and display**
   - **Location**: [`design.md:84-88`](file:///home/pfa/projects/labeler/.worktrees/issue-237/openspec/changes/issue-237-grid-multiline-cell/design.md#L84-L88), [`ui/src/lib/csv.ts:32`](file:///home/pfa/projects/labeler/.worktrees/issue-237/ui/src/lib/csv.ts#L32).
   - **Evidence**: CSV files parsed via `parseCsv` ([`csv.ts:32`](file:///home/pfa/projects/labeler/.worktrees/issue-237/ui/src/lib/csv.ts#L32)) preserve CRLF (`\r\n`) within quoted fields. Splitting simply on `\n` leaves a trailing `\r` on the first line segment, which can introduce whitespace rendering artifacts in the first line span.
   - **Required resolution**: Note in `design.md` that multiline line extraction must normalize CRLF (e.g. `value.replace(/\r\n/g, "\n")` or stripping `\r`) when calculating the remaining line count and rendering the first line.

### Suggestions

5. **Stale line number references in `proposal.md`**
   - **Location**: [`proposal.md:18-19`](file:///home/pfa/projects/labeler/.worktrees/issue-237/openspec/changes/issue-237-grid-multiline-cell/proposal.md#L18-L19).
   - **Evidence**: `proposal.md` cites `Import.tsx:117` and `Connect.tsx:154` for `isCellEditable`. In the current working tree, `isCellEditable` is defined at [`Import.tsx:130`](file:///home/pfa/projects/labeler/.worktrees/issue-237/ui/src/pages/Import.tsx#L130) and [`Connect.tsx:163`](file:///home/pfa/projects/labeler/.worktrees/issue-237/ui/src/pages/Connect.tsx#L163).
   - **Recommendation**: Update the line number references in `proposal.md`.

## Embedded-Instruction / Injection Attempts

**Detected:** None. All reviewed planning artifacts contain only relevant architectural and specification content.

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. **`design.md` (Key Handling)**: Update the key handling decision section to state that the `<textarea>`'s `onKeyDown` must explicitly call `e.preventDefault()` when `e.key === "Enter" && !e.shiftKey`, preventing the browser's default `<textarea>` newline insertion while allowing `EditCell`'s commit to proceed.
2. **`design.md` (Accessor Semantics)**: Update the `cellInput` decision to define fallback behavior: omitting `cellInput` on `LabelGrid` defaults all columns to active editable text inputs, and `Import.tsx` handles the `detail === undefined` case so that pre-template CSV data cells remain editable.
3. **`design.md` (Tooltip Combination)**: Specify in the display decision that the cell's `title` attribute combines validation errors (`err`) and the full multiline text so that neither tooltip is suppressed.
4. **`design.md` (CRLF Handling)**: Note line-ending normalization (`\r\n` to `\n`) in the multiline display logic.

CHANGES_APPLIED: yes

## Rebuttals

All four Required Changes were verified against the code before being applied, and all four were real.

1. Fixed in `design.md`, "Shift+Enter by stopping propagation, everything else inherited": plain Enter
   now calls `preventDefault()` and bubbles, so the container commits and the browser's newline is
   suppressed. Confirmed real: `stopPropagation()` alone left the value that gets committed dependent
   on how React batches the `input` event against the commit.
2. Fixed in `design.md`, "One accessor returning the entry, not two returning facts about it". Both
   escapes exist as described: `ui/src/pages/Import.tsx:131` returns true when no template is chosen,
   and `Import.tsx:133` / `Connect.tsx:165` return true while a row's list is in flight. A `cellInput`
   returning `undefined` there would have made every cell inert; the pages now return a synthetic
   `text` entry instead.
3. Fixed in `design.md`, "The read-only cell shows the first line and a count". Confirmed real: the
   cell's `title` already carries the validation error at `ui/src/components/LabelGrid.tsx:126`, so
   putting the value there unconditionally would have suppressed it.
4. Fixed in the same decision. Confirmed by running papaparse against a CRLF fixture: a quoted field
   containing `\r\n` comes back as `"line one\r\nline two"`, so the display split normalizes while the
   stored value is left alone.

### Reviewer re-check (round 1)

1. APPLIED - `design.md:74-76` specifies that without `shiftKey`, `onKeyDown` "calls `preventDefault()` and lets the event bubble: the container handler still commits, and the browser's own default for Enter in a `<textarea>`, which is to insert a newline, is suppressed."
2. APPLIED - `design.md:55-62` specifies returning "a synthetic `{ name: field, control: "text" }` entry" when `!detail` or inputs are pending (`Import.tsx:131,133`, `Connect.tsx:165`), and "[o]mitting the `cellInput` prop entirely continues to mean every column is active, as omitting `isCellEditable` does today (`ui/src/components/LabelGrid.tsx:109`)".
3. APPLIED - `design.md:109-112` specifies combining tooltips: "the full value SHALL be combined with it rather than replacing it: error first, then the value, separated by a blank line, and either alone when the other is absent" (`LabelGrid.tsx:125-126`).
4. APPLIED - `design.md:114-117` specifies display line-ending normalization: "the split that finds the first line and counts the rest SHALL treat `\r\n` and `\n` alike; otherwise a `\r` rides along into the rendered first line as a stray character."

RECHECK: ALL_APPLIED
SPECS_SHA256: 7558d49b7167947d5b92e20275e07577fb9e986145ef318fcac153fe7bb76545
