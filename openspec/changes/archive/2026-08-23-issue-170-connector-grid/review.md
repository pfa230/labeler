## Review Metadata

- **Round**: 2
- **Prior round**: 1 (APPROVE_WITH_CHANGES, CHANGES_APPLIED yes, RECHECK_RESULT ALL_ACCEPTED) - archived at the end of this file
<!-- CANONICAL FIELDS - machine-readable, each on its own line, exactly this format. -->
<!-- Which agent wrote the artifacts under review, and which wrote this review. -->
<!-- e.g. claude | agy | codex | opencode | fresh-context-subagent -->
<!-- They MUST differ: nobody reviews their own work. -->

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only; no edits made
- **Artifacts reviewed**: proposal.md, specs/, design.md (amended); the installed `@svar-ui/react-grid` 2.7.3 and `@svar-ui/grid-store` 2.7.2 packages including recovered sources
- **Issue**: #170
- **Why round 2**: three implementation spikes against the shipped package falsified two factual premises of the round-1 `design.md` (unconditional row windowing; the grid is a bounded-height inner scroll viewport). `design.md` was amended, which voids the round-1 verdict. `proposal.md` and `specs/` were unchanged at the time of this review.

<!-- STALENESS: this verdict covers only the contents reviewed in this round. Any -->
<!-- later edit to proposal.md, specs/ or design.md, other than applying the listed -->
<!-- Required Changes, VOIDS it and requires a new round. -->

<!-- This file IS the reviewer's output, redirected here. Findings, Injection -->
<!-- Attempts and Verdict are its words. The author appends only Rebuttals and sets -->
<!-- CHANGES_APPLIED, with targeted edits: never rewrite the file. -->

## Findings

### Critical (blocking)

### Moderate

- `design.md:150-168` makes the test-only `ResizeObserver` report enough height to render every row, but that mock also makes tests pass if the production wrapper has no bounded height—the exact defect that would collapse the real grid. It additionally removes production windowing from every jsdom test, despite `design.md:14-16` treating all-rows-in-DOM as a requirement. Specify the actual wrapper-height contract and add a test that fails when it is absent; keep any all-rows observer override scoped to tests that intentionally need it, with at least one test exercising a realistic bounded window.

- `design.md:128-137` does not disable SVAR’s independent selection model. In the shipped package, `select` defaults to `true`, and clicking any non-input cell dispatches `select-row`; the checkbox spike proves only that checkbox clicks are ignored. The plan must require `select={false}` and verify that ordinary cell, link, drill-button, and keyboard interaction cannot populate `selectedRows` or apply SVAR’s selected-row styling.

- `design.md:70-73,107-126,200-204` precisely defines controlled sorting but never defines the equivalent ownership boundary for filtering. SVAR’s `filter-rows` mutates its own `filterValues` and `_filterIds`; merely passing filtered data and a controlled `filterValues` object is not the same as preventing that internal state transition. Specify whether `filter-rows` is intercepted and returns `false`, how the application updates controlled filter state, and how clearing/hiding a filter resets both the displayed input and derived rows.

- `design.md:24-25,54-57,118-122` overstates the accessibility supplied by SVAR. Its built-in text filter renders an unlabeled `<input>`, its second filter header emits `aria-sort="none"` even when the first header reports the active direction, and the grid does not provide the disclosure live region cited in the TanStack rejection. `design.md:185-190` also leaves the replacement arrow implementation unspecified. Define accessible names for every filter, the intended assistive-technology behavior of the two header rows, the exact locally rendered arrow mechanism, and tests that fail when the visual direction or accessible sort state is wrong.

- Several amended factual claims remain inaccurate. `design.md:101-103,115-116` says SVAR destroys the supplied array “in place,” but the shipped store first clones it and sorts the clone; the absence of a built-in third state is real, but that stated reason is not. `design.md:55` says react-data-grid rows “cannot grow,” although the installed incumbent supports a per-row `rowHeight` callback and its nowrap CSS can be overridden; the genuine distinction is that it does not measure wrapped content automatically. `design.md:56` treats AG Grid’s selection model as mandatory, although row selection is opt-in and a custom application-owned checkbox cell can be used without it. Correct these claims and state the surviving rationale honestly: SVAR supplies automatic wrapped-content measurement with less application-owned layout machinery, while the AG Grid rejection is a preference about fit or complexity, not unavoidable duplicate selection state.

- `proposal.md:13-15,27-30,70-71` is stale after the amendment. The change now implements application-owned comparison, filtering, action interception, disclosure copy, checkbox cells, icon CSS, geometry, and a test observer; it also replaces page-flow scrolling with a bounded inner viewport. Therefore “everything below is the grid’s behavior,” “none … from scratch,” “everything … preserved,” and “additive” are no longer accurate. Update the proposal to disclose the bounded viewport and narrow the ownership and preservation claims.

### Suggestions

