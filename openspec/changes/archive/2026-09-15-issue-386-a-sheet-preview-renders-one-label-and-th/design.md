## Context

See proposal.md, Why. Two hooks preview labels today. `useRowPreview` (`ui/src/lib/rowPreview.ts`)
takes one resolved label, fires on every key change with no debounce, aborts the in-flight request on
key change, and branches on `format`: `/api/render/label` for `single`, `/api/batch` with
`labels: [label]` for `sheet`. `useLivePreview` (`ui/src/lib/livePreview.ts`) serves the print form:
a 300 ms debounce keyed on the input, an `AbortController` per effect, a FIFO cache of 12 object URLs,
and the same one-label sheet branch. Both pages (`Connect.tsx`, `Import.tsx`) duplicate the same
derived state wholesale: `validateRow`, `rowInvalid`, `viewRows`, `hasErrors`, `firstValidId`,
`resolvedSelectedId`, and a `run()` that builds `labels` inline with `flatMap` over `rowsRef.current`
and `pruneDataForSubmit`. `resolveLabels` in `ui/src/lib/labelGrid.ts` expands rows by copies but
neither page calls it. `LabelGrid` renders the radio column only when `onSelectRow` is passed
(`LabelGrid.tsx:597`). `PreviewPane` renders from `{ url?, error?, loading }` and prefixes `error`
with `Preview failed:`. The Connect `Composer` is keyed on `connectionId:detail.id`, so a template
switch remounts it; the Import `CsvEditor` is not keyed and can render with a stale `detail` during a
switch. Both pages hide the pane while the grid is empty.

## Goals / Non-Goals

**Goals:**

- One function builds the batch body for both the submit path and the sheet preview, so the two
  cannot disagree by construction, and a test pins them equal.
- A sheet preview hook on the `useLivePreview` pattern: debounce, abort, keyed on the resolved batch.
- The refusal is decided by the predicates `run()` already runs, not by a parallel validation.
- The single path keeps its hook, its timing and its selection model.

**Non-Goals:**

- The print form's preview (`useLivePreview`, `PrintForm.tsx`). It collects one label, and one label
  on a sheet is what its Print submits; nothing there is the defect.
- Caching sheet PDFs across edits, a refresh control, a size threshold, or any server-side change.
- Fixing the Import page's stale-`detail` window during a template switch; the key includes the
  template id, so the preview re-renders once the new detail arrives, as today.
- Deduplicating the two pages' derived state beyond what this change touches.

## Decisions

**Decision: a new hook, `useSheetPreview`, rather than widening either existing hook.**

`ui/src/lib/sheetPreview.ts` exports `useSheetPreview(input, enabled, debounceMs = 300)` with
`input = { templateId, labels: ResolvedLabel[], startSlot }`. It copies `useLivePreview`'s effect
shape: compute the key, set a timer for the debounce, one `AbortController` per effect, clear both in
cleanup, drop any response whose signal is aborted, and render from state plus `enabled` only (the
repo's `react-hooks/refs` rule forbids reading refs during render, and `useLivePreview`'s comment
records why every `setState` sits inside the timer). It holds one object URL, revoked when replaced
and on unmount, as `useRowPreview` does.

Why not widen `useLivePreview`: its input is one label's `data` and its consumer is the print form.
Teaching it a `labels` array means a union input and a second body shape in a hook whose only caller
never sends one. Why not widen `useRowPreview`: the single path must keep firing immediately, and a
hook that debounces one branch and not the other is two hooks in one function. Why no cache: the
issue names debounce plus abort as the whole cost ceiling; a whole-sheet PDF is the largest thing the
UI renders, and twelve of them held as object URLs is memory spent on a back-edit that rarely happens.

**Decision: `useRowPreview` becomes single-only.**

Its `format` and `startSlot` inputs and the `/api/batch` branch are removed. After this change no
caller previews a sheet as one label, and a branch nobody reaches is the one-label sheet preview kept
alive under a second spelling. Its single behavior (immediate fire, abort on key change, idle with no
label, revoke on unmount) is untouched, and `rowPreview.test.ts` loses only the sheet case.

**Decision: `resolveLabels(rows, copies, dataFor)` is the one batch builder.**

The existing `resolveLabels` gains a required `dataFor: (row: LabelGridRow) => Record<string, ParamValue>`
and both pages call it with the same resolver, `(r) => pruneDataForSubmit(r.data, getRowInputs(r.id)
?? detail.inputs.default)`, from `run()` and from the preview. `run()` keeps reading `rowsRef.current`
(a blur-commit fires before the click handler and must be included, as its comment says); the
preview reads `rows`, the render state, which equals the ref after the render that follows every
`commitRows`. The debounce absorbs that one-render lag. Making `dataFor` required rather than optional
is deliberate: a caller that forgets to prune would submit deferred or inactive keys, which is the
drift this function exists to prevent.

Rejected: leaving the two `flatMap`s in place and asserting body equality in a test only. The test
would pin today's agreement, not prevent tomorrow's edit to one site.

**Decision: the refusal is computed in the page from `run()`'s own predicates.**

