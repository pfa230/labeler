## 1. Pin the current behaviour, then break it

- [x] 1.1 Read `connectorBrowserFiltering.test.tsx`, `connectorBrowserDisclosure.test.tsx` and
  `ConnectorBrowser.test.tsx` and list every assertion that names `Clear filters`, the disclosure
  copy, or the position of either filter row. That list is the change surface for group 5.
- [x] 1.2 Write the new assertions before the markup: source-group legend and scope sentence present;
  refine-group caption and scope sentence present; the accessible description of the Search input, of
  the tag input, and of one column filter box; `Clear all filters` clearing both kinds; the disclosure
  rendered after the grid. Run them and record that each one fails for the right reason — a test that
  passes against the current tree is testing nothing.

## 2. The source filter group

- [x] 2.1 Wrap the connector filter controls in a `<fieldset>` with `<legend>` "Source filters" and a
  description paragraph carrying an id, styled from the existing tokens (Tailwind preflight strips
  `fieldset`/`legend` defaults).
- [x] 2.2 Put `aria-describedby` pointing at that id on **every** input in the group, the tag input's
  own branch (`ConnectorBrowser.tsx:486-516`) included, not on the `fieldset`.
- [x] 2.3 Confirm the group and its description render only when `resource.filters.length > 0`, so the
  Homebox `locations` resource shows neither.

## 3. The refine group

- [x] 3.1 Add the caption above the grid: "Refine loaded rows" plus its scope sentence, the sentence
  carrying an id.
- [x] 3.2 Pass that id into `FilterCell` as a constant and set `aria-describedby` on each filter input.
  Keep it out of the `useMemo` deps that hold `FilterCell`'s identity stable, or the input remounts per
  keystroke and drops focus (`:382-403`).

## 4. Move the scope disclosure

- [x] 4.1 Render the disclosure block below the grid, in the row that holds `Load more`, and reword
  "Sorting and filtering" to "Sorting and refining" so it names the group by the caption's word.
- [x] 4.2 Verify the `hasMore` line still shows while a source filter is applied: a source filter
  narrows the result but still pages it.

## 5. One clear control

- [x] 5.1 Add `setColumnFilters({})` to `handleClearFilters`.
- [x] 5.2 Move the button to the utility cluster beside `Columns (n/m)`, rename it `Clear all filters`,
  and widen its visibility condition to include any non-empty entry in `columnFilters`. Leave `Apply`
  inside the source group.
- [x] 5.3 Update the assertions listed in 1.1 to the new copy and reach.

## 6. Record the decision

- [x] 6.1 Write `docs/adr/0072-two-filter-scopes-named-not-merged.md`: two scopes kept and named rather
  than folded, and why folding needs a filter-to-column mapping the schema does not have.
- [x] 6.2 Add its row to `docs/adr/README.md`. Re-check the number against `main` first — `issue-197`
  and `issue-200` both already claim 0070.

## 7. Gates and the visual check

- [x] 7.1 `cd ui && npm run lint && npm run build && npm test` all green.
- [x] 7.2 `cargo fmt`, `cargo clippy --all-targets --all-features`, `cargo test` all green.
- [x] 7.3 Run the app and **look at it**: the two groups read as two groups, the disclosure reads as
  belonging to the grid, the fieldset border does not read as a second card, and nothing wraps badly
  at a narrow width. Served the built UI from the service itself (`localhost:8080/connect`), not the
  vite proxy: `changeOrigin: true` makes Host the backend while Origin stays the dev port, and
  `middleware.rs:178` rejects that on every POST. Backed by a stub Homebox on the LAN address, since
  production egress blocks loopback (`egress.rs:34`), carrying 70 items so `has_more` is true and the
  moved disclosure renders.
- [x] 7.4 Screenshots of the browse table in both themes, source filter applied and column filter
  typed, on the Items resource and on filterless Locations. Posted to #207 with the findings
  (comment 5411768673); the images themselves cannot be uploaded through the GitHub API and were
  handed over directly. One finding, accepted as cosmetic: `.connector-grid-viewport` is a fixed
  `60vh` (`ui/src/theme.css:46-47`), so a short loaded set leaves the disclosure well below the last
  row it counts.