- `design.md:170-171` incorrectly uses the 200-row materialize cap as a bound on loaded rows; it caps selection, while repeated Load more actions can accumulate more rows. Remove that rationale and base the windowing assessment on the loaded-row behavior actually supported.

- `design.md:233-234` delegates disclosure wording entirely to implementation even though these strings distinguish three materially different states. Record the exact filtered-count, partial-load, and no-match-with-more-available messages, their placement, and whether changes are announced through a polite status region.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Define the bounded-height production contract and a production-faithful geometry/windowing test strategy; prevent the global observer shim from masking a missing height.
2. Explicitly disable SVAR selection with `select={false}` and test non-checkbox and keyboard interactions.
3. Specify controlled filter action handling, including interception, state updates, hidden-column clearing, and Load more behavior.
4. Specify and test filter accessible names, two-row sort semantics, local arrow glyphs, and disclosure announcements.
5. Correct the false package claims and rewrite the SVAR/react-data-grid/AG Grid rationale around capabilities actually established.
6. Update `proposal.md` to acknowledge application-owned behavior and the bounded inner-scroll viewport.

<!-- CANONICAL FIELD - the AUTHOR sets this only after every required change is -->
<!-- applied AND the reviewer has re-checked them. -->
<!-- yes = all applied and re-checked | no = outstanding | n/a = verdict is APPROVE or REVISE -->

## Rebuttals

<!-- Author responds: fixed (cite the change) or rebutted (give reasoning). -->
<!-- NOT self-certifying: rebutting a Critical or Moderate counts only once the -->
<!-- reviewer marks it "accepted by reviewer" with a one-line reason. Suggestions -->
<!-- may be declined by the author alone. -->

Every finding was verified against the installed package before being accepted. Two were confirmed by
reading code the author had not previously read, and one of those (RC2) is a production defect this
change would otherwise have shipped.

**RC1. The shim can mask a missing wrapper height** - Fixed, and the finding is correct: a shim
reporting a large height makes every test pass whether or not the real wrapper is bounded, and an
unbounded wrapper collapses to zero because `.wx-grid { height: 100% }` of an auto-height parent is
zero. `design.md` "The grid owns a bounded scroll viewport" now pins the contract separately from the
shim: a named `connector-grid-viewport` class whose height is declared once in `theme.css`; a test
asserting the wrapper carries it and a test asserting the stylesheet declares a height for it, so
deleting either fails rather than yielding a silently empty grid; the shim reporting that same declared
height rather than an arbitrary one; and at least one test overriding the shim with a small viewport to
assert windowing genuinely engages. The section also now states plainly that all-rows-in-DOM is a
property of the test configuration, not of the component, and the Goals bullet was rewritten so it no
longer reads as a production guarantee.

**RC2. SVAR's own selection is not disabled** - Fixed, and this was the most valuable finding in the
round. Verified: `Grid.jsx:44` defaults `select = true`, and `Layout.jsx:507-529`'s click handler
dispatches `select-row` for a click on any cell, guarded only by `if (ev.target.closest("input")) return;`.
The task-1.3 spike clicked only checkboxes, so its "selectedRows stayed empty" result proved nothing
about ordinary row clicks; it was passing for the wrong reason. `design.md` "Selection stays entirely
ours" now requires `select={false}` and requires tests for a click on an ordinary cell, on the name
link, on the Drill in button, and keyboard activation, each asserting `selectedRows` stays empty, no row
carries `wx-selected`, and our own `SelectedRow[]` is unchanged.

**RC3. Filtering has no defined ownership boundary** - Fixed. Confirmed that `filter-rows` maintains
`filterValues` and computes `_filterIds`, which feeds `flatData`, so a filtered array plus internal
filtering would apply the predicate twice over two different notions of which rows exist. Rather than
dispatch and intercept, the filter row now uses a custom header cell (`header: [{text}, {cell: ...}]`,
which `HeaderCell.jsx` renders in place of its built-in `<Filter>`). The grid is never given filter
state, so there is nothing to intercept or keep in step, and clearing a filter or hiding a column is one
React state change.

**RC4. Accessibility is overstated** - Fixed on every point, and the underlying facts are confirmed.
`@svar-ui/react-core`'s `Text` accepts `id`, `placeholder` and `title` but no `aria-label`, so the
built-in filter would ship unlabeled; the custom cell from RC3 carries an explicit per-column accessible
name. `HeaderCell.jsx` does hardcode `aria-sort="none"` on a filter cell and this is not configurable;
it is now recorded as a known limitation and bounded by a test asserting exactly one header cell per
sorted column reports a non-`none` `aria-sort`. The arrow glyph is now specified concretely as scoped
CSS `::before` content on `.wxi-arrow-up`/`.wxi-arrow-down` using Unicode triangles, fetching nothing.
The three disclosure strings are now written out verbatim in `design.md` with their placement and a
`role="status"` region, rather than delegated to implementation.

