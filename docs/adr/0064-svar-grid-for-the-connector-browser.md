# 64. The connector browse table uses SVAR DataGrid; the ordering rules stay ours

Date: 2026-08-22

## Status

Accepted. Issue [#170](https://github.com/pfa230/labeler/issues/170).

## Context

The Connect page's browse table was hand-written `<table>` JSX. It rendered rows in whatever order the
connector returned and offered no way to reorder or narrow them. A Homebox connection routinely loads
50-200 items, so finding one, grouping by location, or reading the cheapest first meant scrolling and
squinting. The connection's server-side filter bar re-queries Homebox and resets the cursor, which is
the wrong instrument for "show me the loaded rows sorted by quantity".

Adding sorting and per-column filtering by hand means owning the header semantics that assistive
technology depends on, the filter inputs and their labels, and the indicator states. That is the work
worth buying from a maintained component rather than writing and then maintaining.

Two constraints shaped the choice. `ui/src/components/LabelGrid.tsx` already uses `react-data-grid` for
the editable label grid, so a second grid either joins it or sits beside it. And a Homebox description
is long: the table has to show wrapped, content-sized rows rather than one ellipsised line.

## Decision

**The browse table uses SVAR React DataGrid (`@svar-ui/react-grid` 2.7.3, MIT), pinned exactly.**

The deciding capability is **automatic measurement of wrapped content-sized rows**: `autoRowHeight`
plus `.wx-row.wx-autoheight { height: max-content }` with `white-space: normal` cells. No other MIT
candidate provides it. `react-data-grid`, the incumbent, does accept a per-row `rowHeight` callback and
its nowrap CSS can be overridden, so variable heights are reachable there; what it will not do is
*measure* how tall a wrapped description turns out to be, which would leave us computing every row's height
from text length and column width on every render and resize.

**The ordering and matching rules are ours, not the grid's.** `ui/src/lib/connectorSort.ts` holds a
comparator selected by the column's declared `FieldType`; `ui/src/lib/connectorFilter.ts` holds the
per-column predicate. They are pure and unit-tested without rendering anything. This is deliberate on
three grounds: the rules encode connector semantics rather than grid mechanics; they are the part worth
testing exhaustively, which is far cheaper as pure functions; and they let the specified behavior
survive a later change of grid.

**The grid renders sort state; it does not own the ordering.** Verified against the shipped store rather
than its documentation: `sortMarks` holds only `asc`/`desc`, with no third state; `add` reaches
multi-column sorting on Ctrl/Meta-click; and sorting replaces the store's `data` with a sorted clone, so
the connector's order is not retained inside the grid. `ConnectorBrowser` therefore keeps `rows` in
connector order, intercepts `sort-rows` with `add` forced falsy, cycles its own three-state sort, and
drives `sortMarks` purely so the indicator and `aria-sort` render, clearing it for the third state.

**Selection stays entirely ours, and the grid's is switched off.** SVAR ships no checkbox column, which
suits a selection that spans resources, snapshots a label, and caps at 200. Critically, `select`
defaults to `true` and a click on any non-`input` cell dispatches `select-row`, so the grid is
configured with `select={false}`. Not adopting its selection is not the same as disabling it.

**Filtering uses a custom header cell declaring both `cell` and `filter`.** `cell` decides what renders,
so our own labelled input replaces SVAR's (whose `Text` component accepts no `aria-label`); `filter`
decides how the cell behaves as a header, suppressing sort-on-click and reporting `aria-sort="none"`.
Neither key alone is correct. The grid consequently never dispatches `filter-rows` and holds no filter
state, so there is nothing to keep in step.

**The vendor theme components are not used.** `<Willow>`/`<WillowDark>` inject `<link>` tags to
`https://cdn.svar.dev` for fonts and an icon font. Labeler is self-hosted and serves its assets from
`ui/dist`, so the `--wx-*` variables are mapped onto the app's own tokens in `theme.css`, scoped to the
grid container, and the sort arrows are supplied as local Unicode glyphs.

## Consequences

**The browse list becomes a bounded scroll region.** This is the cost that was not anticipated when the
grid was chosen. SVAR's root is a fixed-height scroll viewport (`.wx-grid { height: 100% }`,
`.wx-table-box { height: 100%; overflow: hidden }`, `.wx-scroll { flex: 1 }`), so the list scrolls
inside its own region rather than extending the page, and the wrapper must carry an explicit bounded
height or the grid collapses to nothing. An earlier draft of the design listed page-flow layout as a
goal and rejected `react-data-grid` partly for having a fixed-height viewport; that reason does not
distinguish the two libraries and has been withdrawn. Keeping page flow would have required a
height-feedback loop against a library computing the same number, which was judged worse than accepting
the region.

**Rows are windowed, unconditionally.** `dynamic` being unset does not disable it: the rendered slice is
derived from `clientHeight`. In the browser this is a benefit and is what makes an unbounded loaded-row
count safe, since the 200-row cap limits selection rather than loading. Under jsdom, where there is no
layout, it meant 3 of 50 rows rendered, so `setupTests.ts` now supplies a `ResizeObserver` that reports
a real `contentRect`. Because that shim would also hide a missing wrapper height, the height is pinned
by its own test rather than trusted.

**Two grid libraries in one app.** Accepted deliberately. The alternative is migrating the editable grid
to a component not chosen for editing, or keeping the browse table on one that cannot render it. Both
tables are named and small; the cost is one dependency family, not a divided abstraction.

**A young package, and types that disagree with its runtime.** 2.7.3, repo created 2025-10-13. A custom
cell receives `onAction` while the shipped types declare `onaction`. The licence is MIT across all
eleven `@svar-ui/*` packages, so a fork is available; the rules living in our modules mean replacing the
grid later is a re-render, not a re-specification; and `LabelGrid` is untouched, so a failure here is
confined to one screen. Every claim about package behavior in this record was verified against the
shipped code, not its documentation, because two documentation-derived claims proved false during
implementation.

**AG Grid remains the first alternative to revisit.** Its `domLayout='autoHeight'` is a documented
page-flow mode and its row selection is opt-in, so the earlier rejection on "duplicate selection state"
was wrong. It was rejected here on breadth and on docs that mix Enterprise features into Community
pages, which is a preference about fit, not a forced hand.

**Bundle.** Measured, not estimated: the baseline with the dependency present but unimported was
133.00 KB gzipped JS and 5.47 KB gzipped CSS; with the grid in use it is 169.49 KB and 8.43 KB, a delta
of **+36.5 KB JS and +3.0 KB CSS gzipped**. That is well under the 83 KB JS / 16 KB CSS the untree-shaken
estimate in the proposal predicted, because the vendor's toolbar, comments, tasklist and editor entry
points drop out. Assets are served over a LAN and cached, so size was deliberately not a selection
criterion; the number is recorded because the proposal quoted one.
