## Context

See proposal.md Why. Current `ui/src/components/LabelGrid.tsx` registers seven inline editors
through `registerInlineEditor` (`TEXT_EDITOR`…`DATETIME_EDITOR`) and the column `editor` predicate
decides which one opens. SVAR's grid opens that editor only on `dblclick -> exec("open-editor")` (plus
F2), with no flag for single-click or always-on. `DataCell` at rest renders plain text (or an
indeterminate checkbox/select placeholder) and discards the live control until the overlay appears.
`PreviewCell` and `ActionsCell` show `cell` is a column-level template that can render a live control,
but the vendor's click handler only exempts `closest("input")` (`react-grid/dist/index.es.js:1487`) and
otherwise dispatches `focus-cell` which focuses the cell wrapper (`react-grid/dist/index.es.js:223`);
with SVAR 2.7.3 a click on a focused custom textarea moves `document.activeElement` from `TEXTAREA` to
`DIV[role=gridcell]`, so textarea/select always-on controls would lose focus without extra handling.
`cellInput` supplies the per-row, per-column `InputSpec`; `disabled` forces read-only while a batch is
in flight. Validation markers, `list`/`image` handling, column ordering (`inputs.all` union, CSV header
order for Import), and the `update-cell` intercept that commits to `onRowsChange` are the surrounding
constraints the new rendering must preserve.

## Goals / Non-Goals

**Goals:**

- Render each batch-grid cell's control always on, with no gesture, by moving rendering from the
  overlay editor path to the column `cell` path.
- Keep every existing control type/validation (`text`, `textarea`, `select`, `checkbox`,
  `integer`/`number`, `date`/`datetime`) and keep `list`/`image` read-only and `disabled` read-only.
- Keep per-row editability from `cellInput` and keep commits on `update-cell`/`onRowsChange`.
- State Tab/Shift+Tab as the sole keyboard navigation, appearance as bordered input on surface at rest,
  and removal of the dead editor registration.

**Non-Goals:**

- Adding or changing any wire shape, template schema, or Rust service code.
- Introducing deferral for grids, changing validation rules, or changing column-order rules.
- Adding custom focus management, Enter-to-move, or arrow-between-cells navigation.
- Editable `list`/`image` controls (out of scope per `template-inputs` tolerate rule; separate issues).

## Decisions

**Decision: Render live controls from `cell`, not from `registerInlineEditor`.**

Why: `cell` is the vendor's documented column template; `LabelGrid` already uses it for `PreviewCell`
(radio) and `ActionsCell` (two buttons) and the vendor's row-select handler already ignores clicks
inside inputs. Using it for data cells makes the control the cell, so the double-click indirection has
nothing to open. The intercept on `update-cell` / `onRowsChange` stays the commit path, which is the
same flow the vendor documents for an external editor. Remove `registerInlineEditor`, the seven
`*Editor` components (`TextEditor`…`DateEditor`), the `CellEditor` dispatcher, and the `editor`
predicate; keep `DataCell` as the single rendering path and make it return a controlled input/select
when `cellInput` reports an editable entry.

Rejected: `api.on("focus-cell") -> exec("open-editor")` to make a single click open the overlay.
Cheaper, but it preserves the two-state cell and the ask is for cells that are editable from the
start, with editable vs read-only distinguishable without clicking.

**Decision: Keep `cellInput` as the per-row editability source; plain-text fallback for read-only.**

Any cell where `cellInput` returns `undefined`, where `control` is `list`/`image`, or where
`disabled` is true renders plain text (display text, `—`, or the existing `⚠` marker). Otherwise it
renders the corresponding control: `input type=text`, `textarea`, `select` (with `""` → `"(none)"`
plus retained out-of-range value), `input type=checkbox` with indeterminate for `unset`,
`input type=number` with `min`/`max`/`step`, `input type=date`/`datetime-local`. This matches the
per-control list in the spec delta and preserves the existing validation/marker/multiline branches
(`+N` indicator, `+N` title, checkbox indeterminate).

**Decision: Keyboard is Tab/Shift+Tab in DOM order, with explicit suppression of grid hotkeys.**

Why: With an INPUT/TEXTAREA focused, `store`'s hotkey dispatch treats `isInput` and returns early from
arrow handlers, so always-on inputs and the previous arrow-between-cells navigation cannot coexist.
Tab is the browser's native focus walk and needs no code; the grid windows rows so Tab stops at the
rendered window edge rather than scrolling — accepted per issue. No Enter-moves-down, no arrow
navigation, no custom focus handler. For `textarea`, Enter inserts a newline (native); no commit/abandon
key exists because there is no overlay to commit.

