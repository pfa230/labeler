## 1. Prove the two load-bearing assumptions before building on them

- [x] 1.1 Add `@svar-ui/react-grid` to `ui/package.json` at an exact pinned version (no `^`), run `npm install`, and confirm `npm run build` succeeds and `npm run lint` is clean with the new dependency present.
- [x] 1.2 Spike a throwaway render of the grid inside `ConnectorBrowser` with `autoRowHeight` on and `dynamic` left at its default. **Outcome: both halves failed as written, and the plan changed rather than the finding.** jsdom saw 3 of 50 rows because windowing is unconditional; fixed test-only by a `ResizeObserver` reporting a real `contentRect` (`setupTests.ts`), pinned by `connectorGridWindowing.test.tsx`. Page-flow scrolling is impossible (`.wx-grid { height: 100% }` over an inner `.wx-scroll` pane); the page-flow goal was withdrawn by decision and the grid now lives in a bounded `.connector-grid-viewport`. See ADR-0064 Consequences and design.md "The grid owns a bounded scroll viewport". Remaining to confirm in a browser: rows grow to fit a wrapped long Homebox description. Closes with 7.3.
- [x] 1.3 Spike a custom checkbox cell driven by the existing `selected` prop and cap predicate, and confirm an unselected row's checkbox is disabled once the selection reaches the 200-row cap, with no grid-owned selection state involved. Delete the spikes once all three are confirmed.

## 2. The ordering and matching rules, as pure modules

- [x] 2.1 Write `ui/src/lib/connectorSort.ts`: a comparator factory taking a `FieldSpec` and a direction, with text/badge compared case-insensitively, number/money numerically, date chronologically over ISO-8601, absent-or-uninterpretable cells ordered last in both directions, and ties preserving input order.
- [x] 2.2 Write `ui/src/lib/connectorSort.test.ts` covering each declared `FieldType`, both directions, blanks, non-numeric values in a number/money column, unparsable values in a date column, and tie stability. Each test must fail against a deliberately wrong comparator before it passes.
- [x] 2.3 Write `ui/src/lib/connectorFilter.ts`: a per-column predicate matching case-insensitively anywhere within the cell as displayed, with multiple column filters combined by AND and an empty filter matching everything.
- [x] 2.4 Write `ui/src/lib/connectorFilter.test.ts` covering case-insensitivity, substring position, AND across columns, empty filters, absent cells, and a value that is unsortable but still findable.

## 3. Replace the table with the grid

- [x] 3.1 Replace the hand-written `<table>` in `ConnectorBrowser.tsx` with the SVAR grid, keeping `rows` in connector order and passing a derived array produced by the filter then the comparator.
- [x] 3.2 Port the three cell renderers: the name cell linking to `row.url` when present, the Drill in cell shown only where the resource has a relationship, and the selection checkbox from task 1.3 driven by the existing `selected` prop, cap and handlers.
- [x] 3.3 Wire header sorting: intercept `sort-rows` with `add` forced falsy so Ctrl/Meta-click cannot enter multi-column sorting, cycle our own three-state sort for the activated column, and drive `sortMarks` from that state so the indicator and `aria-sort` render, with the third state clearing it.
- [x] 3.4 Wire per-column header filters (a custom header cell declaring **both** `cell` and `filter`; `header.filter` alone would render SVAR's unlabeled input and dispatch `filter-rows`, `cell` alone would leave the filter row sortable) for every visible column, applying live with no confirmation step, and clear a column's filter when that column is hidden through the Columns picker.
- [x] 3.5 Clear the active sort and all column filters on resource switch, drill-in and parent clear, alongside the existing resets of `applied`, `filterDraft` and `tags`. Persist neither.
- [x] 3.6 Add the scope disclosure: how many of the loaded rows are shown while a filter is active, a statement that loaded rows are not the whole result while the connection reports more available, and a distinct message when a filter matches nothing while more rows can be loaded.
- [x] 3.7 Confirm the untouched surfaces still work: the Columns picker and its `localStorage` persistence, the server-side filter bar and tag chips, Load more, and the selection summary's visible/hidden split.

## 4. Theming

- [x] 4.1 Map the `--wx-*` variables the grid uses onto the app's `--surface`, `--ink`, `--border`, `--muted` and `--accent` tokens in `ui/src/theme.css`, scoped to the grid container so the stylesheet cannot leak into the rest of the UI.
- [x] 4.2 Drive the theme wrapper from the app's existing theme state rather than the OS setting, and resolve `design.md`'s open question about a first-paint flash of the wrong theme. Check both light and dark in a browser.

## 5. Tests

- [x] 5.1 Rewrite the five existing `ConnectorBrowser.test.tsx` cases against the grid's DOM, each still asserting what it asserts today: row load and selection toggle, the label snapshot on select, the visible/hidden summary, the server-side search filter on Apply, and tag chip handling with auto-commit.
- [x] 5.2 Add sorting tests: the three-state cycle including the return to connector order, a numeric column ordering numerically rather than as text, blanks last in both directions, a page appended by Load more joining the current order, sorting a second column releasing the first, and a Ctrl/Meta-click behaving as a plain click.
- [x] 5.3 Add filtering tests: narrowing and restoring on clear, two column filters combining with AND, no browse request issued and the cursor unchanged while filtering, and a hidden column's filter no longer restricting the table.
- [x] 5.4 Add tests for the preserved contract under the view controls: a filtered-out row staying selected and still listed in the summary, the visible/hidden split unchanged by filtering, and the cap still disabling unselected rows while a sort is active.
- [x] 5.5 Add tests for the disclosure strings: the filtered count, the more-rows-available statement, and the no-match-with-more-available message.

## 6. Decision record

- [x] 6.1 Write `docs/adr/0064-svar-grid-for-the-connector-browser.md` covering why the browse table needs a grid the editable one cannot provide, the candidate comparison and each rejection, why sorting stays ours while the grid renders only the indicator, and the consequence of carrying two grid libraries. Confirm 0064 is still free against `main` before writing.
- [x] 6.2 Add the ADR's row to `docs/adr/README.md`.

## 7. Gates

- [x] 7.1 Run `npm --prefix ui run lint`, `npm --prefix ui run test` and `npm --prefix ui run build`; all clean, with no lint suppression added.
- [x] 7.2 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test` to confirm the backend is untouched and still green.
- [x] 7.3 Load the Connect page against a real Homebox connection and check the result by eye at 50+ rows: sort each column type, filter two columns at once, confirm long descriptions wrap and rows grow, confirm the bounded grid region scrolls internally and is usable at that height (the page-flow goal was withdrawn; see ADR-0064), and confirm selection survives sorting and filtering. A screen that renders without error is not a screen that is correct.
- [x] 7.4 Measure the built bundle with `npm --prefix ui run build` and record the gzipped delta against the 132.8 KB baseline in the ADR's consequences.
