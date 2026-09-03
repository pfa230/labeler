## 1. The editor in `ParamInput`

- [x] 1.1 Replace the `if (control === "list" || paramSpec.type === "list") { return null; }` branch in `ui/src/components/ParamInput.tsx` with the editor, leaving it where it is: after the `date`/`datetime` branch and before the text fallback, so both spellings keep answering alike and no other branch moves.
- [x] 1.2 Derive the rows from the prop on every render — `Array.isArray(value) ? value : []` — and keep no draft array in the component. Every gesture calls `onChange` with a new `string[]` in row order. An entry the form holds nothing for renders zero rows rather than throwing.
- [x] 1.3 Render one row per element, each carrying a single-line text control holding that element, a control removing it, a control moving it one position earlier and a control moving it one position later; and below the rows a control appending an empty element as the new last row.
- [x] 1.4 Make the first row's move-earlier control and the last row's move-later control inert: `aria-disabled="true"`, and a handler that returns before touching the array. Do **not** use the `disabled` attribute for this — the spec requires them to stay in the focus order, and a natively disabled button cannot hold focus, which is what the focus rule in 1.6 needs.
- [x] 1.5 Give every control an accessible name containing the entry's `name` and, for a control acting on one element, that element's current 1-based position. Wrap the editor in a group carrying the entry's label (`spec.description || name`) as its accessible name and `noteId` as its `aria-describedby`, so a `truncated_elsewhere` note is announced with the controls it describes.
- [x] 1.6 Place focus after a move and after a removal, from a ref keyed by the control's position, set in a layout effect after the state change. A move puts focus on the same control on the moved element's new row, the inert one at a boundary included. A removal puts it on the removing control of the row that took the removed row's position, or of the preceding row when the removed row was last, or on the appending control when the removed element was the only one. Appending needs no placement.
- [x] 1.7 While `disabled` is true, disable **every** control outright, the appending control and the two inert move controls included, and render the rows the prop holds. Deferral is the one case where these controls carry the `disabled` attribute; 1.4's rule governs an editable editor only.

## 2. The print form's state and completeness

- [x] 2.1 In `initialFieldState` (`ui/src/pages/print/PrintForm.tsx:24-33`), seed `[]` into `data` for every entry whose `control` is `list` and which publishes no `default`, alongside the defaulted entries it already seeds. Do not put such an entry into `deferred`: it has no default to defer to.
- [x] 2.2 In `withArrivals` (`PrintForm.tsx:39-52`), do the same for a `list` entry arriving in a later list for the first time, and change the return guard from `deferred === value.deferred ? value : …` to one that also detects a changed `data`. Without that the seed from 2.1's sibling path is computed and thrown away, and the form looks correct until submission.
- [x] 2.3 Delete `if (input.control === "list") return true;` from the `valid` computation (`PrintForm.tsx:115`). Do not replace it with a narrower test: the entry now holds `[]`, so the existing rule passes it.
- [x] 2.4 Leave `pruneDataForSubmit` (`ui/src/lib/labelInputs.ts:240-262`) unchanged — it already passes a `list` array through — and leave `ui/src/pages/print/FieldForm.tsx` unedited, including its `invalid` computation, which `[]` already satisfies.

## 3. Tests: the editor

In `ui/src/components/ParamInput.test.tsx`. Replace the existing "renders no control when control is list or ParamSpec type is list" case at `:284`, which asserts the behaviour this change removes.

- [x] 3.1 An entry with `control: "list"` and an entry with `ParamSpec` `type: "list"` each render the editor, and a `value` of `undefined` renders zero rows and no crash.
- [x] 3.2 Appending twice and typing `A` and `B` calls `onChange` with `["A", "B"]`; appending one row and typing nothing yields `[""]`, not `[]` and not a dropped row.
- [x] 3.3 With `A`, `B`, `C`: moving `C` one position earlier and then moving `A` one position later yields `["C", "A", "B"]`, and removing `B` yields `["A", "C"]`.
- [x] 3.4 With `A`, `B`, `C`: the first row's move-earlier control and the last row's move-later control report themselves unavailable, activating either calls no `onChange`, and both are still reachable by keyboard; the other four move controls each move an element.
- [x] 3.5 Moving the second of three elements one position earlier by keyboard leaves `document.activeElement` on the first row's inert move-earlier control, and activating it again calls no `onChange`.
- [x] 3.6 Removing the middle of three rows leaves focus on the removing control of the row that took its place; removing the last of two leaves it on the preceding row's; removing the only row leaves it on the appending control.
- [x] 3.7 Two editors for entries named `tags` and `codes`, both `description: "Values"`, give every control an accessible name containing its own entry's `name`, and every control acting on one element also names that element's position.
- [x] 3.8 With `disabled` true, every control in the editor is disabled, the appending control and the two inert move controls included, and the rows still show the value passed in.
- [x] 3.9 In `ui/src/pages/print/FieldForm.test.tsx:277-286`, update the list input test from asserting that list rendering is skipped to asserting that `FieldForm` renders the list editor group and append control.

