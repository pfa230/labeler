## Context

See proposal.md — Why, and #385 for the traced chain and the reproduction.

The service already derives two input lists from one walk (`derive_inputs_internal`,
`src/templates.rs:193`). With `resolved_data: None` every item is active and the walk records every
branch's reads: that is `inputs.all` on the template detail. With a label's resolved data it evaluates
each `when:` and skips an inactive subtree: that is `POST /api/templates/{id}/inputs`, one list per
label. Both are published today and both are already fetched by the two screens this change touches —
Connect reads `detail.inputs.all` for its field-mapping palette (`Connect.tsx:176-179`) and Import
reads it to find `list` entries (`Import.tsx:128-134`).

What the grids do not do is read it for their columns. `requiredUnion` (`Connect.tsx:203-218`,
`Import.tsx:111-126`) unions the *per-row* lists, so a name behind a gate no row satisfies has no
column anywhere.

Three questions #385 leaves open are decided below: where the union comes from (D1), what the
submitted `data` carries (D3), and whether an inactive column reads differently (D4).

## Goals / Non-Goals

**Goals:**

- Every parameter the layout reads has a grid column, on both grids, whatever any row's data holds.
- A cell in such a column is editable, and typing into it feeds the row once its own values activate
  the branch that reads it.
- No row that is valid today becomes invalid, and no row that is valid today submits different `data`.

**Non-Goals:**

