## Why

For a `sheet` template the Connect and Import pages preview a sheet holding exactly one label, chosen by
a per-row radio, while Print and Download submit every row expanded by `copies` from `start_slot`
(`ui/src/lib/rowPreview.ts:31` posts `labels: [input.label]`; `ui/src/pages/Connect.tsx:283-291` and
`ui/src/pages/Import.tsx:273-283` post the whole batch). The operator checking slot order, where
`start_slot` lands, whether the last row spills onto a second page or how copies interleave is shown
none of it. The preview and the artifact disagree, which is the one thing a preview must not do.

Implements #386.

## What Changes

- **BREAKING** For a `sheet` template the preview renders the batch Print and Download would submit:
  every row, expanded by `copies`, from `start_slot`, through the same `POST /api/batch` body. The
  one-label sheet preview is gone; nothing spells it any more.
- **BREAKING** For a `sheet` template the grid renders no `preview-row` radio column. With the whole
  sheet on screen there is nothing to select. The column stays for a `single` template, whose preview
  is one label chosen by the radio with the existing fallback to the first valid row, unchanged.
- A sheet preview refuses when the submit path would refuse for what the grid holds: an invalid row,
  or a batch over the 500-label cap. No PDF is shown; the pane names the blocking rows by their grid
  position (or names the cap), and the preview renders the moment the grid is repaired. An empty grid
  is not a refusal: the pane is not shown at all with no rows, as today.
- The sheet preview is live: it follows every edit to the grid, `copies` and `start slot`, debounced
  (~300 ms, keyed on the resolved batch) with the in-flight render aborted when the key changes. No
  refresh control and no size threshold.
- The single-row preview keeps its request shape, its timing and its selection model. Its hook loses
  the sheet branch it no longer serves.
- No change to `POST /api/batch`, to `POST /api/render/label`, to the template schema or to any Rust
  code. No new wire fields.

## Capabilities

### New Capabilities

- `batch-grid-preview`: what the preview pane on the Connect and Import pages shows for a batch grid,
  by template format: the artifact the grid would submit for a `sheet`, one selected row for a
  `single`; when the sheet preview refuses and what it says; how it follows the grid. Supersedes the
  frozen `docs/SPEC.md` §2.0 paragraph "The CSV Import and Homebox Connect pages render an on-demand,
  selected-row preview…", which is the only place this behavior is documented today.

### Modified Capabilities

None. `template-inputs` names the grid's preview column once, in a scenario ("a grid with no preview
column is freshly mounted"), as a fixture condition rather than a requirement, and this change makes
that fixture the sheet grid's ordinary state; the scenario's contract is unaffected.

## Impact

- `ui/src/lib/rowPreview.ts`: single-label only; the `format`/`startSlot` inputs and the `/api/batch`
  branch go.
- New `ui/src/lib/sheetPreview.ts`: debounced, abortable whole-batch preview hook on the
  `useLivePreview` pattern, keyed on the resolved batch.
- `ui/src/lib/labelGrid.ts`: the batch body builder (`resolveLabels`) takes the per-row data resolver
  so the submit path and the preview build the batch through one function; a pure helper decides
  whether the sheet preview is blocked and by which rows.
- `ui/src/pages/Connect.tsx`, `ui/src/pages/Import.tsx`: build the batch once for both paths; pass
  `onSelectRow`/`selectedRowId` to `LabelGrid` only for `single`; choose the pane state by format.
- `ui/src/components/PreviewPane.tsx`: a blocked state that names rows, distinct from a failed render.
- Tests: `rowPreview.test.ts`, new `sheetPreview.test.ts`, `labelGrid.test.ts`, `PreviewPane.test.tsx`,
  `Connect.test.tsx`, `Import.test.tsx`.
- No Rust, no API, no template change.
