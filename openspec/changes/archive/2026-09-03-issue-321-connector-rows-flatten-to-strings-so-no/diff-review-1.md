TREE_SHA256: 64a2d9f44e7b895947c89c5cd69f20fe07f2a8e76cde490c499a7a8d8f5d12da

# Diff review: issue-321 (connector rows carry lists)

Gates run locally: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features` clean, `cargo test` green, `npm run lint` clean, `npm run build` clean. `npm run test` is **not** reliably green (finding 1).

## Blocking

### 1. The new batch-submit test in `Connect.test.tsx` fails nondeterministically (~38% of runs)

`ui/src/pages/Connect.test.tsx:691-692` clicks Download and waits for the mocked `POST /api/batch`. The click frequently lands while `useBatchRowInputs` is still resolving, so `run()` bails at `ui/src/pages/Connect.tsx:256` (`if (rowsPending) { setFormError("Resolving row inputs; please wait."); return; }`) and no batch is ever submitted.

Evidence [verified]: in an instrumented copy of the tree, the body text at the moment `waitFor` times out reads `...PrintDownload2 labelsResolving row inputs; please wait....`, i.e. the bail branch fired. Failure rate measured 6/16 isolated runs of that file, plus 1 failure in a full `vitest run` (452 tests) and 1 in 8 earlier isolated runs. `ui/package.json:12` maps `test` to `vitest run` and `.github/workflows/ci.yml:94` runs it, so this reds the branch run intermittently.

`await screen.findByRole("grid", ...)` is not a sufficient barrier: the grid renders as soon as `commitRows` lands, while the `/inputs` resolution is a `setTimeout(0)` plus a fetch (`ui/src/lib/labelInputs.ts:208`, `:234`). The test needs to wait on inputs having resolved (or retry the click) before asserting the submission. Task 8.3 is checked, but the suite it names does not pass reliably.

### 2. `defaultMapping` and `validateMapping` keep a second spelling for their arguments, half of it dead on arrival

`ui/src/lib/connectorRows.ts:17-42` and `:44-84` both take `(InputSpec | string)[]` and `(FieldSpec | string)[]`, branching on `typeof c === "string"` to fabricate `multi_valued: false` (`:24`, `:52`).

- The only production caller passes objects (`ui/src/pages/Connect.tsx:127`, `:129`).
- The string branch of `defaultMapping` survives solely to keep one pre-existing test compiling (`ui/src/lib/connectorRows.test.ts:6`).
- The string branch of `validateMapping` is brand-new code with **no caller at all**, test or production.

AGENTS.md, "Breaking changes, until 1.0": "a change that alters behavior breaks what came before, and that is the finished job. No migration, no desugaring, no deprecation window, no second spelling." Widening these signatures so the old call shape still type-checks is that second spelling, and it buys one un-updated test line.

The `?? false` fallbacks at `:26`, `:54`, `:70` and `:71` compound it: `FieldSpec.multi_valued` is a required boolean (`ui/src/api/connectors.ts:35`), so the coalesce can only mask a schema that failed to carry the key, which is exactly what the spec requirement "a reader SHALL never have to infer a column's cardinality from its absence" exists to prevent. That is a silent fallback over a real absence.

## Non-blocking

### 3. Single-element lists in `number` / `money` / `date` columns contradict the delta

`specs/connector-browser/spec.md` states "a multi-valued cell in a `number`, `money` or `date` column is uninterpretable as that type and orders with the blanks." The implementation interprets the *display text*, so `numberKey(["5"])` returns `5` (`ui/src/lib/connectorSort.ts:21-29`) and `dateKey(["2026-01-05"])` parses as a date (`:34-41`). Both sort as real values, not with the blanks.

The delta says both things: "compared by its display text... with no rule of its own" and the uninterpretable claim. The code honours the first; the spec's second sentence is only true for multi-element lists, which is the case design.md reasoned about (`Number("1, 2")` is `NaN`). The tests at `ui/src/lib/connectorSort.test.ts:230-247` use two-element lists, so they do not exercise the divergence. Unreachable today (the only multi-valued column is `tags`, `ty: text`), but the spec sentence and the code disagree and one of them should move.

### 4. `extract_field`'s `tags` arm is resource-blind

`src/connector/homebox.rs:590` matches the key `"tags"` for every resource. `tags` is declared on `entities` only, so materializing `fields: ["tags"]` for a `locations` row now returns `[]` where it previously returned `""` via the catch-all. `connector-multi-valued-fields` says an array-valued upstream key "that no column declares SHALL keep the answer it gives today, which for materialize is the empty string." Reachable: `ui/src/pages/Connect.tsx:124-125` unions column keys across all resources, so a `tags` mapping can be applied to selected location rows. Harmless in effect, but it is a deviation, and `extract_field` has no resource parameter with which to honour a per-resource declaration.

### 5. The `ColumnDef` to `FieldSpec` conversion is now inlined three times

`src/connector/homebox.rs:251-258`, `:276-283` and `:922-929` each repeat the same six-field literal, while `field()` (`:494-502`) survives with a hardcoded `multi_valued: false` for the dynamic `custom:` columns only. A `From<&ColumnDef> for FieldSpec` (or a `multi_valued` parameter on `field()`) would keep one conversion. As it stands the test at `:922-929` builds its `expected` with a copy of the production mapping, so it cannot catch a wrong `multi_valued` on a `ColumnDef`; the HTTP tests in `src/lib.rs` carry that load instead, which is fine, but the unit assertion is now self-mirroring.

### 6. Coverage lost when the old list test was rewritten rather than extended

The replaced test declared the list input as `{ name: "tags", control: "list", required: true }` and asserted the Download button stayed enabled, pinning "a required `list` parameter left unmapped still fails at submit rather than in the grid" (design.md, Risks). Both new tests drop `required` (`ui/src/pages/Connect.test.tsx:627`, `:654`, `:657`), so nothing now covers that a required list input does not block the grid. design.md calls this behaviour out explicitly as accepted-and-unchanged; it should still have a test.

### 7. `displayCellText(value: unknown)` is looser than its contract

`ui/src/lib/connectorRows.ts:6` takes `unknown` where every call site passes `CellValue | undefined` (`ConnectorBrowser.tsx:53`, `connectorFilter.ts:12`, `connectorSort.ts:12`, `:25`, `:37`) or `ParamValue` (`LabelGrid.tsx:162`). Typing it to the union it actually serves would let the compiler catch a future widening of `CellValue`; `unknown` will silently accept anything.

## Verified as correct

`multi_valued` present on every `FieldSpec` with no `skip_serializing_if` (`src/connector/mod.rs:283-288`); byte-identical scalars on both endpoints, with `quantity` asserted as a JSON string on materialize (`src/lib.rs`, `materialize_and_browse_tags_and_undeclared_arrays`); `[]` on browse and materialize for an untagged item, asserted as neither `""`, `null` nor absent; the one-upstream-request assertion made against `hb.received_requests()`; the transform refusal firing before the not-text refusal with a paired positive case; `RowValue` kept distinct from `CellValue` so `data` cannot hold a JSON number; `apply_to_map` matching only `RowValue::Text`; the `tags` capture-name collision now covered by the existing declared-column check; `RowValue` registered in `src/openapi.rs:178`; and `pruneDataForSubmit` passing `[]` through for a `list` control so an untagged item reaches the batch as `{"tags": []}`.

VERDICT: REVISE
