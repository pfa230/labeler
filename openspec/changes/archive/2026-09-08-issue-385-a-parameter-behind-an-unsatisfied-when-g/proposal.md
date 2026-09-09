## Why

Implements #385. A template parameter reaches the batch grid as a column only if the service reports
it among *that row's* inputs, and that per-row report drops every parameter sitting inside a `when:`
the row does not currently satisfy. A connector row arrives with the gating parameter unset, so the
gated fields have no column, no cell and nothing to type into: they are not blank, they are absent,
and nothing tells the operator that more fields exist behind a value they have not entered yet.

## What Changes

- The batch grid's column set on **Connect** and **Import** becomes the union of every parameter the
  layout reads across all `when:` branches, taken from the template detail's `inputs.all`, instead of
  the union of what each row's own data currently activates.
- A grid cell for a name the row's own input list does not report becomes **editable**, using the
  control the union publishes for that name, and keeps its value. Today it renders as an inert `—`.
  **BREAKING** against the published contract: two scenarios of `template-inputs` say such a cell is
  inert, and they are rewritten.
- **Validation is unchanged and stays per active branch.** A row is incomplete exactly when an entry
  in *its own* reported list is `required` and empty. A column belonging to a variant this row does
  not activate never refuses the row, so every row that is valid today stays valid.
- **What a row submits is unchanged.** A row submits exactly the names its own list reports, so a
  value typed into a cell no active branch reads is retained on screen and is not sent, exactly as a
  value from a deactivated branch is retained and not sent today. No request changes for any row that
  is valid now.
- No marker distinguishes a cell whose column is inactive for its row. Activeness is per row, not per
  column, so a column-level marker would be false for the rows that do activate it.
- No backend change, no wire change. `GET /api/templates/{id}` already publishes `inputs.all`, and
  `POST /api/templates/{id}/inputs` keeps its contract unchanged; it stops driving the column set and
  drives validation and submission alone.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `template-inputs`: three requirements change.
  - *An input list describes the controls one label needs* — the batch grid shows a column for a `list`
    entry and renders the cell read-only, holding a mapped array unaltered while the row's input list
    does not report that name and submitting it unchanged when it does; and the mechanism clauses of
    both ordering scenarios (Connect and Import) are updated to state that columns come from `inputs.all`
    and validation from each row's own list.
  - *A screen renders the reported inputs and decides nothing else* — the rule that a screen renders
    exactly the entries reported for the label it is about to submit is split by screen shape: it
    still binds a screen collecting one label (the print form), while a grid collecting many labels
    draws its columns from `inputs.all` and judges each row against that row's own list. The two grid
    scenarios asserting an inert cell for an inactive name are rewritten; every other rule and
    scenario in the requirement is restated unchanged.
  - *The CSV Import screen offers the parameters the chosen template can read* — the sentence
    withholding a parameter "read only inside a branch the row's own values deactivate" is replaced
    by the every-variant rule.

`conditional-visibility` is **not** modified. What `when:` means, when an item is active and what a
per-label input list reports are all unchanged; this change moves which of the service's two existing
reports one client reads.

## Impact

- `ui/src/pages/Connect.tsx` (the `requiredUnion`/`displayedFields` derivation and `cellInput`) and
  `ui/src/pages/Import.tsx` (the same two, keeping its `control === "list"` filter, which stays with
  #348).
- `ui/src/components/LabelGrid.tsx` is untouched: it already renders an inert cell for a field with no
  spec and an editable one otherwise, so what changes is what the two pages hand it.
- No Rust change. `src/templates.rs`, `src/api.rs` and the OpenAPI document are untouched.
- Gates: `ui/` only — `npm run lint`, `npm run test`, `npm run build`.
