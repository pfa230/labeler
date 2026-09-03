TREE_SHA256: 0afb3ef6dbf65f4ad4609c7318deb907b894bf4e6550d04be9f7eb61a25fe213

## Diff review — `issue-213-list-valued-fields-and-params-the-data-m`

**Gates, run by me [verified]:** `cargo fmt --check` exit 0; `cargo clippy --all-targets --all-features` after `touch src/lib.rs`, zero warnings; `cargo test` 794 passed / 0 failed / 2 ignored plus 3 integration tests; `ui/`: `npm run lint` exit 0, `npm run test` 49 files / 441 tests, `npm run build` (`tsc -b && vite build`) exit 0.

**Contract conformance:** the engine side matches the delta closely. The two-stage parser (`src/convert.rs:547-666`), the structural token grammar (`src/interpolation.rs:104-137`), the six load refusals (`src/templates.rs:1415-1420`, `:1478-1530`, `:1621-1625`, `:1399`), request coercion and the `null`/`[]` distinction (`src/render/mod.rs:146-159`, `:253-295`), the join render and the array-in-scalar-slot refusal (`src/render/helpers.rs:135-175`, `src/render/mod.rs:1719-1721`, `:2206-2208`), and the uniform `InputControl::List` reporting with the one-element placeholder fill (`src/templates.rs:192-196`, `:431`) all hold. `check_param_ref` has no call site passing `"list"`, so every numeric/colour reference is refused. `{sys.now:join}` still resolves as a format name, so no stored `datetime_formats` entry is stranded.

---

## BLOCKING 1 — `pruneDataForSubmit`'s list guard is still protected by an assertion that cannot fail

This is the unfixed half of diff-review-3's BLOCKING 1, which named both `Import.tsx:139` and `labelInputs.ts:251-256`. The first half is now genuinely covered; the second is not.

`ui/src/lib/labelInputs.ts:251-256` is load-bearing: `Import.tsx:234` copies **every** CSV column into `row.data`, including one named after a `list` parameter. `listNames` only removes the column from the grid; the value stays in the row and reaches the submit body.

Verified by mutation in a copy outside the worktree [verified]:

| mutation | `Import.test.tsx` result |
| --- | --- |
| delete the `listNames` filter (`Import.tsx:139`) | **fails** at `:744` — this half is now covered |
| delete the `list` branch in `pruneDataForSubmit` | **27 passed** — unchanged |

With that branch deleted I dumped the actual `/api/batch` body from `fetchMock.mock.calls`:

```
{"template":"t1","labels":[{"data":{"sku":"123","tags":"red;blue"}}],"mode":"download"}
```

versus, with the guard present:

```
{"template":"t1","labels":[{"data":{"sku":"123"}}],"mode":"download"}
```

The guard demonstrably changes the request, and the test passes identically either way.

**Root cause:** the only assertion on this is `expect(label.data.tags).toBeUndefined()` inside the `fetch` mock (`ui/src/pages/Import.test.tsx:722-724`). An `expect` failure there rejects the fetch promise, the page catches it, and the test's remaining assertions — including the trailing `waitFor(... includes("/api/batch"))`, which only checks that the call was *made* — still hold. Assert on `fetchMock.mock.calls` after the click instead, where a failure is the test's own.

**Failure scenario:** a template declares `tags: { type: list }`; an operator pastes `sku,tags\n123,red;blue\n`. If this branch regresses, the batch POST carries `tags: "red;blue"`, the server answers `400 InvalidRequest` / `request_body_invalid` for the whole batch (`src/render/mod.rs:158`), and no grid cell points at the cause — with the suite green.

Tasks 6.3 and 6.4 are checked over this. AGENTS.md: "A checked box is a claim the next reader trusts instead of redoing the work, so check one only after performing it."

---

## MINOR 2 — A spec scenario on the batch path has no test

`interpolation-tokens`' "A batch names every label that reached an unusable array" requires each such label in `details.failures` with `UnsupportedLayoutItem` / `field_value_not_scalar`. The only new batch test (`src/lib.rs:8604-8628`) covers the declared-list `InvalidRequest` case. The behaviour is correct — `src/batch.rs:105-119` captures any per-label `AppError`, render-time included — so this is coverage, not a defect. Task 4.8 asked only for the case that was written.

## MINOR 3 — Stringly-typed error sentinel between `coerce_param_value` and its caller

`src/render/mod.rs:153` returns `Err(format!("position {idx}"))`, which `:283` recovers with `strip_prefix("position ")`. Any future coercion error text beginning with `"position "` silently changes the message shape. The same string also reaches `ParamDefaultFailure`'s `value` field via `resolve_parameter_default_candidate` (`:517-524`), where a `param_default_unresolvable` report would read `value: "position 0"` — unreachable today, since a non-string element in a `list` default is refused at load (`src/convert.rs:736-744`).

## MINOR 4 — `sys.now` + join returns the wrong error kind (unreachable)

`src/render/helpers.rs:118-120` answers a `Reader::Join` on `sys.now` with `422 UnsupportedLayoutItem` / `field_value_not_scalar`, a reason about arrays for a value that is never one. Unreachable: `validate_interpolated_string` refuses `{sys.now:join(...)}` at load in all four sites it governs (`src/templates.rs:1479-1489`, reached for defaults at `:1034` and for `text`/`qr`/`image src` at `:1562`, `:1589`, `:1628`). Cosmetic only.

## Note

`src/openapi.rs` is unmodified, contrary to tasks 1.6/5.1 and the proposal's Impact list. It is nonetheless correct: `InputControl`, `ParamType` and `ParamValue` are registered as whole schemas (`src/openapi.rs:108`, `:117-118`), so the derived `ToSchema` carries the new variants. No change needed.

---

Finding 1 must be fixed before this lands: a load-bearing guard whose named test I confirmed by mutation cannot fail, on the exact lines a prior round rejected for the same reason.

VERDICT: REVISE
