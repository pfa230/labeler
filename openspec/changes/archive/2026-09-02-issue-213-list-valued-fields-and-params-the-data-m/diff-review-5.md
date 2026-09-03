TREE_SHA256: 0bf1b239bb1669d42883695fa14b5028628a1b1c31b4892dbfbf7d36d16d8c08

## Diff review — `issue-213-list-valued-fields-and-params-the-data-m`

**Gates, run by me [verified]:** `cargo fmt --check` exit 0; `cargo clippy --all-targets --all-features` after `touch src/lib.rs`, zero warnings; `cargo test` 794 passed / 0 failed / 2 ignored plus 3 integration tests; `ui/`: `npm run lint` exit 0, `npm run test` 49 files / 441 tests, `npm run build` exit 0. `.workflow/review-gate-check.sh --plan-only` exit 0.

**Contract conformance:** strong. The two-stage parser (`src/convert.rs:548-676`), the structural grammar (`src/interpolation.rs:104-137`), the six load refusals (`src/templates.rs:1399`, `:1420-1424`, `:1477-1540`, `:1621-1625`), request coercion with the `null`/`[]` split (`src/render/mod.rs:146-159`, `:256-296`), the join render and the array-in-scalar-slot refusals (`src/render/helpers.rs:135-175`, `src/render/mod.rs:1719`, `:2206`), and uniform `InputControl::List` reporting with the one-element placeholder fill (`src/templates.rs:192-196`, `:431`) all hold. `{sys.now:join}` still resolves as a `datetime_formats` name (`src/interpolation.rs:105-107`), so no stored setting is stranded. Diff-review-4's BLOCKING 1 is genuinely fixed: deleting the `list` branch of `pruneDataForSubmit` (`ui/src/lib/labelInputs.ts:251-256`) now fails `ui/src/pages/Import.test.tsx:752` [verified by mutation in a copy outside the worktree].

---

## BLOCKING 1 — `Import.tsx`'s row-validation list guard is protected by an assertion that cannot fail

`ui/src/pages/Import.tsx:154` (`if (input.control === "list") continue;`) keeps a required `list` input from being reported as a missing field on every CSV row. Its sibling in `ui/src/pages/Connect.tsx:177` is covered; this one is not.

Verified by mutation in a copy outside the worktree [verified]:

| mutation | result |
| --- | --- |
| delete `Import.tsx:154` | `Import.test.tsx` **27 passed** |
| delete `Connect.tsx:177` | `Connect.test.tsx` **1 failed** at "skips list inputs in field mapping and grid columns" |

