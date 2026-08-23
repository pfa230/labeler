## Why

Implements [#170](https://github.com/pfa230/labeler/issues/170).

The Connect page's browse table is hand-written `<table>` JSX. It renders whatever order the connector
returned and nothing else: no sorting, no way to narrow the rows already on screen. A Homebox
connection routinely loads 50-200 items, and finding one, or grouping by location, or reading the
cheapest first, means scrolling and squinting. The server-side filter bar above it re-queries Homebox
and resets the cursor, which is the wrong tool for "show me the loaded rows sorted by quantity".

## What Changes

- The browse table becomes a **SVAR React DataGrid** (`@svar-ui/react-grid`, MIT), replacing the
  hand-written table markup in `ConnectorBrowser.tsx`. The grid supplies the table structure, the
  header semantics, and automatic measurement of wrapped content-sized rows. It does **not** supply all
  of the behavior below: implementation against the shipped package established that the ordering and
  matching rules, the filter input and its accessible name, the selection checkbox, and the sort glyph
  are ours (see `design.md`). The claim to hold onto is narrower than "configured, not written": we do
  not own the grid's layout, measurement or header plumbing, and we do own the rules.
- **Column sorting.** Clicking a column header cycles unsorted → ascending → descending → unsorted.
  Order is computed from the column's declared `FieldType`: lexicographic and case-insensitive for
  `text` and `badge`, numeric for `number` and `money`, chronological for `date`. A cell the connector
  omitted sorts last in both directions, so a blank never displaces a real value.
- **Per-column filtering.** Each visible column carries a filter control in its header: a text input
  for `text`, `number`, `money` and `date`, matching case-insensitively on the cell as displayed.
  Filters across columns are ANDed. Filtering is live and re-applies as new pages arrive.
- **Both act on the rows already loaded, and say so.** Sorting and filtering never re-query the
  connector and never touch the cursor. While the connector reports more rows available, the table
  states what is in scope ("87 of 120 loaded shown"), so a sort is never mistaken for a sort of the
  whole Homebox inventory.
- **The table becomes a bounded scroll region.** The grid is a fixed-height inner scroll viewport by
  construction, so the browse list scrolls within its own region rather than extending the page. This
  is a visible change to how the Connect page reads, and it is the one thing here a user would notice
  that is not an added capability.
- **Everything the table does today is preserved, except its page-flow layout**: the selection checkbox
  with its 200-row cap and disabled state, cross-resource selection with its label snapshot and visible/hidden split, the name
  cell linking to the row's Homebox page, the per-row Drill in button, the Columns visibility picker
  and its `localStorage` persistence, the server-side filter bar, and Load more.
- Sorting and filter text are **ephemeral**: they reset when the resource tab changes, when drilling
  into a relationship, and when the parent is cleared. Column visibility keeps persisting as it does
  today. A remembered sort is harmless; a remembered filter that silently hides rows on a later visit
  is not.
- Not in this change: sorting or filtering server-side (the browse API has no sort parameter and the
  Homebox connector sends no ordering, so it would be a connector contract change), filter operators
  beyond substring match (numeric and date ranges), and multi-column sorting.

## Capabilities

### New Capabilities

- `connector-browser`: what the Connect page's browse table presents and how a user narrows it. Covers
  the view controls that act on loaded rows (sorting, per-column filtering, column visibility), the
  scope those controls operate on, and their interaction with selection and cursor pagination.

### Modified Capabilities

None. `openspec/specs/` holds `auto-length-layout`, `connector-field-transforms` and
`template-registry`, none of which this change touches. The browse table is described only in frozen
`docs/SPEC.md` §12, so the new capability carries the complete post-change contract for the part it
supersedes and names it.

## Impact

- **UI.** `ui/src/pages/connect/ConnectorBrowser.tsx` (the table becomes a grid; sorting and filtering
  state), a new `ui/src/lib/connectorSort.ts` (typed comparators) and `ui/src/lib/connectorFilter.ts`
  (per-column predicates) so the ordering rules are unit-testable independently of the grid,
  `ui/src/theme.css` (mapping SVAR's `--wx-*` variables onto the app's tokens for light and dark).
  `ui/src/pages/connect/connectorColumns.ts` is unchanged.
- **Tests.** `ui/src/pages/connect/ConnectorBrowser.test.tsx` grows sorting and filtering cases; its
  five existing cases are rewritten against the grid's DOM. New unit tests for the comparators and
  predicates.
- **Dependencies.** Adds `@svar-ui/react-grid` (MIT) and its ten `@svar-ui/*` runtime packages,
  measured at 83.3 KB JS + 16.0 KB CSS gzipped untree-shaken against a current bundle of 132.8 KB.
  `react-data-grid` stays, since `LabelGrid` is unaffected.
- **Backend.** None. No API, no Rust code, no schema change. `POST /connections/{id}/browse` is called
  exactly as it is today.
- **Docs.** ADR-0064 and its row in `docs/adr/README.md`.
- **Compatibility.** Client-side only, and additive in capability: a user who never touches a header
  sees the rows in the same order the connector returned them. Not purely additive in presentation, as
  the list gains a bounded scroll region in place of extending the page.
