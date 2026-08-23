## Context

See `proposal.md` - Why. What shapes the approach is what the browse table already is and already
carries.

`ui/src/pages/connect/ConnectorBrowser.tsx` is ~520 lines of hand-written JSX: a `<table>` fed by
`browseConnection`, a resource tab strip, the connection's typed server-side filter bar, a Columns
visibility picker persisted to `localStorage` by `connectorColumns.ts`, a checkbox column, a name cell
that links to Homebox when the row carries a `url`, a per-row Drill in button, Load more over an
opaque cursor, and a selection summary. The selection itself is unusual and lives above the component:
`SelectedRow[]` spans resources, snapshots a label and breadcrumb at selection time, caps at 200 to
match the materialize limit, and splits the summary into "in this view" (loaded) versus elsewhere.

Constraints the choice has to survive: the app has no component library and themes through CSS custom
properties in `ui/src/theme.css`; tests are vitest + jsdom + Testing Library and query real DOM, so
every row must be in the document; and `ui/src/components/LabelGrid.tsx` already uses
`react-data-grid` for the editable label grid, so whatever is chosen here either joins it or sits
beside it.

## Goals / Non-Goals

**Goals:**

- Sorting and per-column filtering come from a maintained component wherever it is sound to take them
  from one. Where it is not (the ordering rules, the filter input and its accessible name, the sort
  glyph), the design says so explicitly rather than implying more comes from the vendor than does.
- The ordering and matching *rules* stay ours and stay testable without rendering anything, because
  they encode connector field types rather than grid mechanics.
- Rows grow to fit wrapped Homebox descriptions rather than truncating them to one ellipsised line.
- Nothing in the existing selection, drill-down, pagination or column-visibility behavior changes.

**Non-Goals:**

- Migrating `LabelGrid` to the same grid. Its job (inline editing with validation) is not this job.
- Building a general `DataTable` abstraction over both tables. There are two tables, and speculative
  unification would be the third thing to maintain.
- Any change to the browse or materialize API contracts.

## Decisions

### Adopt SVAR React DataGrid (`@svar-ui/react-grid`) for the browse table

**ADR-0064** records this decision.

The candidate field was enumerated openly rather than from recall, using npm registry search across
six phrasings (282 distinct packages) plus direct evaluation of the known incumbents. Note that npm
keyword search alone returns neither SVAR nor TanStack, because their descriptions do not use the
search terms; no single enumeration method covers this field.

Facts below were verified directly against published packages and vendor docs on 2026-08-22, not taken
from summaries.

