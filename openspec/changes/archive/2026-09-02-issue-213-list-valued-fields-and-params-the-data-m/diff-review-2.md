TREE_SHA256: 9cb7bbb145a1c2cc4829e38021e83e88b366e894ce615ed6daad44f66973bc64

# Adversarial diff review: `issue-213-list-valued-fields-and-params-the-data-m`

Gates run locally, all green [verified]: `cargo fmt --check` (exit 0), `cargo clippy --all-targets --all-features` (0 warnings), `cargo test` (794 passed, 0 failed, 2 ignored), `npm run lint`, `npm run test` (49 files / 441 tests), `npm run build`. `specs-digest.sh` recomputes `b5c04696…`, matching `review.md`'s `SPECS_SHA256`, so the plan verdict is not voided.

Round 1's findings were addressed: the print-form deadlock (`PrintForm.tsx:115`), the missing join assertions (`render/helpers.rs:2028-2093`, `render/mod.rs:10223-10281`), the untested `string`-array refusal (`render/mod.rs:10285-10444`, `lib.rs:8577-8595`), the unrelated `toast.tsx` change (reverted), the `when: {}` / blank-key / blank-value pins (`templates.rs:7466-7482`), and the silent coercion in the join loop (`render/helpers.rs:158-159`, now `field_value_not_scalar`).

The findings below are what survives.

---

## BLOCKING 1: the `list` placeholder fill is asserted by nothing, and task 5.6 claims it is

`src/templates.rs:192-196` is the whole of the thumbnail's `list` behaviour: `InputControl::List` fills a one-element array holding the entry's own name. Nothing in the tree asserts that value.

The three thumbnail tests written for it assert only the HTTP envelope:

- `src/lib.rs:8663-8664` (`default: [KIDS, CONSUMABLE]`): `status == OK`, `content-type == image/png`.
- `src/lib.rs:8704` (no `default:`, the case the fill exists for): `status == OK`, nothing else.
- `src/lib.rs:8745` (`default: []`): `status == OK`, nothing else.

Change the fill at `templates.rs:193-195` to `Value::Array(vec![])`, or to a two-element array, or to any other list of strings, and all three still pass: any array of strings coerces, joins, and renders a 200 PNG. The tests cannot fail for the property they are checked against.

Task 5.6 states the property explicitly: "a template joining a `list` with no default renders **and reads the parameter's own name**; with `default: [CONSUMABLE, KIDS] `it **reads `CONSUMABLE, KIDS`**; with `default: []` it renders that text **empty**." Three delta scenarios say the same (`specs/template-inputs/spec.md:463-478`: "the thumbnail renders and reads `tags`", "the thumbnail reads `CONSUMABLE, KIDS`", "renders with that text empty"). None of the three read-outcomes is observed.

The cheap mechanism is already in the file and already used for exactly this: `test_placeholder_data` (`src/templates.rs:2529`) is how the `datetime`, `boolean` and `enum` fills are pinned (`:6363`, `:6391`, `:6422`). It was available and unused for `list`. Asserting the emitted Typst source, as `render/mod.rs:10271-10275` now does for the join, is the other established option.

This is the same defect round 1 raised as BLOCKING 2, in the one place the fix did not reach. AGENTS.md: "A checked box is a claim the next reader trusts instead of redoing the work, so check one only after performing it."

## MAJOR 2: the CSV grid shows `—` for a `list` column and submits its value anyway

`ui/src/components/LabelGrid.tsx:155` renders `—` and `:150` disables editing for a cell whose `cellInput` control is `list`. `ui/src/lib/labelInputs.ts:247-253` still submits that cell: `activeMap.get(k)` finds the `list` input (it is a reported entry, so it is in `activeInputs`), the `!input` skip at `:250` does not fire, and `typeof v === "string"` at `:252` passes the CSV string straight into the request body.

