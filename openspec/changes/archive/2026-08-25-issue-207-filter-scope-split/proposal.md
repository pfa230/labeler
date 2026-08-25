## Why

Implements [#207](https://github.com/pfa230/labeler/issues/207).

The Connector Browser stacks two filter systems that answer what reads as one question. The
connector filter row (`ConnectorBrowser.tsx:482-535`) sends filters upstream and restricts the whole
result set; the column header row added by #170 (`:420`, `ui/src/lib/connectorFilter.ts`) matches
substrings client-side over the rows already fetched. For the Homebox items resource they collide on
the same fields: "Search" sits above Name and Description boxes, "Location" sits above a `location`
column with its own box. Nothing on screen says the two are different questions, `Clear filters`
clears only one of them, and the single caption that explains scope sits between the two rows where
it reads as covering both.

## What Changes

The two scopes stay, and stop being ambiguous. Chosen over folding the connector filters into the
column header row: a connector filter is not a column (`q` spans fields and has none, `tag` has none)
and `parent` takes a location **id** while the `location` column renders a **name**, so folding needs
a filter-to-column mapping in `ResourceSpec` plus id resolution, which is #168's work.

- The connector filter row becomes a **named group** stating that it queries the source system and
  restricts the whole result, not only what is loaded.
- The column header boxes become a **named group** stating that they narrow the rows already loaded,
  as you type.
- The **scope disclosure moves** out from between the two rows to the grid it describes, and its
  wording names the loaded-row controls rather than "filtering" in general, so it can no longer be
  read as a caveat on the source filters.
- **One control clears both groups**, and says so. Today `Clear filters` leaves every column box
  populated, so the grid stays narrowed after the user believes they cleared the filters.
- **Chaining is stated, not left invisible**: a column box narrows what the source filters already
  returned. A field carrying both (Location) is two boxes asking two different questions, and each
  now says which.
- Both groups render only when they exist: a resource declaring no filters (Homebox `locations`)
  shows no source group and no promise of one.

No connector, API or schema change. `FilterSpec`, `ResourceSpec` and the browse contract are
untouched; this is UI copy, grouping, placement, and the reach of one button.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `connector-browser`: adds the contract for how the two filter scopes are presented and cleared, and
  changes what the scope disclosure says and where it sits.

## Impact

- `ui/src/pages/connect/ConnectorBrowser.tsx`: filter row markup and labels, disclosure placement and
  copy, `handleClearFilters` reach, the clear control's visibility condition.
- `ui/src/pages/connect/connectorBrowserFiltering.test.tsx`,
  `connectorBrowserDisclosure.test.tsx`, `ConnectorBrowser.test.tsx`: assertions on the moved and
  reworded copy and on clearing both groups.
- `docs/adr/0072-*.md` and `docs/adr/README.md`: the decision to keep two scopes and name them.
- Sequencing: #168 adds autocomplete affordances to the connector row, so it lands after this change.
- No Rust change. `src/connector/homebox.rs` filter declarations stay as they are.

**ADR numbering.** Main's highest is 0071; in-flight worktrees `issue-197` and `issue-200` both claim
0070, and 0067 is unused. This change takes 0072 and may need renumbering if another change merges
first.