The reason the Import test cannot fail is its fixture: `ui/src/pages/Import.test.tsx:734` pastes `"sku,tags\n123,red;blue\n"`, so `row.data.tags` is non-empty and the required-field check passes with or without the guard. Editing only that CSV to `"sku\n123\n"` (the ordinary case — the operator's CSV has no column for a control no screen draws) makes the guard load-bearing and observable [verified]:

- unmutated + CSV without the column → **1 passed**
- mutated + CSV without the column → **failed** at `Import.test.tsx:743`, `expect(download).toBeEnabled()`

**Failure scenario:** a template declares `tags: { type: list }` with no default; an operator pastes a CSV with only the columns the grid shows. If this branch regresses, every row is flagged as missing `tags`, Download and Print are disabled, and the import is blocked outright — with the suite green.

Tasks 6.3 and 6.4 are checked over this. AGENTS.md: "A checked box is a claim the next reader trusts instead of redoing the work, so check one only after performing it." The fix is one fixture: assert the no-column case, either by changing the pasted CSV or by adding a second `it(...)` beside it.

---

## MAJOR 2 — Half of task 4.6 (the measurement-pass `image name:` guard) has no test

Task 4.6 asks for the array refusal at **both** `image` `name:` bindings, "the measure pass and the render pass", and the `interpolation-tokens` delta requires it "decided **before** the data URI is parsed". Both guards exist (`src/render/mod.rs:1719-1721` in `intrinsic`, `:2206-2208` in `render_image_item`), but only the second is exercised.

The only test that feeds an array to an `image` `name:` is `src/lib.rs:8500-8524`, whose template is:

```yaml
  - type: image
    name: logo
    at: [0, 0]
    size: [50, 20]
```

A fixed `size:` demands no intrinsic, so `intrinsic`'s image arm returns at `src/render/mod.rs:1700-1702` (`if !demands[0] && !demands[1] { return Ok(...) }`) before reaching the guard at `:1719`. `field_value_not_scalar` appears in exactly two HTTP assertions (`src/lib.rs:8472`, `:8523`), neither of which is the measure pass [verified by grep].

**Failure scenario:** an `image` sized `[content, content]` bound by `name: logo`, with `data: { "logo": ["data:image/png;base64,…"] }`. The correct answer is `422 UnsupportedLayoutItem` / `field_value_not_scalar`; delete `:1719-1721` and it becomes `image_data_invalid` about a data URI the caller never wrote — the exact outcome `design.md:163-165` says the guard exists to prevent — and every test still passes. One extra template in the existing test closes it.

---

## MINOR 3 — Stringly-typed error sentinel between `coerce_param_value` and its caller (carried from diff-review-4, unfixed)

`src/render/mod.rs:153` returns `Err(format!("position {idx}"))`, recovered at `:283` with `strip_prefix("position ")`. Any future coercion error text beginning with `"position "` silently changes the message shape. The same string reaches `ParamDefaultFailure`'s `value` field via `resolve_parameter_default_candidate` (`:507-512`), where a `param_default_unresolvable` report would read `value: "position 0"` — unreachable today, since a non-string element in a `list` default is refused at load (`src/convert.rs:640-649`).

## MINOR 4 — `sys.now` + join returns the wrong error kind (unreachable; carried from diff-review-4)

`src/render/helpers.rs:118-120` answers `Reader::Join` on `sys.now` with `field_value_not_scalar`, a reason about arrays for a value that is never one. Unreachable: `validate_interpolated_string` refuses `{sys.now:join(...)}` at load (`src/templates.rs:1477-1490`) at all four sites that reach `interpolate` (`src/render/mod.rs:474`, `:1387`, `:1705`, `:2192`).

## MINOR 5 — A spec scenario on the batch path still has no test (carried from diff-review-4)

`interpolation-tokens` requires each label that reached an unusable array to appear in `details.failures` with `UnsupportedLayoutItem` / `field_value_not_scalar`. The only new batch test (`src/lib.rs:8532-8600`) covers the declared-list `InvalidRequest` case. The behaviour is correct — `src/batch.rs` captures any per-label `AppError` — so this is coverage, not a defect.

## Notes

- `src/render/mod.rs:441-449` uses `panic!` where `design.md:301-303` says the non-string element should be "the same refusal a request's is". Unreachable — `json_to_param_value` is only ever handed `coerce_param_value`'s output (`:568`), and the `List` arm at `:147-158` admits only string elements — and it matches the `unreachable!()` idiom already in the file, so no change is needed; the design sentence is what is slightly off, not the code.
- `src/openapi.rs` is unmodified, contrary to tasks 1.6 and 5.1 and the proposal's Impact list. It is nonetheless correct: `InputControl`, `ParamType` and `ParamValue` are registered as whole schemas (`src/openapi.rs:108`, `:117-118`), so the derived `ToSchema` carries the new variants.
- I made no edits; the two mutation experiments ran on copies under `/tmp`, since deleted, and `git status` is unchanged from the start of this review.

Finding 1 must be fixed before this lands: a load-bearing guard whose named test I confirmed by mutation cannot fail, in the same file and on the same class of defect a prior round rejected. Finding 2 is a second checked box that no test stands behind.

VERDICT: REVISE