## 4. Tests: the print form

In `ui/src/pages/print/PrintForm.test.tsx`, through the existing `stubInputs` / `withInputs` / `renderForm` harness.

- [x] 4.1 A template whose only entry is `tags` with `control: "list"`, `required: true` and no `default`: the form is submittable without the editor being touched, and the submitted `data` carries `tags` as the empty array. Also assert that a list entry arriving in a later list for the first time via branch switch without defaults submits `tags: []` (protecting `withArrivals`'s return guard against Mutation A).
- [x] 4.2 Appending two elements and typing `A` and `B` submits `data` with `tags: ["A", "B"]`.
- [x] 4.3 A `tags` entry publishing `default: ["CONSUMABLE"]` opens with one row holding `CONSUMABLE`, every control in the editor disabled, the `Use default` checkbox checked; submitting sends no `tags` key.
- [x] 4.4 Clearing that checkbox makes every control operable; removing the `CONSUMABLE` row and submitting sends `tags` as the empty array.
- [x] 4.5 A `tags` entry carrying `default_error` and no `default` renders an empty, operable editor, offers no checkbox, surfaces the error's message, and still submits, sending `tags` as the empty array.
- [x] 4.6 The very first list request carries `tags` as the empty array for an untouched undefaulted entry (protecting `initialFieldState`'s seeding against Mutation B), and the entry's value survives a branch switch away and back, like any other.
- [x] 4.7 Confirm each test in groups 3 and 4 that asserts new behaviour fails against the pre-change component and passes after it, so none of them is a test that cannot fail. 3.4's "reachable by keyboard" and 4.3's deferral assertions are the ones most likely to pass either way; check them by hand.

## 5. Tests: the screens that must not move

- [x] 5.1 Locate the existing tests in `ui/src/pages/Import.test.tsx`, `ui/src/components/LabelGrid.test.tsx` and `ui/src/pages/Connect.test.tsx` that cover the list exclusions (`Import.tsx:128-140`, `LabelGrid.tsx:151`, `:155`, `:196`, `Connect.tsx:125`, `:152`, `:158`, `:177`), record which test covers each, and confirm they pass unedited. Do not modify any of them.
  - `Import.tsx:128-140` (grid column exclusion): covered by `Import.test.tsx:688-753` ("skips list inputs when building grid columns and does not break import").
  - `Import.tsx:154` (validation skip): covered by `Import.test.tsx:755-815` ("does not require a value for a required list input when the CSV has no column for it").
  - `LabelGrid.tsx:151`, `:155`, `:196` (inert cell rendering `—` and edit disabled): covered by `LabelGrid.test.tsx:329-351` ("renders inert cell with '—' and disables editing when cellInput control is 'list'").
  - `Connect.tsx:125`, `:152`, `:158`, `:177` (mapping palette, union requiredness, and row validation): covered by `Connect.test.tsx:509-563` ("skips list inputs in field mapping and grid columns").
- [x] 5.2 If no existing test covers that a grid row submits no value for a `list` entry, add one — a grid holds no array for the name, so `pruneDataForSubmit` emits nothing for it — and say in the task record which of 5.1's cases were already covered and which this added.
  - Record: Grid row omission of list data is already covered by `Import.test.tsx:751-753` and `:806-810` (both explicitly asserting `body.labels[0].data.tags` is undefined in the batch submission). All 5.1 cases were already covered; none needed adding.

## 6. Gates

- [x] 6.1 `npm run lint` in `ui/`
- [x] 6.2 `npm run test` in `ui/`
- [x] 6.3 `npm run build` in `ui/`, which typechecks; vitest does not.
- [x] 6.4 `cargo fmt`
- [x] 6.5 `cargo clippy --all-targets --all-features`
- [x] 6.6 `cargo test`
