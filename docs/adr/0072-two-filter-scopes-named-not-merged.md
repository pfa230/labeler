# 72. Two filter scopes named, not merged

Date: 2026-08-25

## Status

Accepted. Issue [#207](https://github.com/pfa230/labeler/issues/207).

## Context

The Connector Browser offers two filter surfaces that answer what appears to the user as a single question:
1. Upstream connector filters (`ConnectorBrowser.tsx`), which query the remote system and restrict the entire result set upon activating `Apply`.
2. Client-side per-column filter inputs (added by #170 to the SVAR DataGrid introduced in [ADR-0064](0064-svar-grid-for-the-connector-browser.md)), which perform substring matching live over the rows already fetched.

For resources like Homebox items, the two filter systems collided on identical or related fields:
- "Search" (`q`) sat above the "Name" and "Description" column filter inputs.
- "Location" (`parent`) sat above the "Location" column filter input, where the former accepts a location ID to query the connection while the latter matches substring names client-side.

Several problems resulted:
- Nothing on screen stated that the two filter rows operated on different scopes.
- The single scope disclosure paragraph explaining loaded-row reach sat between the two filter rows, where it was easily misread as qualifying both.
- `Clear filters` cleared only the upstream connector filters, leaving the column filter inputs populated and the table unexpectedly narrowed.

## Decision

1. **Keep two distinct filter scopes and name their reach explicitly.** Rather than folding connector filters into the column header row, the two groups remain separated:
   - **Source filters group:** Wrapped in a `<fieldset>` with a `<legend>` ("Source filters") and a descriptive statement ("Queries the connection and restricts the whole result. Takes effect on Apply."). Every input in the group (including tag filters) is associated with this statement via `aria-describedby`.
   - **Refine loaded rows group:** Headed by a clear caption ("Refine loaded rows") with a descriptive statement ("Narrow the rows already loaded, as you type."). Every column filter input carries `aria-describedby` pointing at this statement.
   - For filterless resources (such as Homebox `locations`), the source filter fieldset and its description are omitted entirely.

2. **Move the scope disclosure below the grid.**
   The disclosure block is moved from between the filter rows to below the grid, adjacent to `Load more`. Its wording is reworded from "Sorting and filtering" to "Sorting and refining" ("Sorting and refining cover only the N rows loaded so far") to align with the "Refine loaded rows" caption and prevent any interpretation as a caveat on upstream source filters.

3. **Provide a unified "Clear all filters" control.**
   `handleClearFilters` clears both the upstream filter state (drafts, tags, applied filters) and client-side `columnFilters`. The button is renamed to "Clear all filters" and relocated to the table utility cluster beside the `Columns (n/m)` picker. Its visibility condition is widened so that it appears whenever any filter (source or column) is populated or applied.

4. **Why folding was rejected:**
   Merging the upstream connector filters into the column header row would require:
   - A declarative filter-to-column mapping in `ResourceSpec`.
   - Complex resolution for cross-column search (`q` queries multiple fields upstream without corresponding to a single column).
   - ID-to-name resolution (e.g. mapping location names in cells to location IDs expected by the connector's `parent` filter).
   These capabilities require dedicated autocomplete affordances (#168), making explicit naming and clear separation the correct architectural choice for now.

## Consequences

- The distinction between server-side filtering and client-side refinement is visually and programmatically clear to all users and assistive technology.
- Activating "Clear all filters" reliably resets the table to its full, unfiltered state.
- Chaining behavior (source filters restricting fetched rows, column filters narrowing loaded rows) is preserved and transparent.