The page already derives `viewRows` (with validation), `hasErrors` and `total`. A pure helper in
`labelGrid.ts`, `sheetPreviewBlock(invalidPositions: number[], total: number): string | undefined`,
returns the spec's message for the first condition that holds (invalid rows before the cap) or
`undefined`. `invalidPositions` is
`viewRows.flatMap((row, index) => rowInvalid(row) ? [index + 1] : [])`: the index is taken from the
full grid before the filter, so invalid rows 2 and 5 are named 2 and 5. Filtering first and then
mapping would renumber them 1 and 2, which is not what the spec states. The helper owns the wording so the two pages cannot phrase it differently, and `labelGrid.test.ts` pins
the strings the spec states.

The pane state is composed in the page, next to the other derived state:

```
const preview = isSheet
  ? rowsPending ? { loading: true }
    : blocked ? { loading: false, blocked }
    : sheetPreview
  : rowPreview;
```

with `useSheetPreview({...}, isSheet && rows.length > 0 && !rowsPending && !blocked)` and
`useRowPreview({ templateId, label: isSheet ? undefined : previewLabel })`. Both hooks are always
called (hooks cannot be conditional); each is idle when the format is not its own.

`rowsPending` is checked before `blocked`, and the order is load-bearing. While the service is still
reporting a row's inputs, `getRowInputs` returns nothing for it and `validateRow` falls back to
`detail.inputs.default` (`labelInputs.ts:232`), which can mark a row invalid that the resolved inputs
will accept: a required entry the default list carries and the row's branch does not. A refusal
computed from that fallback would name a row the operator has nothing to fix and then vanish on its
own. So pending shows as rendering, never as a refusal, and `blocked` is only ever computed from
resolved inputs. The spec states this precedence.

**Decision: `PreviewState` gains `blocked?: string`; `PreviewPane` renders it as its own branch.**

A refusal is not `error`: the pane's `error` branch prefixes `Preview failed:`, and nothing failed.
The `blocked` branch renders the message alone, in the `--bad` colour, with no `<object>`. Order of
precedence in the pane: `loading`, then `blocked`, then `error`, then `url`, then the idle copy.

**Decision: the radio column is gated at the call sites, not in `LabelGrid`.**

`LabelGrid` already omits the column when `onSelectRow` is absent. Each page passes
`selectedRowId={isSheet ? undefined : resolvedSelectedId}` and
`onSelectRow={isSheet ? undefined : setSelectedRowId}`. `selectedRowId` state and the first-valid
fallback stay as they are; for a sheet they are computed and unused, which is cheaper than a second
code path around them. Import's grid before a template is chosen has `isSheet === false` and keeps
its radios, as today.

**Decision: the key is `JSON.stringify([templateId, labels, startSlot])` with each label's keys sorted.**

`useLivePreview` sorts `data` keys so insertion order cannot change the key; the sheet key does the
same per label. Stringifying up to 500 small objects per render is well under a millisecond and is
what makes "an unchanged batch sends nothing" hold without a deep-equal.

## Tests

- `sheetPreview.test.ts` (new): with fake timers, three rerenders inside the debounce produce one
  `fetch`; a `fetch` that never resolves has its `signal.aborted` flipped by a rerender with a new
  key, and the hook reports neither its URL nor an error; `enabled: false` sends nothing; the body
  carries every label and `start_slot`; unmount revokes the URL.
- `labelGrid.test.ts`: `resolveLabels` applies `dataFor` and expands copies adjacently;
  `sheetPreviewBlock` returns the spec's three strings (one row, several rows, cap) and `undefined`.
- `PreviewPane.test.tsx`: `blocked` renders the message without an `<object>` and without
  `Preview failed`.
- `Connect.test.tsx` and `Import.test.tsx`, each: a sheet template with 2 rows, `copies` 3 and
  `start slot` 1 sends one `/api/batch` preview with 6 labels and `start_slot: 1`, and the body
  Download then sends has equal `labels` and `start_slot`; no `input[name="preview-row"]` for the
  sheet, radios present for `single`; an invalid row sends no batch request and shows
  `Fix row N to preview the sheet.`, and filling the cell sends the request; invalid rows 2 and 5 of
  a 5-row grid show exactly `Fix rows 2, 5 to preview the sheet.`; `copies` over the cap shows the
  cap message and sends nothing; a `single` template with an explicitly selected row that is invalid
  still previews that row, and a non-2xx response for it shows `Preview failed:` with the service's
  message while Download keeps the disabled state the invalid row gives it. Pending precedence: the
  inputs endpoint is held open for one row whose fallback (default) inputs report a missing required
  entry that the resolved inputs do not carry; while held, the pane reads as rendering, no repair
  message is shown and no batch request is sent; once the inputs resolve, one batch request follows.
  The existing single-row tests stay green.
- `rowPreview.test.ts`: the sheet case is removed; the three single cases are unchanged.

## Risks / Trade-offs

- [Every settled edit renders up to 500 labels server-side] → the debounce and abort are the ceiling
  the issue accepts; the batch cap bounds the worst request, and a render the server refuses shows as
  a failed preview without gating the run.
- [A blur-commit and the preview key disagree for one render] → the debounce restarts on the next
  render's key; `run()` reads the ref, so Download is never behind. The body-equality test pins this.
- [The two pages drift] → the builder, the block helper and the message strings are shared; the page
  tests are written once per page against the same scenarios.
- [Import renders with a stale `detail` during a template switch] → unchanged from today; the key
  includes the template id and the preview follows the new detail. Named in Non-Goals.
- [Object URL for a large PDF held while the next render is in flight] → one URL at a time, revoked on
  replacement and on unmount, as `useRowPreview` already does.
