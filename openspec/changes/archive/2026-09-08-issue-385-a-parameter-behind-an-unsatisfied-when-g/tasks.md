## 1. The connector grid's columns

- [x] 1.1 Replace `requiredUnion` in `ui/src/pages/Connect.tsx` with a memo over `detail.inputs.all`,
      dropping the `rows.length === 0` fallback to `inputs.default` and the per-row union built from
      `getRowInputs`, so `displayedFields` is one column per union name in `inputs.all` order and does
      not depend on any row (D1, D6 "Order").
- [x] 1.2 Give `Connect.tsx`'s `cellInput` a second lookup: the row's own entry from `getRowInputs`
      where that list reports the name, otherwise the `inputs.all` entry of the same name, keeping the
      existing `{ name, control: "text" }` fallback for a row whose list has not arrived (D2).
- [x] 1.3 Leave `validateRow` and the two `pruneDataForSubmit` call sites reading `getRowInputs`
      untouched, so a row is refused only for a name its own list reports and submits only those names
      (D3).

## 2. The CSV import grid's columns

- [x] 2.1 Replace `requiredUnion` in `ui/src/pages/Import.tsx` with a memo over `detail.inputs.all`
      (`templateFields`), dropping the `rows.length === 0` fallback and the per-row union, and leave
      `displayedFields` seeding its `Set` from `csvFields` first and filtering `listNames`, so CSV headers
      keep the file's order, parameter columns the file lacks are appended in `inputs.all` order, and
      `list` entries still get no column (D1, D6).
- [x] 2.2 Give `Import.tsx`'s `cellInput` the same second lookup as 1.2, so a name in neither the
      row's list nor `inputs.all` — a CSV column the template does not declare — still returns
      `undefined` and renders inert (D2).
- [x] 2.3 Leave `Import.tsx`'s `validateRow` and its `pruneDataForSubmit` call sites reading
      `getRowInputs` untouched (D3).

## 3. Tests for the changed contract

Write each test so it fails against the unmodified pages before the change is made; a test that passes
either way proves nothing (tests 3.4 and 3.9 are regression guards for the two restated ordering
scenarios whose fixtures read all three parameters unconditionally, asserting order preservation under
`inputs.all` rather than variant divergence).

- [x] 3.1 `ui/src/pages/Connect.test.tsx`: a template gating `{tags}` and `{location}` (both declared
      `string`) behind `when: { orientation: horizontal }`, with `orientation` an `enum` declaring no
      `default:` and nothing mapped onto it, shows a column for `orientation`, `tags` and `location`,
      and each row's `tags` and `location` cell is editable and refuses no row for either — the delta's "A gate no
      row satisfies still shows the fields behind it".
- [x] 3.2 `ui/src/pages/Connect.test.tsx`: typing `horizontal` into that row's `orientation` cell,
      after a `tags` value was typed while the gate was unset, makes the row's list report `tags` and
      the submitted `data` carry both `orientation` and that `tags` value — "Typing the gate's value
      brings its branch into play".
- [x] 3.3 `ui/src/pages/Connect.test.tsx`: with one row on `orientation: horizontal` and one on
      `vertical`, the `subtitle` cell of the vertical row is editable, is not validated, keeps its
      value across switching to `vertical` and back, and is absent from that row's submitted `data`
      while inactive — "A grid cell for a name inactive on its row is inert" and "An inert cell keeps
      its value and comes back".
- [x] 3.4 `ui/src/pages/Connect.test.tsx`: a template declaring `params:` as `title`, `subtitle`,
      `code` and reading all three unconditionally renders its columns, and validates, in that order —
      "The connector grid orders every column by declaration" and requirement 1's "The Connect grid
      preserves input-list order".
- [x] 3.5 `ui/src/pages/Connect.test.tsx`: a `list` parameter fed by a mapped multi-valued connector
      column and read only inside a container gated on `orientation: horizontal`, with `orientation`
      an `enum` declaring `values: [horizontal, vertical]` and no `default:` and nothing mapped onto
      it, shows a `tags` column whose cell reads the elements' display text and cannot be edited, the
      row keeps the array, no error is reported against `tags`, and the row is refused and the run
      blocked for the missing `orientation` its own list reports as required — "A mapped list behind an
      unsatisfied gate stays visible and read-only".
- [x] 3.6 `ui/src/pages/Connect.test.tsx`: continuing from 3.5, typing `vertical` into that row's
      `orientation` cell leaves the row unrefused, the `tags` cell still read-only over its array, and
      the submitted `data` carrying `orientation` and no `tags`; typing `horizontal` instead makes the
      row's list report `tags` and submits the connector's elements in the connector's order, unchanged
      and unflattened — "A valid row on the other branch submits without the mapped list" and
      "Activating the gate submits the mapped array unchanged".
- [x] 3.7 `ui/src/pages/Import.test.tsx`: a parameter read only inside one `when:` branch is offered
      for a row whose values select the other branch, its column is editable there, and that row is not
      refused for having no value for it — the Import requirement's "A parameter read only inside one
      branch is offered for every row".
- [x] 3.8 `ui/src/pages/Import.test.tsx`: for a template declaring `title`, `subtitle`, `code` in that
      order and reading all three unconditionally, a CSV whose headers read `subtitle`, `title` yields
      columns `subtitle`, `title`, `code` — "The import grid keeps its CSV's header order and appends
      the rest".
- [x] 3.9 `ui/src/pages/Import.test.tsx`: the same template with a CSV whose headers read `title`,
      `subtitle` yields columns, and validation, in `title`, `subtitle`, `code` order, `code` being the
      appended one — requirement 1's "The Import grid preserves input-list order".
- [x] 3.10 `ui/src/pages/Import.test.tsx`: two rows selecting different branches are each invalid only
      for their own branch's missing required name, while both columns show and are editable on both
      rows — "Two grid rows selecting different branches require different inputs".

## 4. What must not have moved

- [x] 4.1 Run `ui/src/components/LabelGrid.test.tsx` unchanged and confirm it passes: the component's
      inert-cell and editor contract is not this change's subject, and a failure there means the change
      reached the wrong layer (D2).
- [x] 4.2 Re-read every page test that asserts a column's absence, a cell's inertness, or a row's
      acceptance against the amended delta, and correct the ones it inverts rather than patching them
      to keep passing. `Connect.test.tsx`'s mapped-list test asserting the row is not refused is one:
      the row is refused for its missing `orientation` (3.5). Leave the list-column assertions of
      `Import.test.tsx` ("skips list inputs when building grid columns") and the `—` assertions for an
      undeclared CSV column standing.
- [x] 4.3 Confirm `git diff --stat` touches `ui/src/pages/Connect.tsx`, `ui/src/pages/Import.tsx` and
      the two page test files only: no Rust file, no `LabelGrid.tsx`, no `labelInputs.ts`, and no
      OpenAPI change (proposal Impact, design Non-Goals).

## 5. Gates

- [x] 5.1 `cd ui && npm run lint && npm run test && npm run build` — the three gates this diff touches.
- [x] 5.2 `cargo fmt --check`, `cargo clippy --all-targets --all-features` and `cargo test` — unchanged
      by this diff and required to stay green.