- No backend change. `derive_inputs_internal`, both endpoints, and the OpenAPI document are untouched.
- No `enum` dropdown in a grid cell (#271). The gating parameter stays a free-text cell; this change
  makes the gate discoverable, not guided.
- No `list` editor on a grid (#348). Import keeps excluding `list` entries from its columns; Connect
  keeps rendering a `list` column non-editable, and keeps holding and submitting a materialized array
  unchanged.
- No new visual language for the grid.

## Decisions

**D1. The column set comes from `detail.inputs.all`, not from a new field on the per-row endpoint.**
The union already exists on the wire, is already fetched by both screens, and is derived by the same
walk that answers the per-row endpoint, so the two cannot drift. The alternative — teaching
`POST /api/templates/{id}/inputs` to report both sets, or to report the union with a per-entry
`active` flag — is a wire change, an OpenAPI change and a Rust change, and it would repeat one
template-wide answer once per label in the response. It buys nothing the detail body does not already
carry. Both `requiredUnion` blocks collapse to a memo over `detail.inputs.all`, and the
`rows.length === 0` branch that fell back to `inputs.default` disappears: columns no longer depend on
rows at all, so a grid can paint them before a single row's list has arrived.

Per-row lists are still requested, on the same terms, because validation and submission still need
them.

**D2. A cell's control comes from the row's own entry when the row's list reports the name, and from
the `inputs.all` entry otherwise.** The order matters: the two lists can disagree about `control` for
one name, because the `image` override applies to the union whenever *any* branch binds the name to an
`image` item, while the row's list reports what that row's active branch does with it. The row's answer
is the more specific one, so it wins where it exists. `cellInput` in both pages gains that second
lookup; its existing `{ name, control: "text" }` fallback for a row whose list has not arrived stays,
and a name in neither list — an Import CSV column the template does not declare — keeps returning
`undefined`, which `LabelGrid` already renders as an inert `—`.

`LabelGrid` itself is not touched. It renders an inert cell for a field with no spec and an editable
one otherwise; what changes is what the pages hand it. Its own tests keep passing unchanged, which is
the check that the component's contract did not move.

**D3. Validation and submission keep reading the per-row list, and neither changes.** `validateRow`
already iterates `getRowInputs(row.id)`, so a required parameter behind a branch the row does not
activate never refuses the row — that property is inherited, not added. `pruneDataForSubmit` already
drops a value whose name the row's list omits, which is the existing rule for a value entered before a
branch was deactivated; a value typed into a column no row activates reaches submission through the
same door. So the request for a row that is valid today is byte-identical, and the only new state is a
value held on screen.

**D4. No marker distinguishes a cell whose column the row's list does not report.** Activeness is per
row: the same column is in play for a row selecting that branch and idle for the row beside it, so a
column-level mark would be false for half the grid, and a cell-level mark would restate what the row's
own gate value already says. Rejecting it now is also the cheaper mistake: adding a presentation later
is additive, while removing one operators have learned is not. The spec states the prohibition rather
than leaving it unsaid, so adding one is a deliberate change to that requirement.

**D5. A name read only inside a `repeat:` subtree gets a column too, and on Connect that column is
live.** `inputs.all` walks every `repeat:` subtree exactly once, so its union holds those names whether
or not a row supplies elements. A grid has no `list` editor (#348), but that does not mean no grid can
hold a list: a Connect row materialized with a multi-valued connector column mapped onto a `list`
parameter carries that column's array (`connector-multi-valued-fields`, `labelInputs.ts:250-254`), so
the repeat draws an instance per element and every name the subtree reads is one that row genuinely
needs. Those columns are exactly the ones #385 is about, and until this change a row whose gate was
unset hid them.

Where no array can arrive — a CSV row, a row added by hand, a Connect row with nothing mapped onto the
list — the subtree activates only if the `list` parameter declares a resolvable non-empty `default`,
and otherwise the column is one that row cannot use. It is still the right column: the alternative is a
client re-deriving which names sit under a repeat, which is the layout-walking the screen requirement
forbids, and it would hide from a mapped row the very fields the mapping fills.

**D6. Each grid keeps the column rules that are its own, and the delta states both.** Two survive the
union and neither is this change's subject:

- **Order.** Import lists the loaded CSV's headers first, in the file's order, then appends the
  parameter columns the file has no header for (`Import.tsx:136-140`, a `Set` seeded from `csvFields`).
  So `inputs.all` order governs the appended group alone, and a CSV whose headers read `subtitle`,
  `title` for a template declaring `title`, `subtitle`, `code` yields `subtitle`, `title`, `code`.
  Connect has no file, so its columns are `inputs.all` order throughout. Re-sorting Import's columns
  into declaration order would re-order the operator's own sheet, which the union gives no reason to
  do; the delta therefore states one rule per grid rather than one rule with a screen that breaks it.
- **Lists.** Import drops `list` entries from its columns; Connect shows the column and `LabelGrid`
  renders the cell non-editable, holding and submitting a materialized array unchanged (D5).

**D7. The spec delta restates three whole requirements because `MODIFIED` replaces the block.** Verified:
`openspec validate --strict` refuses a `MODIFIED` block that drops a scenario name the current spec
carries, so `A grid cell for a name inactive on its row is inert` keeps its heading while its outcome
inverts, with the italic note this capability already uses for that situation.

The ordering requirement (`An input list describes the controls one label needs`) is **modified** to
update the Import and Connect grid scenarios: their mechanism clauses state that columns come from
`inputs.all` and validation from each row's own list, rather than columns coming from walking the row's
inputs response, and the `Import.tsx` line citation is refreshed to line 124.

## Risks / Trade-offs

- **A template with several variants makes a wider grid, and Connect's grid is already wide.** →
  Accepted: it is the change. Columns flex and the grid scrolls; showing five columns nobody can find
  is what #385 is about.
- **A value typed into a column no row activates is silently not submitted.** → This is the existing
  retention rule, not a new one, and it is what makes typing ahead of the gate work at all. The row
  preview renders the pruned `data`, so what the label carries is visible.
- **The gate itself is still a free-text cell (#271).** → A grid cell for an `enum` offers no dropdown
  and no list of allowed values, so an operator must know that `horizontal` is what `orientation`
  takes. The fields are now visible and the gate has a column, which is the part #385 asks for; the
  affordance is #271's.
- **Existing tests encode the old behavior.** → `LabelGrid.test.tsx` tests the component's inert-cell
  contract, which does not change. Page-level tests mostly stub `inputs.all` equal to `inputs.default`,
  so they keep passing; any that assert a column's absence must be re-read against the new rule rather
  than patched to keep passing.

## Migration Plan

None. The change is client-side, holds no persisted state, and alters no request the server sees.
