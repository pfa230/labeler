# 22. Import option model and template-switch persistence

## Status

Accepted. Implements milestone M8 ([#55](https://github.com/pfa230/labeler/issues/55),
[#56](https://github.com/pfa230/labeler/issues/56), [#65](https://github.com/pfa230/labeler/issues/65),
[#57](https://github.com/pfa230/labeler/issues/57), [#32](https://github.com/pfa230/labeler/issues/32)).
Refines [ADR-0014](0014-csv-import-grid.md) (the CSV import grid); does not supersede it.

## Context

Testing the Import and Print flows surfaced UX defects. Both pages keyed a child component by the
template id (`<CsvEditor key={detail.id}>`, `<PrintForm key={templateId}>`), so switching templates
remounted it and discarded the loaded CSV / entered fields. The Import grid split a template's declared
options inconsistently: an option became a per-row column only if the CSV supplied an `option.<name>`
column, otherwise it appeared once in a top-level "manual" strip applied to the whole batch, so two
equally-valid options landed in different places. The primary actions rendered below a non-virtualized
grid (up to 500 rows), pushing them off-screen. And `/import/csv` ignored `option.<name>` columns.

## Decision

- **Uniform option model.** Every declared option is an always-present per-row grid column, defaulting to
  its **first allowed value** (a CSV/import value wins). Single-valued options (e.g. `outline: [yes]`)
  render read-only. A per-option **explicit "Apply to all rows" button** overwrites every row on click;
  changing the control's selector does NOT mutate rows (no silent clobber of per-row edits). This balances
  low friction (rows are valid on load) against the bulk-edit anti-pattern of auto-overwrite, per UX
  research (Helios/PatternFly/Polaris).
- **State survives a template switch.** Drop the key-based remounts. Reconcile a row's options against the
  current template **at render** (compute the effective option per row), not via a `useEffect` that calls
  `setState` — the repo's eslint makes `react-hooks/set-state-in-effect` an error, and render-time
  derivation also avoids the stale-closure/loop risks of effect-based reconciliation. `useTemplate` uses
  react-query `keepPreviousData` so the template detail stays mounted through a refetch (otherwise the
  transient `undefined` during the switch would unmount the form and wipe state).
- **CSV before a template.** The Import editor renders with an optional template: a CSV can be loaded with
  no template selected (data columns show), and option columns + validation activate once a template is
  chosen. Raw CSV `option.<name>` values are retained until then.
- **Sticky action bar.** Print/Download + the label count + over-cap/error sit in a `sticky` bar in both
  Import and Connect, reachable regardless of grid length.
- **`/import/csv` parity.** The automation endpoint routes `option.<name>` columns to per-row options and
  defaults missing declared options to their first allowed value, matching the UI.

## Consequences

- Stored rows keep their raw option map; defaults are applied in the view and at submit, and baked into a
  row only when it is edited. This is a slight conceptual subtlety but is what makes a no-remount template
  switch correct under the lint rules.
- `keepPreviousData` briefly shows the previous template's detail during a switch; acceptable because
  templates are near-static, and the render-time reconciliation adapts options as soon as the new detail
  arrives.
- A blank declared-option cell remains invalid (a user-cleared state); defaulting on load/switch keeps that
  from being the default experience.
