## Context

See `proposal.md` — Why. What matters for the approach is where each control lives in the DOM,
because that constrains how its scope can be stated.

The connector filters are ours: plain inputs in `ConnectorBrowser.tsx:482-535`, above the grid,
committed by `handleApply` into `applied`, which is a dependency of the browse effect
(`:206-232`). The column filter boxes are not: they render inside SVAR's header, one per column, via
the `FilterCell` component threaded through `IColumnConfig.header[1]` (`:380-425`). There is no
element wrapping that row that we own, and no place to hang a row-level label inside the grid.

`ConnectorBrowser` knows the `connectionId` and the `ConnectorSchema`, neither of which carries a
human name for the source system. Copy therefore says "the connection", not "Homebox".

The scope disclosure exists today at `:637-651`, rendered between the connector filter row and the
grid, and says "Sorting and filtering cover only the N rows loaded so far".

## Goals / Non-Goals

**Goals:**

- Each group of controls states its own reach, in a way a screen reader also reaches.
- One clear control whose name matches everything it clears.
- Positions that carry meaning: nothing sits where it could be read as belonging to the other group.

**Non-Goals:**

- Any change to `FilterSpec`, `ResourceSpec`, `browse`, or `src/connector/`. No filter-to-column
  mapping, which is what folding would need and what #168 will bring the affordances for.
- Making the source group say which fields `q` searches. The schema does not carry that, and adding
  it is connector work outside this issue.
- Changing what either filter kind matches. `q` stays whatever the upstream search does;
  a column box stays a case-insensitive substring of the rendered cell.

## Decisions

### The source group is a `fieldset`; the column group is a caption plus `aria-describedby`

The connector filters get a `<fieldset>` with a `<legend>` ("Source filters") and a description
paragraph carrying an id. The legend gives the group a name that is announced with each control
inside it; the description is associated by putting `aria-describedby` on **each input in the group**,
the tag input included, because `aria-describedby` on the `fieldset` does not become the accessible
description of the controls within it. The legend names the group, the per-input reference states its
reach, and neither substitutes for the other.

The column boxes cannot be wrapped: they are SVAR's header cells. So the group is stated by a caption
line immediately above the grid ("Refine loaded rows" plus its scope sentence), and each filter input
carries `aria-describedby` pointing at that sentence's id — the same per-input association used in the
source group, for the same reason. `FilterCell` already receives per-column
data through the header config object (`filterValue`, `filterLabel`); the id is one more constant
prop, and it must not become a dependency of the `useMemo` that keeps `FilterCell`'s identity stable,
or the input remounts on every keystroke and drops focus (`:395-402` documents that hazard).

Alternative considered: wrapping the whole grid in a `role="group"` with a label. Rejected — it would
scope the label to the rows as well as the filter boxes, and a screen reader would announce it when
entering the data, not when entering a filter.

### The disclosure moves below the grid, beside "Load more"

Above the grid is exactly where the ambiguity lives: the column filter row is the grid's first header
row, so a caption above the grid sits between the two groups and reads as covering both. Moving the
counts below the grid, next to the `Load more` button, puts them with the rows they count and with
the control that changes them. The space above the grid is then free to carry the "Refine loaded rows"
caption, which heads the column filter row it sits directly on top of.

Its wording changes from "Sorting and filtering" to "Sorting and refining", so it names the group by
the same word the caption uses and cannot be read as a caveat on the source filters.

Alternative considered: leaving it where it is and adding "these" to the wording. Rejected — position
is the stronger signal, and the issue's complaint is that the caption "sits between the two rows and
reads as if it covers both".

### `Clear all filters` belongs to neither group, so it sits in the table utility cluster

`handleClearFilters` gains `setColumnFilters({})`; the button moves from inside the connector row to
the top-right cluster beside `Columns (n/m)` and is renamed "Clear all filters". A control that clears
both scopes cannot honestly live inside one of them. `Apply` stays inside the source group, because it
commits only that group.

Its visibility condition widens from the connector state alone to that state **or** any non-empty
column filter, so the case the issue names — the user believes filters are cleared while the grid is
still narrowed — cannot recur in reverse either.

Hiding a column already clears its filter (`:113-124`), so `columnFilters` never holds a needle for an
invisible column; the condition can read `columnFilters` directly rather than re-deriving visibility.

### Chaining is left as it is and stated

A source filter narrows what the connection returns; a column box narrows what is loaded from that.
That is already the behaviour, and it is the correct one: both are AND, in the order the data arrives.
No code changes for it. It gains a test and a line of spec, because the issue's objection was that it
is invisible, not that it is wrong.

### ADR

`docs/adr/0072-two-filter-scopes-named-not-merged.md`, plus its row in `docs/adr/README.md`. It
records keeping two scopes and naming them rather than folding the source filters into the header row,
and why folding is blocked on a filter-to-column mapping the schema does not have. Supersedes nothing;
ADR-0064 (SVAR grid, ordering stays ours) stands.

Numbering: main's highest is 0071, so 0072 is next. In-flight worktrees `issue-197` and `issue-200`
both claim 0070 already, so verify the number against `main` again before the final commit.

## Risks / Trade-offs

- **Location stays filterable in two places.** → Accepted, and it is the cost of not folding. The two
  boxes now sit in named groups that say they ask different questions. Revisit if #168's autocomplete
  makes the source Location control resolve to the same names the column shows.
- **Tailwind preflight strips `fieldset` and `legend` styling.** → Style both explicitly with the
  existing tokens; check the border does not read as a second card against `--surface` in either theme.
- **`aria-describedby` repeats the same sentence on every filter box in a group.** → Announced once
  per focus, and it is the only association that actually reaches a control's accessible description;
  a group-level reference does not. Preferred over silence.
- **A per-input reference is easy to add to the ordinary inputs and forget on the tag input**, which
  is rendered by its own branch (`ConnectorBrowser.tsx:486-514`) rather than the shared one. → Asserted
  directly: a regression test reads the accessible description of a representative ordinary source
  input (Search) **and** of the tag input, and requires both to be the source-scope sentence. The same
  assertion covers one column filter box against the refine-scope sentence.
- **Copy moves that tests assert.** → `connectorBrowserDisclosure.test.tsx` and
  `connectorBrowserFiltering.test.tsx` assert the current strings and the current clear behaviour.
  Change the assertions first and watch them fail, so the new ones are known to be able to fail.
- **The visual result is the deliverable.** → A green suite proves the strings exist, not that the two
  groups read as two groups. Screenshots in both themes, taken in a browser, are a task, not a bonus.

## Migration Plan

None. No persisted state, no API, no stored preference changes shape. Rollback is reverting the
commit.