| candidate | verdict |
| --- | --- |
| **SVAR react-grid 2.7.3** (MIT) | **Chosen.** `autoRowHeight` gives wrapped, content-sized rows (`.wx-row.wx-autoheight { height: max-content }` with `white-space: normal` cells), which is the one capability that decides this table; per-column header filters (`header.filter`) are built in; headers emit `role="columnheader"` and `aria-sort` with `ascending`/`descending`/`none`; it has no built-in selection column, which suits us. Two costs measured during implementation and accepted below: it windows rows unconditionally, and it owns a bounded-height scroll viewport. |
| react-data-grid 7.0.0-beta.61 (MIT, already a dependency) | Rejected on **automatic measurement of wrapped content**, which is narrower than "cannot grow". It does accept a per-row `rowHeight` callback, and its `white-space: nowrap; text-overflow: ellipsis` cell CSS can be overridden, so variable row heights are reachable. What it will not do is *measure* how tall a wrapped description turns out to be: we would have to compute each row's height ourselves from text length and column width and feed it back, which is application-owned layout machinery for every render and every resize. Auto row height remains an unimplemented request years old (adazzle#167, Comcast#190, #671, #2252). Its fixed-height scroll viewport is **not** a distinguishing reason: SVAR has one too (see "The grid owns a bounded scroll viewport" below). |
| AG Grid Community 36.1.0 (MIT) | Rejected on fit and complexity, and the rejection is a **preference, not a forced hand**. Everything needed here is free, and `domLayout='autoHeight'` plus `wrapText` would render the screen, including the page-flow layout SVAR cannot give (see below). Row selection is **opt-in**, so the round-1 claim that its selection model "would become a second source of truth" was wrong: a custom application-owned checkbox cell works there as it does here. The honest remaining reasons are narrower: its docs mix Enterprise features into Community pages, and it is a markedly larger surface for one table. Its one genuine advantage, typed filter operators (numeric and date ranges), is outside #170. **If the bounded viewport accepted below later proves wrong for this screen, AG Grid is the first thing to revisit, and this row should not be read as having closed that door.** |
| TanStack Table v9.1.2 (MIT) | Rejected. Sound and by far the most adopted, but it ships no DOM, so the header semantics, filter inputs and their labels, and the live region would all be ours to write and prove. That is the work this change is trying not to own. |
| Hand-rolled on the existing table | Rejected for the same reason, with the vendor risk traded for defect risk. |
| MUI X DataGrid | Rejected: requires `@mui/material` and Emotion. |
| Glide Data Grid | Rejected: paints cells to canvas, so no row DOM and the test suite cannot see rows. |
| `@grafana/react-data-grid` | Rejected: a fork of react-data-grid pinned at beta.58, inheriting the same layout limits. |
| fixed-data-table-2; `@inovua/reactdatagrid-community`; ka-table | Rejected: fixed row heights by design; last published 2023-07-31; last published 2025-05-02. |
| Kendo, Syncfusion, DevExtreme, Handsontable, RevoGrid | Rejected: commercial licences, or core features behind a paid tier. |
| rsuite-table, LyteNyte, `@microsoft/fabric-datagrid` | Not evaluated in depth. Listed so the omission is visible rather than silent: rsuite-table declares no React 19 peer, LyteNyte is Apache-2.0 and plausible, fabric-datagrid requires Griffel, a third styling system. |

Size was deliberately **not** used as a criterion. The SVAR family measures 83.3 KB JS + 16.0 KB CSS
gzipped untree-shaken against a current bundle of 132.8 KB, and the assets are served from `ui/dist`
over a LAN and cached, so the difference between candidates in this band does not affect anyone.

### The ordering and matching rules live in our own pure modules

`ui/src/lib/connectorSort.ts` exposes a comparator selected by `FieldType`; `ui/src/lib/connectorFilter.ts`
exposes the per-column predicate. The grid is handed these; it does not decide them.

Two reasons beyond taste. The rules are connector semantics (a `money` cell is numeric even though it
arrives as JSON `number | string`, an absent cell sorts last in both directions, ties keep connector
order), so they belong with the connector's other client-side rules, next to `connectorRows.ts`. And
they are the part worth testing exhaustively, which is far cheaper as pure functions than through a
rendered grid. It also means the spec's ordering requirements survive a later change of grid.

A column's declared `FieldType` does not constrain the cell: `CellValue` is `string | number` on the
wire (`ui/src/api/connectors.ts:43`, from the untagged `CellValue::Text | CellValue::Number` in
`src/connector/mod.rs`), and `FieldType` is a property of the column definition, independent of what
any row carries. A `number`, `money` or `date` column can therefore hold a value that is not a number
or not a date, including today: Homebox's `purchaseDate` reaches the client as text. The comparator
therefore classifies each cell as interpretable or not, and treats an uninterpretable cell exactly as
an absent one, per the specs. It does not fall back to text comparison, which would make a column's
ordering rule depend on its contents.

Alternative considered: configuring the grid's own sort functions inline in the column definitions.
Rejected because it puts the type rules inside grid configuration, where they can only be tested by
rendering.

### The grid renders the sort state; it does not own the ordering

SVAR's own sort model does not match the contract in the specs, so the grid is used for the header
control and its indicator only. Verified against the shipped `@svar-ui/grid-store` code, not its docs:

- Its `sort-rows` action carries `{ key, add, order }` with `order` defaulting to `"asc"`, and
  `sortMarks` only ever holds `asc` or `desc`. **There is no third, unsorted state.**