**RC5. Remaining false package claims** - Fixed, all three. (a) The store clones before sorting
(`const nextData = [...data]; nextData.sort(sorter)`), so "destroyed in place" was wrong; the corrected
mechanism is recorded, and the conclusion that survives is stated separately. (b) react-data-grid does
accept a per-row `rowHeight` callback and its nowrap CSS is overridable, so "cannot grow" was too
strong; the rejection is restated as what it actually is, the absence of automatic measurement of
wrapped content. (c) AG Grid row selection is opt-in, so the claim that its selection model would
force a second source of truth was wrong; the row now states the honest, narrower reasons and records
explicitly that AG Grid is the first thing to revisit if the bounded viewport proves wrong.

**RC6. proposal.md is stale** - Fixed. "Everything below is that grid's behavior, configured; none of
it is behavior we implement from scratch" is replaced by a narrower and accurate claim; a new bullet
discloses the bounded scroll region as a visible change; "everything the table does today is preserved"
is qualified with "except its page-flow layout"; and the Compatibility line now separates additive
capability from changed presentation.

**Suggestion 1. The 200-row cap bounds selection, not loaded rows** - Fixed. Correct, and it was an
error inherited from round 1: Load more accumulates loaded rows without bound. The windowing rationale
no longer rests on that cap.

**Suggestion 2. Disclosure wording** - Fixed rather than declined; see RC4.

### Reviewer re-check (round 2 required changes)

1. accepted by reviewer - `design.md:176-200` defines the bounded wrapper contract, height-presence checks, production-sized observer geometry, and an explicit small-viewport windowing test.
2. accepted by reviewer - `design.md:143-155` requires `select={false}` and covers ordinary-cell, link, Drill-in, and keyboard interactions against both SVAR and application selection state.
3. NOT accepted - `design.md:248-253` uses `{ cell: OurFilterCell }` without `filter`, so the shipped header still dispatches sorting when input clicks bubble (`ui/node_modules/@svar-ui/react-grid/dist/index.es.js:527-529,654-661`).
4. NOT accepted - `design.md:260-267` falsely claims the custom filter header gets `aria-sort="none"`; without `filter`, the shipped header makes it sortable and exposes the active sort state (`ui/node_modules/@svar-ui/react-grid/dist/index.es.js:656-662`).
5. NOT accepted - despite the correction at `design.md:102-107`, `design.md:119-120` retains the false claim that the grid sorts `data` in place.
6. accepted by reviewer - `proposal.md:13-19,31-38,78-80` now identifies application-owned behavior and explicitly discloses the bounded inner-scroll viewport and lost page-flow layout.
S1. accepted by reviewer - `design.md:209-212` correctly distinguishes unbounded loaded rows from the 200-row selection cap.
S2. accepted by reviewer - `design.md:275-284` records all three strings exactly, their placement, and polite `role="status"` announcements.

RECHECK_RESULT: NOT_ALL_ACCEPTED

## Author response to re-check (round 2)

All three rejections verified against `HeaderCell.jsx` before acting; all three were correct.

**3 and 4** share one root cause and one fix. Every sort-suppressing branch in `HeaderCell.jsx` keys on
`cell.filter` (lines 61, 88, 255, 261), while the render branch prefers `cell.cell` (285) over the
built-in `<Filter>` (298). Declaring `cell` alone therefore rendered our input inside a still-sortable
header: a bubbled click would have sorted the column, and that row would have advertised the active sort
direction instead of `none`. Declaring `filter` alone would render SVAR's unlabeled input and dispatch
`filter-rows`. The design now declares **both**, `{ cell: OurFilterCell, filter: "text" }`, with the
per-line reasoning recorded so the pairing cannot be "simplified" away later, and the `aria-sort="none"`
claim now states that it follows from declaring `filter` rather than from using a custom cell.

**5** was a straight miss: the bullet was corrected but the prose sentence restating it was not. The
sentence no longer claims in-place sorting and rests only on what is true, that `sortMarks` has no third
state and the store retains only the sorted order.

All six Required Changes and both Suggestions are now applied and re-checked by the reviewer.

CHANGES_APPLIED: yes

### Reviewer re-check (round 2, second pass)

3. accepted by reviewer - `ui/node_modules/@svar-ui/react-grid/dist/index.es.js.map→HeaderCell.jsx:61,285-299` suppresses sorting and prefers `cell.cell`; `Filter.jsx:10-12` and `DataStore.ts:473-507` confirm no built-in filter dispatch or filter-state construction occurs.
4. accepted by reviewer - `ui/node_modules/@svar-ui/react-grid/dist/index.es.js.map→HeaderCell.jsx:255,260-265` makes a filter header non-focusable for sorting and unconditionally exposes `aria-sort="none"`.
5. accepted by reviewer - `design.md:102-107` now correctly states that sorting clones the supplied array, matching `ui/node_modules/@svar-ui/grid-store/dist/index.js.map→DataStore.ts:422-424,462-469`.

RECHECK_RESULT: ALL_ACCEPTED