The installed `grid-store` (`ui/node_modules/@svar-ui/grid-store/dist/index.js:2`) still calls
`preventDefault()` for ArrowUp/ArrowDown **before** returning when `isInput` is true, so textarea
vertical navigation and number stepping would be suppressed, and the `isInput` detector excludes
`SELECT` (`t(a)` checks only INPUT/TEXTAREA), so select arrows would still invoke grid navigation.
Removing `registerInlineEditor` does not disable these handlers. Therefore LabelGrid SHALL explicitly
prevent the grid store from consuming control keystrokes: (a) each always-on control SHALL stop
propagation of ArrowUp/ArrowDown/ArrowLeft/ArrowRight (and Home/End where relevant) at the control so
the document-level grid hotkey listener never receives them, and (b) LabelGrid SHALL intercept the
store's `hotkey` action (`api.intercept("hotkey", ...)`) and return `false`/suppress when
`event.target` is an INPUT, TEXTAREA, or SELECT inside an always-on cell. This preserves native
textarea cursor movement, number input stepping, and select dropdown navigation without reintroducing
grid cell navigation.

The vendor's click handler also only checks `closest("input")`; clicks inside `TEXTAREA`/`SELECT`
dispatch `focus-cell` which focuses the wrapper (`:223`), and the wrapper also handles bubbling `focus`
and can refocus itself (`:249`). Keyboard suppression alone does not fix clicks, and `focus-cell` carries
only `row`, `column`, and `eventSource` (vendor `:1489`, official API `docs.svar.dev/react/grid/api/actions/focus-cell/`),
so inspecting an event target is unsupported. Therefore LabelGrid SHALL retain pointer and focus via
control-level suppression while retaining the pointer path: each always-on control SHALL stop propagation
of `mousedown`/`click` and of `onFocus` (bubbling focus) so neither the click dispatch nor the wrapper's
focus handler runs for that control. After initial mount the first editable control (without a preview
column) is focused, and subsequent clicks and Tab/Shift+Tab keep `document.activeElement` on the intended
control; verified with SVAR 2.7.3 by asserting initial focus and focus retention after click and Tab.

**Decision: Appearance is bordered input on surface at rest.**

An editable cell's control at rest draws a visible border on the surface colour; a read-only cell draws
plain text. That satisfies the issue's requirement that editable vs `list`/`image`/`disabled` be
distinguishable without interaction, without hover/focus tricks. Implement via a class on the `cell`
container (e.g., `border border-input bg-background`) versus the muted `inertStyle` path already used
for `list` empty.

**Decision: Preservation, multiline, and unrepresentable values reuse the same always-on contract.**

- Out-of-range `select` value is retained as an extra retained option; leaving the control without
  choosing leaves it. This is the same branch that handles display as before, now always-on.
- Native controls cannot represent some stored values: `date`/`datetime` holding an offset-bearing
  RFC 3339 instant (`2026-09-01T12:00:00Z`), `integer`/`number` holding a non-numeric string, `checkbox`
  holding a malformed value, and the general fallback previously at `spec.md:1170` for any malformed
  text. Native inputs display those as empty/unset, which would misrepresent what will submit. The cell
  SHALL render the operable control in its empty/unset visual state **plus** an adjacent text adornment
  showing the stored raw value (`title`/`aria-describedby` and visible text), reusing the text fallback
  that was dropped. Rendering or focusing without an explicit user edit SHALL NOT mutate the stored value;
  only an explicit change (typing, selecting, checking) replaces it and clears the adornment, after which
  normal validation applies. This makes date/number/checkbox correction operable and testable.
- An empty required cell SHALL keep its control operable alongside its diagnostic, not behind a
  replacing marker. The previous draft made `⚠` override the control (spec marker paragraph and
  `DataCell` early-return), which left an initially missing required value — and any value cleared
  during editing — with no way to repair the row, while `Connect.tsx:226`/`Import.tsx:155` block the
  run immediately. The control stays rendered and the marker is surfaced as an adjacent adornment
  (`aria-describedby`/`title` plus visual `⚠ <error>` alongside the control) so the cell can be
  filled, cleared, and refilled without losing editability.
- Multiline: a cell whose stored value holds a newline SHALL always indicate continuation (`first line
  +N` with `title` holding full value), for every control. For a read-only cell this is the existing
  `+N` display. For an editable `text` cell holding an imported multiline value, `input type=text`
  would otherwise strip the newline and display `firstsecond` (verified). The editable cell SHALL remain
  an operable single-line input but show `first` truncated with `+N` adornment and `title` holding the
  full `first\nsecond` alongside the input, while the stored value retains the newline until an explicit
  edit replaces it. `textarea` cells keep their native multiline input; both cases submit the stored
  newline unaltered if left without explicit change.