- On sort it does `const nextData = [...data]; nextData.sort(sorter); setState({ data: nextData })`. It
  **clones** before sorting, so the array we hand it is not mutated. The round-1 draft of this design
  said "destroyed in place"; that was wrong, and the corrected mechanism is recorded here rather than
  quietly dropped. What still holds is the consequence: the store's own `data` becomes the sorted order
  and the connector's order is not retained anywhere inside the grid, so a third "restore the
  connector's order" state has nothing to restore from if the grid owns the rows.
- `add` is what accumulates a second sorted column; when falsy the new key replaces the existing one.
  Multi-column sorting is therefore reachable by Ctrl/Meta-click unless suppressed.

The design consequence: `ConnectorBrowser` keeps `rows` exactly as loaded, in connector order, and
derives what the grid renders. `sort-rows` is intercepted rather than allowed to reorder anything: the
handler cycles our own three-state sort state for that column, always replacing rather than
accumulating (`add` forced falsy), so a Ctrl/Meta-click behaves as a plain click and multi-column
sorting cannot be entered. Our comparator then produces the derived array. `sortMarks` is driven from
our state purely so the grid draws the indicator and emits the matching `aria-sort`; the third state
is expressed by clearing it, which the grid already renders as `aria-sort="none"`.

This is why the ordering rules live in our modules rather than being handed to the grid as sort
functions: the grid's `sortMarks` has no third state to express "unsorted" with, and its store keeps
only the sorted order, so there is nothing left to restore the connector's order from.

One further mechanic, found in `HeaderCell.jsx` during implementation: **a header cell that carries a
filter is not sortable.** Its click handler returns early on `cell.filter`, and it reports
`aria-sort="none"` unconditionally. A column that both sorts and filters therefore needs *two* header
rows, `header: [{ text }, { filter: "text" }]`. `HeaderFooter.jsx` puts the sort affordance and its
indicator on the last non-filter row, so the text row sorts and the filter row below it filters.

Alternative considered: let SVAR sort and keep a separate pristine copy of the rows to restore from.
Rejected as two sources of row order that must be kept in step across Load more, filtering and
resource switches, for no gain over deriving one array from one.

### Selection stays entirely ours

SVAR ships no checkbox column: selection is a `selectedRows` list plus a `select-row` action, and a
checkbox is a custom cell the application renders. That is a benefit here, not a gap. We render the
same checkbox we render today, in a custom cell, driven by the same `selected: SelectedRow[]` prop and
the same cap predicate, and we do **not** adopt the grid's selection state at all.

This is the specific failure mode being avoided: the browse table's selection spans resources, carries
a snapshot label, and is capped, so any grid-owned selection would need two-way syncing and would
diverge on the cap, on resource switches, and on the visible/hidden split.

