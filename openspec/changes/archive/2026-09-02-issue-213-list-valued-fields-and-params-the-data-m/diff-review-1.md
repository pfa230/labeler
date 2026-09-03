TREE_SHA256: 596ca9d0d8ee2c994dcbaaccf3cdcd4578c14adb19e41225a671adc8d83a7e6e

# Adversarial diff review — `issue-213-list-valued-fields-and-params-the-data-m`

Gates run locally, all green: `cargo fmt --check` (0), `cargo clippy --all-targets --all-features` (0 warnings), `cargo test` (0 failures), `npm run lint`, `npm run test` (49 files / 440 tests). `specs-digest.sh` recomputes `b5c04696…` matching `review.md`'s `SPECS_SHA256`. [verified]

That is what makes the findings below worth stating: nothing here is caught by a gate.

---

## BLOCKING 1 — The print form deadlocks on a required `list` input, contradicting the delta's own stated consequence

`ui/src/pages/print/PrintForm.tsx:112-118` computes `valid` over **every** reported input with `required: true`, including `control: "list"`. `ui/src/components/ParamInput.tsx:252-254` now renders nothing for that control, so no value can ever be entered. `valid` is therefore permanently false, and it gates Download (`PrintForm.tsx:227`), Print (`:282`) and the live preview (`:126`).

For a template declaring `tags: { type: list }` with no default (`required: true`, per `templates.rs:437-443`), the print screen shows a bare label with no widget and both buttons disabled forever.

The `template-inputs` delta states the opposite outcome as the contract:

> `specs/template-inputs/spec.md:101-104` — "until #318 lands a `list` parameter is suppliable only by an API caller, so **a print screen for a template reading one submits without it and the render is `422 MissingField` naming a field it showed no control for**."

The screen cannot submit at all, so that `422` is unreachable. The same gate in the other two screens *was* fixed (`Import.tsx:114-116`, `Connect.tsx:151-153` exclude `list` from `requiredUnion`, and both new tests assert Download stays enabled), which is what shows this is an omission rather than a decision. Task 6.4's `FieldForm.test.tsx:277-287` asserts only that the control is absent; it never asserts the form can still be submitted, so the test written to catch this cannot.

## BLOCKING 2 — Nothing anywhere asserts what a join renders

Task 4.4 is checked. `src/render/helpers.rs:145-163` is the whole feature, and no test observes its output.

- `src/lib.rs:8206-8290` renders four list payloads and asserts `status == OK` only.
- `src/lib.rs:8540-8680` renders three thumbnails and asserts `200` / `content-type: image/png` only.
- `interpolate`'s own unit tests assert exact strings (`src/render/helpers.rs:1980-2000`, e.g. `assert_eq!(out, "https://h/i/A1")`) and received no join case.
- Render tests routinely assert the emitted Typst source (`src/render/mod.rs:2588`, `:7877`, `:8087`), so the established, cheap way to pin this was available and unused.

Delete the separator, reverse the elements, or emit the raw JSON array, and every test in the repository still passes. The spec scenarios left unverified are concrete: "the label reads `A, B`", "`{codes:join('|')}` … reads `1|true`" (`specs/list-params/spec.md`), "the thumbnail renders and reads `tags`", "the thumbnail reads `CONSUMABLE, KIDS`", "renders with that text empty" (`specs/template-inputs/spec.md:463-478`).

## BLOCKING 3 — The `string`-array refusal, one of the change's two BREAKING claims, is untested at the level the spec states it

`src/render/mod.rs:140-146` makes a `string` parameter refuse a JSON array, and `:348-353` is the new strict-mode arm producing `400 InvalidRequest` / `request_body_invalid` / `parameter '{name}' is not a valid string`. Neither the message, the code nor the reason is asserted by any test.

The test added for task 4.3, `param_types_refuse_array_values` (`src/render/mod.rs:10224-10261`), calls `coerce_param_value` directly and asserts `res.is_err()`. That function returns a bare `String`; it carries no status, code or reason. So task 4.3's literal requirement — "pinning that `boolean`, `integer`, `number`, `length`, `enum` and `datetime` keep exactly **the code, reason and message** they refuse an array with today" — is unmet for all six types, and the seventh (`string`), the only one whose behaviour this change alters, has no test reaching `resolve_parameters_mode` or HTTP at all.

The nearest-looking test, `src/lib.rs:8419-8435`, sends `title: ["A","B"]` against `scalar_tpl`, which declares **no `params:` block**, so it exercises the undeclared render-time `field_value_not_scalar` path instead. `datetime` is the one type genuinely pinned, by the pre-existing `src/lib.rs:1667-1695`.

Spec scenario left unverified: `specs/list-params/spec.md` — "A string parameter no longer stringifies an array → `400 InvalidRequest` with `details.reason` `request_body_invalid` naming `title`".

## MAJOR 4 — `ui/src/app/toast.tsx` carries an unrelated, unplanned change

`ui/src/app/toast.tsx:11-18,24` adds a timer-tracking ref, an unmount cleanup effect, and an early `if (typeof window === "undefined") return;` inside `dismiss`. No task mentions this file, and the proposal's Impact section scopes the UI to `types.ts` (later widened by tasks 6.1-6.3 to `ParamInput`, `Import`, `Connect`, `LabelGrid` — never to `toast.tsx`). It is a behaviour change to a shared component that no plan review and no delta covers, against AGENTS.md's "Keep changes minimal and focused on what was requested" and "Don't refactor adjacent code unless asked". Revert it, or file it as its own issue.

(`ui/src/lib/labelInputs.ts:252-253` is also outside the tasks, but it is a direct consequence of `ParamValue` admitting `string[]` and is defensible; noting it only so the omission is deliberate.)

## MINOR 5 — Task 3.7's regression pins for `when: {}`, a blank key and a blank value were not written

The refusals live at `src/templates.rs:2313` and `:2317`. Grepping the whole tree finds no test asserting either message — the new `list_parameter_load_refusals_quarantine_files` covers only `when: null` and the undeclared key. Task 3.7 is checked; half of what it names is not there.

## MINOR 6 — A silent coercion in the join loop

`src/render/helpers.rs:158`: `other => joined.push_str(&value_to_string(other))`. Unreachable today, since `coerce_param_value` refuses a non-string element. But `specs/list-params/spec.md` says "The service SHALL NOT coerce an element", and AGENTS.md's Exceptions section says "No silent fallbacks. Everything fails loudly." If a future path ever puts a number in that array it prints, rather than failing. `unreachable!` or an error is the spelling that matches the contract.

## MINOR 7 — Two `list-params` scenarios have no test

"A list cannot drive a dimension" and "A list cannot drive a colour". `src/templates.rs:1399` adds the `List` arm to `check_param_ref`, and no call site (`:1046`, `:1055`, `:1063`, `:1564`, `:1567`, `:1572`, `:1593`, `:1633`, `:1650`, `:1667`, `:1670`, `:1675`) passes `"list"` in `allowed_types`, so the refusal is correct by construction. But the eleven-file registry test covers every other new refusal and skips these two, which makes the omission look like an oversight rather than a judgement that construction suffices.

---

Findings 1-3 must be fixed before this lands: one is a broken screen the delta explicitly describes working, and two are checked tasks whose stated verification does not exist.

VERDICT: REVISE