Failure scenario, concrete: a template declares `tags: { type: list }`; the operator pastes a CSV whose header row carries `tags`. `displayedFields` (`ui/src/pages/Import.tsx:128-132`) is `csvFields ∪ requiredUnion`, and `csvFields` alone puts the column on screen even though `requiredUnion` now excludes `list` (`:114-118`). The grid draws `—` in every `tags` cell, refuses to open an editor for them, and Download stays enabled. The batch POST carries `tags: "<csv value>"`, and the server answers `400 InvalidRequest` / `request_body_invalid` (`src/render/mod.rs:157`, pinned at `src/lib.rs:8508-8515`) for the whole batch. Nothing in the grid points at the cells that caused it, and the operator's only remedy is to edit the CSV outside the app.

The regression is not only the 400. Until this change, `—` in that grid meant a field the template does not read, and `pruneDataForSubmit` dropped exactly those (`labelInputs.ts:250`). The marker and the submit rule agreed. They no longer do, and the cell now claims there is no value while sending one.

`ui/src/pages/Connect.tsx` is not affected: `templateFields` excludes `list` before the mapping is built (`:125`), so no connector column ever lands under a list name.

The new Import test cannot catch this: its CSV is `sku\n123\n` (`ui/src/pages/Import.test.tsx:727`), so no `tags` column exists, and `expect(screen.queryByText("tags")).toBeNull()` passes for that reason rather than for the rule under test.

## MINOR 3: `json_to_param_value`'s array arm coerces where design.md asked for a refusal

`src/render/mod.rs:441-450`: a non-string element becomes `other.to_string()` and is pushed into the `ParamValue::List`.

`design.md:301-303` specifies the opposite for this exact function: "The function needs an explicit `Value::Array` arm mapping to `ParamValue::List`, **with a non-string element being the same refusal a request's is**, and the arms of that match are worth reading rather than trusting." `specs/list-params/spec.md` says "The service SHALL NOT coerce an element", and AGENTS.md's Exceptions section says "No silent fallbacks. Everything fails loudly."

It is unreachable today [verified]: the only call site is `render/mod.rs:566`, fed by `resolve_parameter_default`, which coerces through `coerce_param_value` (`:505`) and therefore refuses a non-string element first. So there is no failure scenario to demonstrate, which is why this is MINOR and not more. It is the identical shape round 1 filed as MINOR 6 against the join loop and the implementer fixed there; leaving it here is the same silent stringification the design paragraph exists to warn about, one level down. `unreachable!` or a fallible signature is the spelling that matches the contract.

## MINOR 4: task 5.5 is checked, and two of its three claims have no test

`src/lib.rs:8645-8649` asserts the first: control `list`, resolved array as `default`, `required: false`. The other two named in task 5.5 have no assertion anywhere [verified by grep: `"control"` compared against `"list"` appears once in `src/lib.rs`, at `:8647`]:

- "one declaring no default is `required: true` with no `default`" (delta scenario "An undefaulted list is required", `specs/template-inputs/spec.md:216-219`);
- "an undeclared name is never `list`" (`specs/template-inputs/spec.md:56`).

Both are one assertion each on a response the test already builds.

Related and smaller: the `list-params` scenario "An empty default is a list, not an omission" (`default: []` plus an omitted `tags` renders, and is not `422 MissingField`) has no render-path test. `src/convert.rs:963-971` pins the load-time half (`default: []` is `Some(List([]))`, `default:` is `None`), and the `empty_def_list` thumbnail at `src/lib.rs:8745` cannot distinguish the two, because inventing `["tags"]` for a required entry also renders a 200.

---

Finding 1 must be fixed before this lands: it is a checked task whose stated verification does not exist, in the one behaviour the withdrawn-exception decision (`ANSWERS.md`) added to this change. Finding 2 is a user-reachable dead end the delta does not describe and should be resolved or explicitly ruled out of scope with file:line reasoning.

VERDICT: REVISE