**Not adopting it is not enough: it must be switched off.** `select` defaults to **`true`** in `Grid.jsx`,
and `Layout.jsx`'s body click handler dispatches `select-row` for a click on any cell, guarded only by
`if (ev.target.closest("input")) return;`. So a click on the checkbox is ignored, but a click anywhere
else in the row (a plain cell, the name link's surrounding cell, the Drill in cell) enters SVAR's own
selection, applies its `wx-selected` styling, and diverges from ours immediately. The task-1.3 spike
clicked only checkboxes and therefore proved nothing about this; it reported `selectedRows` empty for
that reason alone.

The grid is therefore configured with **`select={false}`**, which makes that handler return before
dispatching. Tests must cover the interactions the spike missed: a click on an ordinary cell, on the
name link, and on the Drill in button, plus keyboard activation, each asserting that `selectedRows` in
grid state stays empty, that no row carries the `wx-selected` class, and that our own `SelectedRow[]`
is unchanged except where our checkbox was the thing activated.

### The grid owns a bounded scroll viewport, and windows rows inside it

Two assumptions in the first draft of this design were tested against the shipped package during
implementation (tasks 1.2 and 1.3) and both were wrong. What follows is what the code does.

**Row windowing is unconditional.** `dynamic` being unset does not disable it. `Layout.jsx` computes
`dataRows = dynamic ? data : data.slice(renderRows.start, renderRows.end)`, where
`end = min(count, start + visibleRows + 2)` and `visibleRows` derives from `clientHeight`, measured by
a `ResizeObserver` on the grid's parent. Under jsdom there is no layout, so `clientHeight` is 0 and the
window collapses: measured, 50 rows supplied and **3 rendered**.

The consequence is confined to tests, and the fix is test-only. `ui/src/setupTests.ts` already stubs
`ResizeObserver` as a no-op that never invokes its callback; that stub is replaced with one that
delivers a `contentRect`, which is what a real browser does. With it, all 50 rows render and every row
is queryable. No production code depends on this. Alternatives were measured and rejected:
defining `clientHeight` on `HTMLElement.prototype` does not work (the value the grid uses comes from
the observer's `contentRect`, not from reading the node), and a CSS height on a wrapper does not work
either (jsdom performs no layout).

**The production height contract, and why the shim must not stand in for it.** A shim that reports a
large height makes every test pass whether or not the real wrapper has a bounded height, and a real
wrapper without one collapses to nothing, because `.wx-grid { height: 100% }` of an auto-height parent
is zero. That is the one failure the shim would hide, so it is pinned separately rather than trusted:

- The grid is wrapped in an element carrying a single named class, `connector-grid-viewport`, whose
  bounded height is declared once in `theme.css` (a viewport-relative height with a `min-height` floor,
  so the region is usable on a laptop and on a phone).
- A test asserts that the rendered wrapper carries that class, and a second asserts that `theme.css`
  declares a height for it. Deleting the height, or the class, fails a test rather than silently
  producing an empty grid that every other test still passes.
- The shim reports its height from that same declared value rather than an arbitrary large number, so
  the tests exercise the geometry the application actually ships.
- At least one test overrides the shim to report a realistically small viewport and asserts that
  windowing genuinely engages: fewer rows in the DOM than were loaded. Without it, no test would cover
  the windowed path at all, and a change that broke scrolling behavior would go unnoticed.

Note that this makes "every loaded row is in the DOM" a property of the *test* configuration, not a
guarantee of the component. `design.md`'s goals are written accordingly: tests query rows freely because
the shim gives them a tall viewport, while the shipped grid windows.

**The grid is a bounded-height scroll viewport.** Its own stylesheet says so: `.wx-grid { height: 100% }`,
`.wx-table-box { height: 100%; overflow: hidden }`, `.wx-scroll { flex: 1 }`, and `.wx-body` carries an
explicit pixel height. Given a parent of auto height, `height: 100%` collapses and nothing renders, so
the grid must be given a bounded height and it scrolls internally.

This retires the page-flow goal rather than working around it. A height-feedback loop (observing the
grid's computed `fullHeight` and setting the wrapper to match, so the inner pane never scrolls) was
considered and rejected: it is a resize loop we would own and debug, against a library actively
computing the same number, and the proposal's premise is that this table's behavior is configured
rather than written. The browse table therefore gets a bounded scroll region. Rows still grow to fit
wrapped descriptions inside it, which is the capability that actually decided the grid choice.

Windowing is a browser-side benefit, not a risk, and it is what makes the loaded-row count safe to grow:
repeated Load more actions accumulate rows without bound, since the 200-row materialize cap limits how
many rows may be *selected*, not how many may be loaded. The round-1 draft cited that cap as a bound on
loaded rows; it is not one.

### Theming maps SVAR's variables onto the app's tokens, without the vendor's theme component

SVAR styles through 41 `--wx-*` variables, normally supplied by a `<Willow>` / `<WillowDark>` wrapper.
**That wrapper is not used here.** `Willow` defaults to `fonts=true` and injects `<link>` tags to
`https://cdn.svar.dev` for Open Sans and an icon font. Labeler is a self-hosted service whose assets are
served from `ui/dist` over a LAN, so a per-page-load fetch to a vendor CDN is unacceptable regardless of
whether it succeeds.

Instead, `ui/src/theme.css` gains a mapping block binding the `--wx-*` variables the grid uses to the
app's existing `--surface`, `--ink`, `--border`, `--muted` and `--accent` tokens, scoped to the grid
container. The grid's stylesheet is imported once, alongside the component. Nothing is fetched remotely.

Two consequences follow:

- **The sort indicator needs its own glyph.** SVAR draws it as `<i class="wxi-arrow-up">`, whose glyph
  lives in the CDN icon font. Without that font the element renders empty. The mapping block therefore
  supplies the two arrows itself. `aria-sort` is emitted by the grid regardless, so assistive technology
  is unaffected either way, but the spec requires the indicator visually *and* to assistive technology.
- **Skin detection still resolves.** `suggestSkin()` looks for a `[class^="wx"][class$="theme"]` element
  and falls back to `"willow"` when there is none, which is the value we want. It affects only the CSS
  class used for off-screen text measurement.

This also settles the open question below. The app sets `.dark` on `<html>` from a blocking inline
script in `index.html`, before React mounts, so a mapping expressed in CSS and keyed off that class is
correct at first paint. A React wrapper swapped on mount is what would flash; not rendering one avoids
the problem rather than mitigating it.

### Filtering is ours too, through a custom header cell rather than the built-in filter

The same ownership boundary drawn for sorting applies to filtering, and for the same reason: SVAR's
`filter-rows` action maintains its own `filterValues` and computes `_filterIds`, which feeds `flatData`
and therefore what renders. Handing the grid a filtered array *and* letting it filter internally would
apply the predicate twice, against two different notions of which rows exist.

Rather than dispatch `filter-rows` and then intercept it, the filter row uses a **custom header cell**,
declared with **both** keys: `header: [{ text }, { cell: OurFilterCell, filter: "text" }]`. Both are
load-bearing, and `HeaderCell.jsx` reads them on different lines:

- `cell.cell` decides *what renders*: line 285 prefers it over the built-in `<Filter>` on line 298. Our
  cell renders our own input, bound to our own React state, and never calls `api.exec("filter-rows")`.
- `cell.filter` decides *how the cell behaves as a header*: lines 61, 88, 255 and 261 all key on it to
  suppress the sort click, suppress Enter-to-sort, drop the cell out of the tab order, and report
  `aria-sort="none"`.

Declaring `cell` alone was the round-2 draft's mistake, and the reviewer caught it: without `filter`,
the cell stays sortable, so a click that bubbled out of the filter input would sort the column, and the
cell would advertise the active sort direction from a row that is not the labelled one. Declaring
`filter` alone would render SVAR's unlabeled input and dispatch `filter-rows`. Both together give a
non-sortable, correctly-announced header row containing an input we fully own.

The grid consequently has no filter state at all: `filterValues` stays empty because nothing ever
dispatches `filter-rows`, so `createFilter` is never invoked and `_filterIds` is never computed. There
is nothing to intercept and nothing to keep in step. Clearing a filter, hiding a column, and appending a
page through Load more are each a change to one piece of React state, re-deriving the displayed array
through the same filter-then-compare pipeline; none of them can leave a stale grid-side filter behind.

This also fixes an accessibility defect that the built-in filter cannot: SVAR's filter input is
`@svar-ui/react-core`'s `Text`, which accepts `id`, `placeholder` and `title` but **no `aria-label`**, so
every built-in filter would ship as an unlabeled input in a header. Our cell gives each filter an
explicit accessible name naming its column, e.g. `aria-label="Filter by Name"`.

**Two header rows, and what they mean to assistive technology.** Because the filter cell declares
`filter` (see above), it is not sortable, and each column has two `role="columnheader"` cells: the text
row, which holds the sort control and reports the true `aria-sort`, and the filter row below it, which
`HeaderCell.jsx:261` then reports as `aria-sort="none"`. That second value is not configurable through
the grid's API, and it is a consequence of declaring `filter`, not a property of custom cells in
general. It is
accepted as a known limitation rather than papered over, and it is bounded by a test: for a sorted
column, exactly **one** of its header cells reports a non-`none` `aria-sort`, and it is the one carrying
the column's label. A regression that moved the sort state onto the filter row, or duplicated it across
both, fails that test.

**The sort arrow is ours.** SVAR draws it as `<i class="wxi-arrow-up">`, whose glyph lives in the CDN
icon font this design does not load. The mapping block in `theme.css` therefore gives `.wxi-arrow-up`
and `.wxi-arrow-down`, scoped to the grid container, their own content via a CSS `::before` using a
plain Unicode triangle (`\25B2` / `\25BC`) so nothing is fetched and nothing depends on a webfont. The
test asserts the rendered direction, not merely that a sort mark exists.

**Disclosure copy is specified, not left to taste.** The three states are materially different and are
fixed here so the tests and the UI cannot drift:

- filter active: `Showing {shown} of {loaded} loaded rows`
- more rows available: `Sorting and filtering cover only the {loaded} rows loaded so far`
- filter matches nothing while more remain: `No loaded row matches. More rows can be loaded.`

They render in one container immediately above the grid, marked `role="status"` so a change is announced
politely rather than interrupting, and so a screen-reader user learns the result of typing a filter
without having to hunt for it.

### View-control state is transient and keyed to the browsing context

Sort and filters are component state, cleared by the same transitions that already reset `applied`,
`filterDraft` and `tags`: resource switch, drill-in, and parent clear. Nothing is written to
`localStorage`. Column visibility keeps its existing persistence in `connectorColumns.ts`, untouched.

## Risks / Trade-offs

- **The React package is young: 2.7.3, repo created 2025-10-13, 91 stars, 74k weekly downloads** →
  Pin the exact version. The licence is MIT across all eleven `@svar-ui/*` packages, so a fork is
  available if it stalls. The ordering and matching rules live in our modules, so replacing the grid
  later is a re-render, not a re-specification. `LabelGrid` is untouched, so a failure here is confined
  to one screen.
- **Two grid libraries in one app** → Accepted deliberately. The alternative is either migrating the
  editable grid to a component not chosen for editing, or keeping the browse table on a component that
  cannot render it. Both tables are named and small; the cost is one extra dependency family, not a
  divided abstraction.
- **Per-row disabled selection is undocumented in SVAR** → Neutralised by rendering our own checkbox
  cell rather than using any grid selection feature. The first implementation task proves it against
  the cap before the rest is built.
- **SVAR's stylesheet may leak into or clash with Tailwind and the app's tokens** → Scope the mapping
  to the grid container and verify both themes visually before the change is called done.
- **The shipped types disagree with the shipped runtime** → A custom cell receives its action callback
  as `onAction`, while `ICellProps` declares `onaction`. Confirmed by runtime probe, not by reading the
  types. Taken as evidence for the "young package" risk above rather than as an isolated defect: where
  this design cites package behavior, it cites code or a measurement, not the vendor's documentation.
- **Two design assumptions were falsified during implementation** (jsdom row visibility, page-flow
  layout) → Both are recorded above with what replaced them. The grid choice survives because it turned
  on wrapped content-sized rows, which no other MIT candidate offered and which SVAR does deliver; it
  did not turn on either falsified assumption.
- **The five existing `ConnectorBrowser.test.tsx` cases query the current table's DOM and will break**
  → They are rewritten as part of this change, not deleted; each keeps asserting the behavior it
  asserts today (selection, label snapshot, summary, server-side filter apply, tag chips).
- **A sort over loaded rows can read as a sort over the whole inventory** → Addressed in the spec as a
  disclosure requirement rather than left to implementation taste.

## Migration Plan

None required. This is a client-side component change with no data, schema, API or configuration
migration. Rollback is reverting the commit; a user who never touches a header sees the connector's
order exactly as today.

## Open Questions

None outstanding. The one question this design carried, whether the SVAR theme wrapper could follow the
app's theme toggle without a first-paint flash, is answered under "Theming" above: the wrapper is not
used at all, the mapping is expressed purely in CSS keyed off the `.dark` class that `index.html` sets
before React mounts, and there is therefore nothing to flash.