These are existing guarantees that become properties of the always-on control rather than of an editor
open/close cycle.

**Decision: Preserve grid-cell accessibility when removing `editor`.**

SVAR derives each cell wrapper's `aria-readonly` directly from the column `editor` property
(`react-grid/dist/index.es.js:256`); removing `editor` would therefore mark every data cell as
read-only in the accessibility tree, contradicting the WAI-ARIA grid pattern where `aria-readonly`
describes cells where editing is disabled. The obsolete `editor` mechanism SHALL remain removed.
LabelGrid SHALL instead synchronize each data-cell wrapper's `aria-readonly` with actual per-row
editability derived from `cellInput`/`control`/`disabled`: `false` for operable controls (has entry and
not `list`/`image` and not `disabled`) and `true` for missing entries, `list`, `image`, and `disabled`,
updating on transitions into and out of `disabled` without a remount. Verified with component assertions
for editable/read-only cells and for `disabled` toggles.

## Risks / Trade-offs

- [Loss of arrow-key cell navigation] → Accepted per issue; arrow keys now move caret inside the
  focused field. Mitigation is spec statement, Tab as the documented walk, and explicit suppression of
  grid-store ArrowUp/ArrowDown `preventDefault` so textarea/number/select native behavior is preserved.
- [Tab stops at windowed rows] → SVAR windows rows; Tab cannot scroll to off-screen rows without custom
  focus code, which the issue explicitly rejects. Accepted; operators paginate/scroll then Tab.
- [Initially invalid required cells must remain repairable] → Marker no longer replaces the control;
  control plus diagnostic are rendered together. Added coverage for filling an initially invalid cell
  and for clearing then refilling one, which the previous draft blocked.
- [Grid hotkeys consuming control keystrokes] → The store's `isInput` excludes SELECT and still
  preventDefaults ArrowUp/Down for INPUT/TEXTAREA. Mitigation is `stopPropagation` on the control plus
  `intercept("hotkey")` suppression when target is INPUT/TEXTAREA/SELECT; verified with textarea,
  number, and select-specific tests.
- [Clicks inside textarea/select steal focus] → Vendor click handler only exempts `closest("input")`;
  textarea/select clicks dispatch `focus-cell` (`:1487` → `:223`) and the wrapper's bubbling `focus`
  can refocus itself (`:249`); `focus-cell` carries only `row`/`column`/`eventSource` so target-based
  interception is unsupported. Mitigation is `stopPropagation` on `mousedown`/`click` and on
  `onFocus` propagation at each control (retaining pointer suppression) so neither path moves focus to
  the wrapper; verified with SVAR 2.7.3 by asserting initial focus on first editable cell without preview
  column and retention after click and Tab.
- [Removing `editor` breaks `aria-readonly`] → SVAR derives `aria-readonly` from `editor` (`:256`);
  removing it would mark every editable cell read-only. Mitigation is synchronizing each data-cell
  wrapper's `aria-readonly` with actual editability (`false` for operable, `true` for missing/`list`/`image`/`disabled`)
  and updating on `disabled` transitions; verified with editable/read-only and `disabled` toggle assertions.
- [Editable `text` holding multiline from import strips newline] → `input type=text` shows
  `firstsecond` for `first\nsecond`. Mitigation is `first +N` adornment with `title` holding full
  value alongside the operable input, stored newline preserved until explicit edit; covered explicitly.
- [Native control cannot represent stored value] → `input type=date` shows empty for offset-bearing
  instant, `number` shows empty for `abc`, etc., misrepresenting what will submit. Mitigation is
  operable empty/unset control plus adjacent text adornment of raw stored value, with explicit-change
  boundary (focus/render alone does not mutate); restores general fallback from `spec.md:1170`.
- [More DOM nodes (one control per visible cell)] → Windowed rows bound the count; still within React
  grid's normal budget. No mitigation needed.
- [Stale `select`/`date` values silently retained] → Same as today, just more visible because the
  control is always rendered. Keep the retained-option and text fallback branches and validate via tests.
- [Vendor `cell` vs `editor` contract] → Verified that `cell` rendering an input does not trigger
  vendor row-select (existing `PreviewCell`/`ActionsCell` prove it for INPUT; clicks now also handled
  for TEXTAREA/SELECT); commits via `onChange` → `onRowsChange` match the documented external-editor flow.

## Migration Plan

No data migration. The change is UI-only. Deploy as part of normal build; `cd ui && npm run lint &&
npm run test && npm run build` green is the gate. Rollback is revert of the single commit.

## Open Questions

None — the issue decides keyboard (Tab only), appearance (bordered at rest), control set (same types),
read-only cases (`list`/`image`/`disabled`), and removal of `registerInlineEditor`/`editor`.
