## 1. Shared builders in `ui/src/lib/labelGrid.ts`

- [x] 1.1 Give `resolveLabels(rows, copies, dataFor)` a required `dataFor: (row: LabelGridRow) => Record<string, ParamValue>` and build each label's `data` from it, keeping copies expanded adjacently in grid order; update the existing `labelGrid.test.ts` case to pass a resolver
- [x] 1.2 Add `sheetPreviewBlock(invalidPositions: number[], total: number): string | undefined` returning, in this order, `Fix row N to preview the sheet.` for one position, `Fix rows N, M to preview the sheet.` for several (comma-space separated, in the order given), `Over the 500-label limit; reduce the batch to preview the sheet.` when `total > MAX_BATCH_LABELS`, else `undefined`
- [x] 1.3 Test `resolveLabels` applies `dataFor` and expands copies adjacently, and `sheetPreviewBlock` returns exactly the three strings and `undefined` (invalid rows take precedence over the cap) in `labelGrid.test.ts`

## 2. Preview hooks

- [x] 2.1 Create `ui/src/lib/sheetPreview.ts` exporting `useSheetPreview(input, enabled, debounceMs = 300)` with `input = { templateId, labels: ResolvedLabel[], startSlot }`, on `useLivePreview`'s effect shape: key `JSON.stringify([templateId, labels, startSlot])` with each label's `data` keys sorted; a timer for the debounce and one `AbortController` per effect, both cleared in cleanup; `POST /api/batch` with `{ template, mode: "download", labels, ...(startSlot ? { start_slot } : {}) }` and `signal`; drop any response or error whose signal is aborted; non-2xx becomes `error` from the envelope message or `preview failed (<status>)`; one object URL held, revoked on replacement and on unmount; no cache; render from state plus `enabled` only (no ref reads during render, every `setState` inside the timer), returning `{ loading: false }` when disabled and `{ loading: true }` while a newer key is debouncing or in flight
- [x] 2.2 Make `useRowPreview` single-only in `ui/src/lib/rowPreview.ts`: remove the `format` and `startSlot` inputs and the `/api/batch` branch; keep immediate fire, abort on key change, idle with no label, revoke on unmount unchanged
- [x] 2.3 Add `ui/src/lib/sheetPreview.test.ts`: with fake timers, three rerenders inside the debounce produce one `fetch` carrying the last batch; a `fetch` that never resolves has its `signal.aborted` set by a rerender with a new key and the hook reports neither a URL nor an error for it; `enabled: false` sends nothing and reports not loading; the body carries every label in order and `start_slot`; an equal key on rerender sends no second request; unmount revokes the URL
- [x] 2.4 Remove the sheet case from `ui/src/lib/rowPreview.test.ts`; the three single cases stay unchanged

## 3. Preview pane

- [x] 3.1 Add `blocked?: string` to `PreviewState` in `ui/src/components/PreviewPane.tsx` and render it as its own branch: the message alone in the `--bad` colour, no `<object>`, no `Preview failed:` prefix; branch precedence `loading`, then `blocked`, then `error`, then `url`, then the idle copy
- [x] 3.2 Add a `PreviewPane.test.tsx` case: a `blocked` state renders the message with no `<object>`, no `<img>` and no `Preview failed` text

## 4. Call sites: `ui/src/pages/Connect.tsx` and `ui/src/pages/Import.tsx`

- [x] 4.1 In both pages define one resolver `dataFor = (r) => pruneDataForSubmit(r.data, getRowInputs(r.id) ?? detail.inputs.default)` (Import: `?? detail.inputs?.default ?? []`) and make `run()` build `labels` with `resolveLabels(rowsRef.current, submittedCopies, dataFor)` in place of the inline `flatMap`
- [x] 4.2 In both pages compute `invalidPositions = viewRows.flatMap((row, index) => rowInvalid(row) ? [index + 1] : [])` and `blocked = sheetPreviewBlock(invalidPositions, total)`, call `useSheetPreview({ templateId: detail.id, labels: resolveLabels(rows, copies, dataFor), startSlot }, isSheet && rows.length > 0 && !rowsPending && !blocked)`, call `useRowPreview({ templateId, label: isSheet ? undefined : previewLabel })`, and compose `preview = isSheet ? (rowsPending ? { loading: true } : blocked ? { loading: false, blocked } : sheetPreview) : rowPreview`, with `rowsPending` checked before `blocked`
- [x] 4.3 In both pages pass `selectedRowId={isSheet ? undefined : resolvedSelectedId}` and `onSelectRow={isSheet ? undefined : setSelectedRowId}` to `LabelGrid`, leaving `selectedRowId` state and the first-valid fallback as they are; the Import grid with no template chosen keeps its radios

## 5. Page tests, each written for `Connect.test.tsx` and for `Import.test.tsx`

- [x] 5.1 Sheet template with 2 valid rows, `copies` 3 and `start slot` 1: one `POST /api/batch` preview request with 6 labels in row order and `start_slot: 1`; activating Download then sends a body whose `labels` and `start_slot` deep-equal the preview's
- [x] 5.2 Sheet template renders no `input[name="preview-row"]`; a `single` template renders one radio per row and the existing selected-row preview tests stay green
- [x] 5.3 Sheet template with one row missing a required value: no `/api/batch` request, no `<object>`, pane reads `Fix row N to preview the sheet.`; filling the cell sends one batch request holding every row and the pane embeds the PDF
- [x] 5.4 Sheet template with a 5-row grid whose rows 2 and 5 are invalid: pane reads exactly `Fix rows 2, 5 to preview the sheet.` and the text does not begin with `Preview failed`
- [x] 5.5 Sheet template with 2 rows and `copies` set to 300: no `/api/batch` request and the pane reads `Over the 500-label limit; reduce the batch to preview the sheet.`
- [x] 5.6 Pending precedence: hold the inputs endpoint open for one row whose fallback (`inputs.default`) marks a required entry missing that its resolved inputs do not carry; while held, the pane reads as rendering, names no row and no batch request is sent; resolving the inputs sends one batch request holding every row
- [x] 5.7 Single template: select row 2, clear a required value in it, and stub `/api/render/label` to return a non-2xx envelope; row 2 stays the row requested, the pane shows `Preview failed:` with the service's message, and Download is disabled
- [x] 5.8 Sheet template: a settled edit to a cell, then to `copies`, then to `start slot`, each sends one batch request carrying the new `labels` count or `start_slot`; a re-render with the batch unchanged sends none

## 6. Gates

- [x] 6.1 Run `cd ui && npm run lint && npm run test && npm run build`
- [x] 6.2 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features` and `cargo test` (no Rust is touched; the gates still run)
