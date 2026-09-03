TREE_SHA256: 2b25e0066c5e90dda15a37a5ace85eb7ce601b8f91faf53331b7a455146f1eca

## Diff review, `issue-213-list-valued-fields-and-params-the-data-m`

**Gates, run by me in this worktree [verified]:** `cargo fmt --check` exit 0; `cargo clippy --all-targets --all-features` exit 0, zero warnings; `cargo test` 797 passed, 0 failed; `ui/`: `npm run lint` exit 0, `vitest run` 49 files / 442 tests passed, `npm run build` exit 0. `.workflow/specs-digest.sh` recomputes `42e3190b…`, matching `review.md`'s `SPECS_SHA256`. `git status` unchanged from the start of this review; I edited nothing.

**Prior round's two blocking items are genuinely closed [verified].**
- diff-review-5 BLOCKING 1 (Import list guard behind an assertion that cannot fail): `ui/src/pages/Import.test.tsx:756-812` now adds the no-column case (`"sku\n123\n"`), which is the fixture that makes `Import.tsx:154` load-bearing.
- diff-review-5 MAJOR 2 (measure-pass `image name:` guard untested): `src/lib.rs:8526-8560` adds `img_content_tpl` with `size: [content, content]`, which reaches `intrinsic`'s guard at `src/render/mod.rs:1719` rather than short-circuiting at `:1700`.

**Contract conformance.** All six refusals hold and are reached through the registry and through `PUT`: the `when` list refusal with a layout path (`src/templates.rs:1420-1424`, path threaded at `:1081` and `:1685`), the `image name:` refusal (`:1621-1625`), the bare-token, bare-reader and join refusals (`:1477-1545`), and `check_param_ref` (`:1399`). The grammar is structural, not colon-counting (`src/interpolation.rs:104-137`), and every token the old rule accepted still parses identically, so `{sys.now:join}` remains a `datetime_formats` name and no stored setting is stranded. Request coercion keeps the `null`-is-omission / `[]`-is-a-value split (`src/render/mod.rs:256-266`), the join renders with no separator before the first or after the last element (`src/render/helpers.rs:157-170`), and array-in-scalar-slot is `422` / `field_value_not_scalar` at the token, at both `image name:` bindings, and per label in a batch for both the single and the sheet paths (`src/batch.rs:105-119`, `src/render/mod.rs:1030-1048`). A list `default:` reaches the model uninterpolated (`src/render/mod.rs:500`), and `{tags:join(', ')}` in a `default:` is refused by the existing bare-token rule at `src/templates.rs:1025-1033` before `validate_interpolated_string` sees it, which is the delta's "a default cannot carry a join" scenario. The three `openspec/specs/` requirements the delta renames or modifies all exist under the exact names the delta uses, so archive will resolve them.

---

## MINOR 1, non-blocking: `{tags:long_date}` on a declared list gets the join message, not the instant message its scenario asks for

`src/templates.rs:1504-1509` returns for **any** bare reader name on a declared list:

> `template contains '{tags:long_date}': a list parameter is read through join('<separator>')`

The delta's scenario at `openspec/changes/issue-213-.../specs/interpolation-tokens/spec.md:156-160` says that token's message shall name the token "and stat[e] that a format applies to an instant only". It does not; the instant clause at `:1520-1523` is unreachable once `is_declared_list` is true.

**Failure scenario:** a template declaring `tags: { type: list }` with `value: "{tags:long_date}"` is quarantined with a message that omits the sentence the scenario requires. The refusal itself, its timing, the named token and the quarantine are all correct; only the explanation differs.

I am not raising this as blocking because the requirement body it sits under contradicts the scenario and the code follows the body: `spec.md:44-50` says the rule covering "`{tags:<name>}` naming one for any bare reader name" shall report "that a list is read through `join('<separator>')` rather than only that a format applies to an instant". The scenario reads as a carry-over from the pre-list version of the same scenario. Reconciling it is a one-line edit to `specs/`, which would void the approving plan verdict's digest, and the behavioural cost of leaving it is one sentence of diagnostic text. Recording it here rather than acting on it.

## Carried minors, unchanged and still unreachable

diff-review-4/5's MINOR 3 (the `"position {idx}"` string sentinel between `src/render/mod.rs:153` and `:283`) and MINOR 4 (`sys.now` plus `Reader::Join` answering `field_value_not_scalar` at `src/render/helpers.rs:118-120`) are both still present. I re-verified both are unreachable: `json_to_param_value`'s only call site is `src/render/mod.rs:568` on `resolve_parameter_default`'s already-coerced output, whose `List` arm at `:147-158` admits string elements only, so the `panic!` at `:441-449` cannot fire; and `validate_interpolated_string` refuses `{sys.now:join(...)}` at load before any of the four sites that call `interpolate`. Neither is a defect I can demonstrate.

## Notes

- `src/openapi.rs` is unmodified although tasks 1.6 and 5.1 name it. Correct as-is: `InputControl`, `ParamType` and `ParamValue` are registered as whole schemas (`src/openapi.rs:108`, `:117-118`), so the derived `ToSchema` carries the new variants; the HTTP test at `src/lib.rs:8645-8656` pins the serialized shape.
- `FieldForm` still renders a list entry's name header and, where one has a default, its "Use default" checkbox, with `ParamInput` returning `null` beneath it (`ui/src/components/ParamInput.tsx:252-254`). That satisfies the `template-inputs` obligation to omit the control without failing; the empty labelled row is #318's to fill.
- No `#[allow(clippy::...)]` anywhere in the diff; nothing outside `src/` and `ui/src/` is touched; `.agent-runs/` and `ANSWERS.md` are gitignored, so the change folder is all a `git add -A` would stage.

VERDICT: APPROVE
